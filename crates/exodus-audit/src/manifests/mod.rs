//! Manifest readers: platform files give the processes, the data they provision, env names and the platform;
//! language manifests give the dependencies and the language version. Files at the repository root, then the
//! shapes the root does not show (`shapes`: Firebase, Vercel, a static site, the deployable folders of a monorepo).

mod compose;
mod fly;
mod heroku;
mod langs;
mod railway;
mod render;
mod shapes;

pub use shapes::STAYS;

use std::collections::BTreeMap;

use exodus_schema::{Platform, Process};

use crate::{Facts, at, scan};

pub fn read(f: &mut Facts, files: &BTreeMap<String, String>) {
    if let Some(t) = files.get("Dockerfile") {
        dockerfile(f, t);
    }
    langs::read(f, files);
    if let Some(t) = files.get("Procfile") {
        procfile(f, t);
    }
    for name in ["railway.json", "railway.toml"] {
        if let Some(t) = files.get(name) {
            railway::read(f, name, t, files);
        }
    }
    for name in ["render.yaml", "render.yml"] {
        if let Some(t) = files.get(name) {
            render::read(f, name, t);
        }
    }
    if let Some(t) = files.get("fly.toml") {
        fly::read(f, t);
    }
    if let Some(t) = files.get("app.json") {
        heroku::read(f, t);
    }
    // The base compose file first, so it names the processes; a compose folder (`docker/`) when the root has none
    // and no platform file says how the app is deployed (a compose file in a folder beside one is a dev or test
    // setup, never the production process list).
    let mut composes: Vec<(&String, &String)> = files
        .iter()
        .filter(|(n, _)| !n.contains('/') && compose::is_compose(n))
        .collect();
    if composes.is_empty() && f.platforms.is_empty() {
        let placer = crate::place::Placer::new(files);
        composes = files
            .iter()
            .filter(|(n, _)| {
                n.split_once('/').is_some_and(|(_, name)| compose::is_base(name)) && placer.off_server(n).is_none()
            })
            .collect();
    }
    composes.sort_by_key(|(n, _)| !compose::is_base(n.rsplit('/').next().unwrap_or(n)));
    for (name, t) in composes {
        compose::read(f, name, t, files);
    }
    for (_, t) in files.iter().filter(|(n, _)| !n.contains('/') && scan::is_env_sample(n)) {
        env_sample(f, t);
    }
    let projects = shapes::read(f, files);
    // Nothing else named a process: try these in order, until one does.
    for fallback in [fallback_process, shapes::folders, shapes::hosted] {
        if f.processes.is_empty() && !f.hosted_root {
            fallback(f, files);
        }
    }
    f.processes.extend(projects);
}

/// The 1-based line of the first line at or after `from` (1-based) that `pred` accepts.
fn line_where(text: &str, from: usize, pred: impl Fn(&str) -> bool) -> Option<usize> {
    text.lines()
        .enumerate()
        .skip(from.saturating_sub(1))
        .find(|(_, l)| pred(l))
        .map(|(i, _)| i + 1)
}

/// The line of the last needle, each searched after the previous one: `["\"deploy\"", "\"startCommand\""]`.
fn line_path(text: &str, needles: &[&str]) -> Option<usize> {
    needles
        .iter()
        .try_fold(1, |from, n| line_where(text, from, |l| l.contains(n)))
}

fn platform(f: &mut Facts, rank: u8, name: &str, file: &str, sleep: Option<exodus_schema::Sleep>) {
    f.platforms.push((
        rank,
        Platform {
            name: name.into(),
            file: file.into(),
            sleep,
        },
    ));
}

fn process(id: &str, kind: &str, name: &str, command: &str, from: String) -> Process {
    Process {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        command: crate::redact::command(command),
        from,
        always_on: kind != "release" && kind != "cron",
        schedules: None,
        framework: None,
    }
}

/// A command that only fires jobs on a schedule, whatever its process is called (`worker-beat: celery -A app beat`).
fn scheduler(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let celery_beat = words.iter().any(|w| w.ends_with("celery")) && words.contains(&"beat");
    celery_beat
        || words
            .iter()
            .any(|w| matches!(*w, "rqscheduler" | "rq-scheduler" | "clockwork" | "sidekiq-scheduler"))
}

/// `web: node server.js` per line; the type names the kind, else the command (a Celery beat is a scheduler).
fn procfile(f: &mut Facts, text: &str) {
    f.manifest("Procfile", "procfile");
    platform(f, 4, "Heroku", "Procfile", None);
    for (i, line) in text.lines().enumerate() {
        let Some((name, command)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || name.starts_with('#')
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let kind = match name {
            "web" => "web",
            "release" => "release",
            "clock" | "cron" | "scheduler" | "beat" => "cron",
            _ if scheduler(command) => "cron",
            _ => "worker",
        };
        let mut p = process(name, kind, name, command.trim(), at("Procfile", i + 1));
        // A Procfile clock runs all the time and keeps its own timers.
        p.always_on = kind != "release";
        f.processes.push(p);
    }
}

/// What a Dockerfile's last stage runs: the start command (ENTRYPOINT then CMD, the last of each), its line (else
/// the stage's FROM), and
/// the stage's base image without registry, owner and tag (`caddy`, `node`).
pub struct Docker {
    pub cmd: Option<String>,
    pub line: usize,
    pub image: String,
}

impl Docker {
    /// A last stage on a file server's image serves files: a static site.
    pub fn server(&self) -> Option<&'static str> {
        [
            ("caddy", "Caddy"),
            ("nginx", "nginx"),
            ("httpd", "Apache httpd"),
            ("docker-static-website", "BusyBox httpd"),
        ]
        .iter()
        .find(|(i, _)| self.image.starts_with(i))
        .map(|(_, n)| *n)
    }
}

