//! Language manifests: the dependencies (for the stack and the stores the code talks to) and the language
//! version. When several languages are present, the server-side one names the stack (Ruby before Node, whose
//! package.json may only build assets).

use std::collections::BTreeMap;

use super::{line_path, line_where};
use crate::stack::{Dep, Eco, version};
use crate::{Facts, at};

pub fn read(f: &mut Facts, files: &BTreeMap<String, String>) {
    let root = |name: &str| files.get(name).map(String::as_str);
    let first_line = |name: &str| root(name).and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()));

    if let Some(t) = root("Gemfile") {
        f.manifest("Gemfile", "gemfile");
        let mut ruby = first_line(".ruby-version").map(|v| v.trim_start_matches("ruby-").to_string());
        for (i, line) in t.lines().enumerate() {
            let words = quoted(line);
            match line.split_whitespace().next() {
                Some("ruby") if ruby.is_none() => ruby = words.first().cloned(),
                Some("gem") if !words.is_empty() => dep(
                    f,
                    Eco::Ruby,
                    &words[0],
                    words.get(1).map(String::as_str),
                    at("Gemfile", i + 1),
                ),
                _ => {}
            }
        }
        f.language(1, "Ruby", ruby.and_then(|v| version(&v, 2)));
    }

    let mut python = None;
    for (name, t) in files
        .iter()
        .filter(|(n, _)| n.starts_with("requirements") && n.ends_with(".txt") && !n.contains('/'))
    {
        f.manifest(name, "requirements");
        python.get_or_insert(None);
        for (i, line) in t.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('-') {
                continue;
            }
            requirement(f, line, at(name, i + 1));
        }
    }
    if let Some(t) = root("pyproject.toml") {
        f.manifest("pyproject.toml", "pyproject");
        let table = t.parse::<toml::Table>().unwrap_or_default();
        let project = table.get("project");
        let requires = project.and_then(|p| p.get("requires-python")).and_then(|v| v.as_str());
        python = Some(requires.map(String::from));
        let line_of = |needle: &str| at("pyproject.toml", line_where(t, 1, |l| l.contains(needle)).unwrap_or(1));
        for spec in project
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(spec) = spec.as_str() {
                requirement(f, spec, line_of(spec));
            }
        }
        let poetry = table
            .get("tool")
            .and_then(|t| t.get("poetry"))
            .and_then(|p| p.get("dependencies"));
        for (name, v) in poetry.and_then(|d| d.as_table()).into_iter().flatten() {
            let spec = v.as_str().or_else(|| v.get("version").and_then(|v| v.as_str()));
            if name == "python" {
                python = Some(spec.map(String::from));
            } else {
                dep(f, Eco::Python, name, spec, line_of(name));
            }
        }
    }
    if let Some(declared) = python {
        let pinned = first_line(".python-version")
            .map(String::from)
            .or_else(|| first_line("runtime.txt").map(|v| v.trim_start_matches("python-").to_string()))
            .or(declared);
        f.language(2, "Python", pinned.and_then(|v| version(&v, 2)));
    }

    if let Some(t) = root("go.mod") {
        f.manifest("go.mod", "gomod");
        let mut go = None;
        let mut block = false;
        for (i, line) in t.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("").trim();
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["go", v] => go = Some(v.to_string()),
                ["require", "("] => block = true,
                [")"] => block = false,
                ["require", m, v, ..] => dep(f, Eco::Go, m, Some(v), at("go.mod", i + 1)),
                [m, v, ..] if block => dep(f, Eco::Go, m, Some(v), at("go.mod", i + 1)),
                _ => {}
            }
        }
        f.language(3, "Go", go.and_then(|v| version(&v, 2)));
    }

    if let Some(t) = root("Cargo.toml") {
        f.manifest("Cargo.toml", "cargo");
        let table = t.parse::<toml::Table>().unwrap_or_default();
        let start = line_where(t, 1, |l| l.trim() == "[dependencies]").unwrap_or(1);
        for (name, v) in table
            .get("dependencies")
            .and_then(|d| d.as_table())
            .into_iter()
            .flatten()
        {
            let spec = v.as_str().or_else(|| v.get("version").and_then(|v| v.as_str()));
            let line = line_where(t, start, |l| l.trim_start().starts_with(name.as_str())).unwrap_or(start);
            dep(f, Eco::Rust, name, spec, at("Cargo.toml", line));
            // a feature names the engine of sqlx and diesel: `sqlx[postgres]`, as `celery[redis]` does for Python
            for feature in v.get("features").and_then(|x| x.as_array()).into_iter().flatten() {
                if let Some(feature) = feature.as_str() {
                    dep(
                        f,
                        Eco::Rust,
                        &format!("{name}[{feature}]"),
                        None,
                        at("Cargo.toml", line),
                    );
                }
            }
        }
        // the crate's own features turn on a dependency's engine: `postgresql = ["diesel/postgres"]`
        let fstart = line_where(t, 1, |l| l.trim() == "[features]").unwrap_or(1);
        let deps: Vec<String> = table
            .get("dependencies")
            .and_then(|d| d.as_table())
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default();
        for (feature, list) in table.get("features").and_then(|x| x.as_table()).into_iter().flatten() {
            for item in list.as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
                if let Some((krate, sub)) = item.split_once('/')
                    && deps.iter().any(|d| d == krate.trim_end_matches('?'))
                {
                    let line =
                        line_where(t, fstart, |l| l.trim_start().starts_with(feature.as_str())).unwrap_or(fstart);
                    dep(
                        f,
                        Eco::Rust,
                        &format!("{}[{sub}]", krate.trim_end_matches('?')),
                        None,
                        at("Cargo.toml", line),
                    );
                }
            }
        }
        f.language(4, "Rust", None);
    }

    if let Some(t) = root("composer.json") {
        f.manifest("composer.json", "composer");
        let v = serde_json::from_str::<serde_json::Value>(t).unwrap_or_default();
        for (name, spec) in v["require"].as_object().into_iter().flatten() {
            if name != "php" {
                let line = line_path(t, &["\"require\"", &format!("\"{name}\"")]).unwrap_or(1);
                dep(f, Eco::Php, name, spec.as_str(), at("composer.json", line));
            }
        }
        f.language(5, "PHP", v["require"]["php"].as_str().and_then(|s| version(s, 2)));
    }

    if let Some(t) = root("mix.exs") {
        f.manifest("mix.exs", "mix");
        let mut elixir = None;
        for (i, line) in t.lines().enumerate() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("elixir:") {
                elixir = quoted(rest).first().cloned();
            } else if let Some(rest) = l.strip_prefix("{:") {
                let name = rest.split([',', '}']).next().unwrap_or("").trim();
                dep(
                    f,
                    Eco::Elixir,
                    name,
                    quoted(rest).first().map(String::as_str),
                    at("mix.exs", i + 1),
                );
            }
        }
        f.language(6, "Elixir", elixir.and_then(|v| version(&v, 2)));
    }

    if let Some(t) = root("package.json") {
        f.manifest("package.json", "package");
        let v = serde_json::from_str::<serde_json::Value>(t).unwrap_or_default();
        for (name, spec) in v["dependencies"].as_object().into_iter().flatten() {
            let line = line_path(t, &["\"dependencies\"", &format!("\"{name}\"")]).unwrap_or(1);
            dep(f, Eco::Node, name, spec.as_str(), at("package.json", line));
        }
        let node = v["engines"]["node"].as_str().map(String::from).or_else(|| {
            first_line(".nvmrc")
                .or_else(|| first_line(".node-version"))
                .map(|s| s.trim_start_matches('v').to_string())
        });
        f.language(7, "Node", node.and_then(|v| version(&v, 1)));
    }
}

