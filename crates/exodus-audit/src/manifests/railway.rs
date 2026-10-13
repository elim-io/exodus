//! `railway.json` / `railway.toml` (config as code): the start command (else the command of the Dockerfile Railway
//! builds), a cron schedule, app sleeping, the health check path and the volume the service requires.
//! Railway keeps databases and volumes outside the repository, so they come from the code.

use std::collections::BTreeMap;

use exodus_schema::{Data, Schedule, Sleep};

use super::{health, line_path, line_where, named_dockerfile, platform, process};
use crate::{Facts, at};

pub fn read(f: &mut Facts, file: &str, text: &str, files: &BTreeMap<String, String>) {
    let section = |name: &str| {
        if file.ends_with(".json") {
            serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .map(|v| v[name].clone())
        } else {
            text.parse::<toml::Table>()
                .ok()
                .and_then(|t| t.get(name).cloned())
                .and_then(|d| serde_json::to_value(d).ok())
        }
        .unwrap_or_default()
    };
    let (deploy, build) = (section("deploy"), section("build"));
    f.manifest(file, "railway");
    let line = |key: &str| -> Option<usize> {
        if file.ends_with(".json") {
            line_path(text, &["\"deploy\"", &format!("\"{key}\"")])
        } else {
            let start = line_where(text, 1, |l| l.trim() == "[deploy]")?;
            line_where(text, start, |l| l.trim_start().starts_with(key))
        }
    };
    let sleep = deploy["sleepApplication"].as_bool().and_then(|on| {
        Some(Sleep {
            on,
            from: at(file, line("sleepApplication")?),
        })
    });
    platform(f, 0, "Railway", file, sleep);
    if let Some(path) = deploy["healthcheckPath"].as_str() {
        health(f, path);
    }
    // A volume the service must have mounted (`requiredMountPath`) is where the app keeps its files.
    if let Some(mount) = deploy["requiredMountPath"].as_str() {
        let name = mount.trim_matches('/');
        f.data.push(Data {
            id: crate::code::slug(if name.is_empty() { "volume" } else { name }),
            kind: "files".into(),
            name: mount.into(),
            engine: "disk".into(),
            version: None,
            size_mb: None,
            from: at(file, line("requiredMountPath").unwrap_or(1)),
            env_name: None,
        });
    }
    // Railway builds the Dockerfile the build section names, else the root one.
    let docker = build["dockerfilePath"]
        .as_str()
        .and_then(|p| named_dockerfile(f, files, p).map(|d| (p.trim_start_matches("./").to_string(), d)));

    let Some(command) = deploy["startCommand"].as_str() else {
        // No start command: the service runs the image's own (ENTRYPOINT and CMD of that Dockerfile).
        if let Some((path, d)) = docker
            && let Some(cmd) = d.cmd
            && !f.processes.iter().any(|p| p.kind == "web")
        {
            f.processes.push(process("web", "web", "web", &cmd, at(&path, d.line)));
        }
        return;
    };
    let from = at(file, line("startCommand").unwrap_or(1));
    if let Some(cron) = deploy["cronSchedule"].as_str() {
        let mut p = process("cron", "cron", "cron", command, from);
        p.schedules = Some(vec![Schedule {
            cron: cron.into(),
            from: at(file, line("cronSchedule").unwrap_or(1)),
        }]);
        f.processes.push(p);
    } else if let Some(web) = f.processes.iter_mut().find(|p| p.kind == "web") {
        // The start command overrides the Procfile's web line.
        web.command = command.into();
        web.from = from;
    } else {
        f.processes.push(process("web", "web", "web", command, from));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dockerfile_build_runs_the_image_command_with_its_volume_and_health_path() {
        let mut f = Facts::default();
        let json = "{\n  \"build\": {\n    \"builder\": \"DOCKERFILE\",\n    \"dockerfilePath\": \"docker/Dockerfile.debian\"\n  },\n  \"deploy\": {\n    \"requiredMountPath\": \"/data\",\n    \"healthcheckPath\": \"/alive\"\n  }\n}\n";
        let files = BTreeMap::from([
            ("railway.json".to_string(), json.to_string()),
            (
                "docker/Dockerfile.debian".to_string(),
                "FROM rust AS build\nRUN cargo build\nFROM debian:trixie-slim\nCMD [\"/start.sh\"]\n".to_string(),
            ),
        ]);
        read(&mut f, "railway.json", json, &files);
        assert_eq!(f.processes.len(), 1);
        assert_eq!(
            (
                f.processes[0].kind.as_str(),
                f.processes[0].command.as_str(),
                f.processes[0].from.as_str()
            ),
            ("web", "/start.sh", "docker/Dockerfile.debian:4")
        );
        assert_eq!(f.dockerfile.as_deref(), Some("docker/Dockerfile.debian"));
        assert_eq!(f.health.as_deref(), Some("/alive"));
        assert_eq!(
            (f.data[0].kind.as_str(), f.data[0].id.as_str(), f.data[0].from.as_str()),
            ("files", "data", "railway.json:7")
        );
    }
}
