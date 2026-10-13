//! The audit on the four sample repositories in examples/, against their hand-written expected.json.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Compared whole; patterns and questions are compared by `compare_patterns` and by id (examples/README.md).
const COMPARED: &[&str] = &[
    "platform",
    "stack",
    "manifests",
    "processes",
    "data",
    "settings",
    "clean",
];

/// A pattern's fields that must be equal; `title`, `why`, `fix` and `subject` are the rule's text and must only
/// be present.
const EQUAL: &[&str] = &["id", "rule", "kind", "at", "code", "part", "kb"];
const PRESENT: &[&str] = &["title", "why", "fix", "subject"];

fn compare_patterns(found: &Value, expected: &Value) -> Option<String> {
    let strip = |v: &Value, keep: &[&str]| -> Vec<Value> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|p| keep.iter().map(|k| ((*k).to_string(), p[*k].clone())).collect())
            .collect()
    };
    let (f, e) = (strip(found, EQUAL), strip(expected, EQUAL));
    if f != e {
        return Some(format!(
            "patterns\n  found:    {}\n  expected: {}",
            serde_json::to_string(&f).unwrap(),
            serde_json::to_string(&e).unwrap()
        ));
    }
    for (fp, ep) in found.as_array().unwrap().iter().zip(expected.as_array().unwrap()) {
        for k in PRESENT {
            // A rule with no subject (a note) has none in either.
            if ep.get(*k).is_some() != fp.get(*k).is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty())) {
                return Some(format!("pattern {}: {k} present in one and not the other", ep["id"]));
            }
        }
    }
    None
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn check(name: &str) {
    let root = examples().join(name);
    let found = serde_json::to_value(exodus_audit::audit(&root).unwrap()).unwrap();
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(root.join("expected.json")).unwrap()).unwrap();
    let mut wrong = Vec::new();
    for key in COMPARED {
        if found[key] != expected[key] {
            wrong.push(format!(
                "{key}\n  found:    {}\n  expected: {}",
                serde_json::to_string(&found[key]).unwrap(),
                serde_json::to_string(&expected[key]).unwrap()
            ));
        }
    }
    wrong.extend(compare_patterns(&found["patterns"], &expected["patterns"]));
    let ids = |v: &Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|q| q["id"].clone())
            .collect::<Vec<_>>()
    };
    if ids(&found["questions"]) != ids(&expected["questions"]) {
        wrong.push(format!(
            "questions\n  found:    {:?}\n  expected: {:?}",
            ids(&found["questions"]),
            ids(&expected["questions"])
        ));
    }
    assert!(wrong.is_empty(), "{name}:\n{}", wrong.join("\n"));

    // The samples carry env values on purpose; none may reach the findings.
    let text = found.to_string();
    for value in ["change-me", "/data/cards.db", "enabled", "postgres://", "redis://"] {
        assert!(
            !text.contains(value),
            "{name}: the value {value:?} leaked into the findings"
        );
    }
}

#[test]
fn node_railway() {
    check("node-railway");
}

#[test]
fn rails_heroku() {
    check("rails-heroku");
}

#[test]
fn django_render() {
    check("django-render");
}

#[test]
fn go_fly() {
    check("go-fly");
}

/// Creating the SQLite file before opening it is one change (the SQLite file), not a second one for files on disk.
#[test]
fn the_sqlite_file_is_one_change() {
    let root = std::env::temp_dir().join(format!("exodus-sqlite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("go.mod"),
        "module x\n\ngo 1.23\n\nrequire modernc.org/sqlite v1.34.0\n",
    )
    .unwrap();
    std::fs::write(
        root.join("main.go"),
        "package main\n\nfunc main() {\n\tf, _ := os.Create(dbPath)\n\tf.Close()\n\tdb, _ := sql.Open(\"sqlite\", dbPath)\n\t_ = db\n}\n",
    )
    .unwrap();
    let found = exodus_audit::audit(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let rules: Vec<&str> = found.patterns.iter().map(|p| p.rule.as_str()).collect();
    assert_eq!(rules, ["sqlite-database"]);
}

#[test]
fn audit_is_deterministic() {
    let root = examples().join("node-railway");
    assert_eq!(exodus_audit::audit(&root).unwrap(), exodus_audit::audit(&root).unwrap());
}

/// Every place a value can sit in a repository holds the marker; the findings may carry the names, never the marker.
#[test]
fn no_env_value_reaches_the_findings() {
    const V: &str = "VALUE-MARKER-7c1e";
    let root = std::env::temp_dir().join(format!("exodus-values-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    let files = [
        (".env", format!("DATABASE_URL=postgres://u:{V}@h/db\n")),
        (".env.production", format!("A_TOKEN={V}\n")),
        ("prod.env", format!("B_TOKEN={V}\n")),
        ("src/.envrc", format!("export C_TOKEN={V}\n")),
        (".env.example", format!("D_TOKEN={V}\nexport E_KEY={V}\n")),
        ("fly.toml", format!("app = \"x\"\n[env]\n  F_TOKEN = \"{V}\"\n")),
        (
            "app.json",
            format!("{{\"env\":{{\"G_KEY\":{{\"value\":\"{V}\"}},\"H\":\"{V}\"}}}}"),
        ),
        (
            "render.yaml",
            format!(
                "services:\n  - type: web\n    name: w\n    startCommand: node src/s.js\n    envVars:\n      - key: I_SECRET\n        value: {V}\n"
            ),
        ),
        (
            "Dockerfile",
            format!("FROM node:20\nENV J_SECRET={V}\nENV K_SECRET {V}\nCMD [\"node\",\"src/s.js\"]\n"),
        ),
        (
            "docker-compose.yml",
            format!(
                "services:\n  db:\n    image: postgres:16\n    environment:\n      L_PASSWORD: {V}\n      - M_URL=postgres://u:{V}@db/x\n"
            ),
        ),
        (
            "Procfile",
            format!("web: N_URL=postgres://u:{V}@h/d node src/s.js --token={V}\n"),
        ),
        (
            "src/s.js",
            format!(
                "const session = require('express-session');\napp.use(session({{ secret: '{V}' }}));\nconst db = 'postgres://u:{V}@h/d'; setInterval(f, 5000);\n"
            ),
        ),
    ];
    for (path, text) in &files {
        std::fs::write(root.join(path), text).unwrap();
    }
    let found = serde_json::to_string(&exodus_audit::audit(&root).unwrap()).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(!found.contains(V), "a value leaked: {found}");
    for name in [
        "D_TOKEN", "E_KEY", "F_TOKEN", "G_KEY", "I_SECRET", "J_SECRET", "K_SECRET", "N_URL",
    ] {
        assert!(found.contains(name), "the name {name} was lost");
    }
    for name in ["A_TOKEN", "B_TOKEN", "C_TOKEN"] {
        assert!(!found.contains(name), "{name} came from a file that holds values");
    }
}