fn dep(f: &mut Facts, eco: Eco, name: &str, spec: Option<&str>, at: String) {
    f.deps.push(Dep {
        eco,
        name: name.to_string(),
        version: spec.and_then(|s| version(s, 1)),
        at,
    });
}

/// A PEP 508 line: `celery[redis]==5.4.0`. Each extra is also a dependency (`celery[redis]`) so the stack can
/// tell that Celery talks to Redis.
fn requirement(f: &mut Facts, spec: &str, at: String) {
    let end = spec
        .find(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c)))
        .unwrap_or(spec.len());
    let name = spec[..end].to_ascii_lowercase().replace('_', "-");
    if name.is_empty() {
        return;
    }
    let rest = &spec[end..];
    let mut version_part = rest;
    if let Some(extras) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
        for extra in extras.0.split(',') {
            dep(f, Eco::Python, &format!("{name}[{}]", extra.trim()), None, at.clone());
        }
        version_part = extras.1;
    }
    let version_part = version_part.split(';').next().unwrap_or("");
    dep(f, Eco::Python, &name, Some(version_part), at);
}

/// The strings in double or single quotes on a line, in order.
fn quoted(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(['"', '\'']) {
        let q = rest[start..].chars().next().unwrap_or('"');
        let tail = &rest[start + 1..];
        let Some(end) = tail.find(q) else { break };
        out.push(tail[..end].to_string());
        rest = &tail[end + 1..];
    }
    out
}