pub fn docker(text: &str) -> Docker {
    let (mut entry, mut cmd, mut line, mut image) = (None, None, 1, String::new());
    for (i, l) in text.lines().enumerate() {
        let l = l.trim();
        let (word, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
        match word.to_ascii_uppercase().as_str() {
            "FROM" => {
                (entry, cmd, line) = (None, None, i + 1);
                let from = rest.split_whitespace().find(|w| !w.starts_with("--")).unwrap_or("");
                let base = from.rsplit('/').next().unwrap_or(from);
                image = base.split([':', '@']).next().unwrap_or(base).to_ascii_lowercase();
            }
            "ENTRYPOINT" => (entry, line) = (Some(crate::yaml::command(rest)), i + 1),
            "CMD" => (cmd, line) = (Some(crate::yaml::command(rest)), i + 1),
            _ => {}
        }
    }
    let parts: Vec<String> = [entry, cmd].into_iter().flatten().filter(|s| !s.is_empty()).collect();
    Docker {
        cmd: (!parts.is_empty()).then(|| parts.join(" ")),
        line,
        image,
    }
}

/// The Dockerfile a platform file names (`build.dockerfilePath`, `[build] dockerfile`): its start command becomes the
/// app's, and its path the one the report names, when the root has none of its own.
fn named_dockerfile(f: &mut Facts, files: &BTreeMap<String, String>, path: &str) -> Option<Docker> {
    let path = path.trim_start_matches("./");
    let d = docker(files.get(path)?);
    if f.dockerfile.as_deref().is_none_or(|p| p == path) || f.docker_cmd.is_none() {
        f.dockerfile = Some(path.into());
        f.docker_cmd.clone_from(&d.cmd);
    }
    Some(d)
}

/// The route a platform checks the app's health on (`/alive`), else the first health route the code serves.
pub(crate) fn health(f: &mut Facts, path: &str) {
    if f.health.is_none() && path.starts_with('/') {
        f.health = Some(path.into());
    }
}

/// The root Dockerfile: its start command and the env names of `ENV`.
fn dockerfile(f: &mut Facts, text: &str) {
    f.manifest("Dockerfile", "dockerfile");
    f.dockerfile = Some("Dockerfile".into());
    f.docker_cmd = docker(text).cmd;
    for line in text.lines() {
        let line = line.trim();
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if word.eq_ignore_ascii_case("ENV") {
            let names = if rest.contains('=') {
                rest.split_whitespace()
                    .filter_map(|kv| kv.split_once('='))
                    .map(|(k, _)| k)
                    .collect()
            } else {
                rest.split_whitespace().next().into_iter().collect::<Vec<_>>()
            };
            f.env.extend(
                names
                    .into_iter()
                    .filter(|n| crate::code::is_env_name(n))
                    .map(String::from),
            );
        }
    }
}

/// Names left of `=`; the values are not kept.
fn env_sample(f: &mut Facts, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((name, _)) = line.split_once('=')
            && crate::code::is_env_name(name.trim())
        {
            f.env.insert(name.trim().into());
        }
    }
}

/// No platform file names a process: the Dockerfile's command, else `npm start`'s script, is the web process; a
/// Dockerfile on a file server's image, a `serve` script or a Caddyfile or nginx.conf that serves files is a
/// static site; a web framework in the dependencies is a web process started the framework's way.
fn fallback_process(f: &mut Facts, files: &BTreeMap<String, String>) {
    if let Some(t) = files.get("Dockerfile") {
        let d = docker(t);
        let from = at("Dockerfile", d.line);
        if let Some(server) = d.server() {
            f.processes
                .push(shapes::static_site("web", d.cmd.as_deref().unwrap_or(""), server, from));
            return;
        }
        if let Some(cmd) = d.cmd {
            f.processes.push(process("web", "web", "web", &cmd, from));
            return;
        }
    }
    if let Some(t) = files.get("package.json")
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(t)
        && let Some(start) = v["scripts"]["start"].as_str()
    {
        let from = at("package.json", line_path(t, &["\"scripts\"", "\"start\""]).unwrap_or(1));
        f.processes.push(match shapes::serve_script(start) {
            Some(server) => shapes::static_site("web", start, server, from),
            None => process("web", "web", "web", start, from),
        });
        return;
    }
    if let Some(p) = shapes::server_config(files) {
        f.processes.push(p);
        return;
    }
    if let Some((_, dep)) = crate::stack::found(&f.deps, crate::stack::Role::Web).first() {
        let from = dep.at.clone();
        f.processes.push(process("web", "web", "web", "", from));
    }
}
