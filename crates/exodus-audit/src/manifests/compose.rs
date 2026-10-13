//! `docker-compose*.yml` / `compose.yml`: images of known stores become data, built services become processes
//! when no platform file names any, `environment` gives env names.

use std::collections::BTreeMap;

use exodus_schema::Data;

use super::{docker, process, shapes};
use crate::{Facts, at, yaml};

pub fn is_compose(name: &str) -> bool {
    (name.ends_with(".yml") || name.ends_with(".yaml"))
        && (name.starts_with("docker-compose") || name.starts_with("compose."))
}

/// The file the others override (`docker-compose.yml`, `compose.yaml`): it names the processes; the variants
/// (`docker-compose.dev.yml`) add stores only.
pub fn is_base(name: &str) -> bool {
    matches!(
        name,
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml"
    )
}

/// (image name prefix after the registry and owner, data id, kind, engine)
const STORES: &[(&str, &str, &str, &str)] = &[
    ("postgres", "postgres", "db", "PostgreSQL"),
    ("postgis", "postgres", "db", "PostgreSQL"),
    ("mysql", "mysql", "db", "MySQL"),
    ("mariadb", "mysql", "db", "MySQL"),
    ("mongo", "mongodb", "db", "MongoDB"),
    ("redis", "redis", "cache", "Redis"),
    ("valkey", "redis", "cache", "Redis"),
];

/// The Dockerfile a service builds, relative to the repository: `context` (or the `build` value) and `dockerfile`,
/// from the compose file's folder.
fn dockerfile_of(file: &str, build: &yaml::Node) -> String {
    let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
    let context = build.str("context").unwrap_or(&build.value);
    let name = build.str("dockerfile").unwrap_or("Dockerfile");
    [dir, context, name]
        .iter()
        .flat_map(|p| p.split('/'))
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

pub fn read(f: &mut Facts, file: &str, text: &str, files: &BTreeMap<String, String>) {
    f.manifest(file, "compose");
    let root = yaml::parse(text);
    let had_processes = !f.processes.is_empty();
    // A service built from the same Dockerfile, target and command as one before it is the same process.
    let mut built: Vec<(String, String)> = Vec::new();
    for svc in root.get("services").into_iter().flat_map(|s| s.children.iter()) {
        if let Some(env) = svc.get("environment") {
            for e in &env.children {
                // `- KEY=value` is a list item holding one scalar; `KEY: value` is a mapping entry.
                let name = if e.item {
                    e.children.first().and_then(|c| c.value.split('=').next()).unwrap_or("")
                } else {
                    e.key.as_str()
                };
                if crate::code::is_env_name(name) {
                    f.env.insert(name.into());
                }
            }
        }
        let image = svc.str("image").unwrap_or("");
        let (repo, tag) = match image.rsplit_once(':') {
            Some((r, t)) if !t.contains('/') => (r, t),
            _ => (image, ""),
        };
        let base = repo.rsplit('/').next().unwrap_or(repo);
        let store = STORES.iter().find(|s| !base.is_empty() && base.starts_with(s.0));
        if let Some((_, id, kind, engine)) = store {
            if f.has_engine(engine) {
                continue;
            }
            let version = tag
                .split(['.', '-'])
                .next()
                .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()));
            f.data.push(Data {
                id: (*id).into(),
                kind: (*kind).into(),
                name: svc.key.clone(),
                engine: (*engine).into(),
                version: version.map(String::from),
                size_mb: None,
                from: at(file, svc.line),
                env_name: None,
            });
        } else if let Some(build) = svc.get("build")
            && !had_processes
            && !svc.str("profiles").is_some_and(|p| p.contains("deprecated"))
        {
            // a target is a stage of the file, not its last one: its server is not read
            let built_from = build.str("target").is_none().then(|| dockerfile_of(file, build));
            let d = built_from.and_then(|p| files.get(&p)).map(|t| docker(t));
            let command = svc
                .str("command")
                .map(yaml::command)
                .or_else(|| d.as_ref().and_then(|d| d.cmd.clone()))
                .or_else(|| f.docker_cmd.clone())
                .unwrap_or_default();
            let image = format!(
                "{} {} {}",
                build.value,
                build.str("dockerfile").unwrap_or(""),
                build.str("target").unwrap_or("")
            );
            if built.contains(&(image.clone(), command.clone())) {
                continue;
            }
            built.push((image, command.clone()));
            let kind = if command.contains("migrate") {
                "release"
            } else if svc.get("ports").or(svc.get("expose")).is_some() {
                "web"
            } else {
                "worker"
            };
            let from = at(file, svc.line);
            f.processes.push(match d.as_ref().and_then(|d| d.server()) {
                Some(server) if kind == "web" => shapes::static_site(&svc.key, &command, server, from),
                _ => process(&svc.key, kind, &svc.key, &command, from),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_processes_and_env_names() {
        let mut f = Facts::default();
        let text = "services:\n  app:\n    build: .\n    ports: [\"8080:8080\"]\n    command: [\"node\", \"server.js\"]\n    environment:\n      - DATABASE_URL=postgres://u:p@db/app\n  db:\n    image: docker.io/library/postgres:16-alpine\n    environment:\n      POSTGRES_PASSWORD: secret\n";
        read(&mut f, "docker-compose.yml", text, &BTreeMap::new());
        assert_eq!(f.processes.len(), 1);
        assert_eq!(
            (f.processes[0].kind.as_str(), f.processes[0].command.as_str()),
            ("web", "node server.js")
        );
        assert_eq!(f.processes[0].from, "docker-compose.yml:2");
        assert_eq!(f.data.len(), 1);
        assert_eq!(
            (f.data[0].engine.as_str(), f.data[0].version.as_deref()),
            ("PostgreSQL", Some("16"))
        );
        assert_eq!(f.data[0].from, "docker-compose.yml:8");
        assert_eq!(f.env.iter().collect::<Vec<_>>(), ["DATABASE_URL", "POSTGRES_PASSWORD"]);
    }

    #[test]
    fn a_deprecated_profile_is_not_a_process() {
        let mut f = Facts::default();
        let text = "services:\n  app:\n    build: .\n    ports: [\"80:80\"]\n  old:\n    build: ./old\n    profiles: [deprecated]\n";
        read(&mut f, "docker-compose.yml", text, &BTreeMap::new());
        assert_eq!(f.processes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["app"]);
    }
}
