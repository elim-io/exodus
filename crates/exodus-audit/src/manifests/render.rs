//! `render.yaml` (Blueprint): services become processes, `preDeployCommand` a release step, Key Value (Redis)
//! services, `databases:` and disks become data; `envVars` give env names and which variable reaches which store.

use std::collections::BTreeMap;

use exodus_schema::{Data, Schedule, Sleep};

use super::{platform, process};
use crate::{Facts, at, yaml};

pub fn read(f: &mut Facts, file: &str, text: &str) {
    f.manifest(file, "render");
    let root = yaml::parse(text);
    // store name -> the first env var that points at it
    let mut links: BTreeMap<String, String> = BTreeMap::new();
    let mut sleep = None;
    let mut data = Vec::new();
    for svc in root.get("services").into_iter().flat_map(|s| s.items()) {
        let name = svc.str("name").unwrap_or("service");
        for ev in svc.get("envVars").into_iter().flat_map(|e| e.items()) {
            let Some(key) = ev.str("key") else { continue };
            f.env.insert(key.into());
            for from in ["fromDatabase", "fromService"] {
                if let Some(target) = ev.get(from).and_then(|n| n.str("name")) {
                    links.entry(target.into()).or_insert_with(|| key.into());
                }
            }
        }
        let ty = svc.str("type").unwrap_or("");
        let kind = match ty {
            "web" | "pserv" => "web",
            "worker" => "worker",
            "cron" => "cron",
            "redis" | "keyvalue" => {
                data.push(store(svc.line, file, "redis", "cache", name, "Redis"));
                continue;
            }
            _ => continue,
        };
        if kind == "web" && sleep.is_none() {
            sleep = svc.get("plan").map(|p| Sleep {
                on: p.value == "free",
                from: at(file, p.line),
            });
        }
        let start = svc.get("startCommand").or_else(|| svc.get("dockerCommand"));
        let command = start
            .map(|n| n.value.clone())
            .or_else(|| f.docker_cmd.clone())
            .unwrap_or_default();
        let mut p = process(kind, kind, name, &command, at(file, start.map_or(svc.line, |n| n.line)));
        if let Some(s) = svc.get("schedule") {
            p.schedules = Some(vec![Schedule {
                cron: s.value.clone(),
                from: at(file, s.line),
            }]);
        }
        f.processes.push(p);
        if let Some(pre) = svc.get("preDeployCommand") {
            let release = format!("{name} pre-deploy");
            f.processes
                .push(process("release", "release", &release, &pre.value, at(file, pre.line)));
        }
        if let Some(disk) = svc.get("disk") {
            let dname = disk.str("name").unwrap_or("disk");
            data.push(store(
                disk.line,
                file,
                &crate::code::slug(dname),
                "files",
                dname,
                "disk",
            ));
        }
    }
    for db in root.get("databases").into_iter().flat_map(|d| d.items()) {
        let name = db.str("name").unwrap_or("database");
        let mut d = store(db.line, file, "postgres", "db", name, "PostgreSQL");
        d.version = db.str("postgresMajorVersion").map(String::from);
        data.push(d);
    }
    for mut d in data {
        d.env_name = links.get(&d.name).cloned();
        f.data.push(d);
    }
    platform(f, 1, "Render", file, sleep);
}

fn store(line: usize, file: &str, id: &str, kind: &str, name: &str, engine: &str) -> Data {
    Data {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        engine: engine.into(),
        version: None,
        size_mb: None,
        from: at(file, line),
        env_name: None,
    }
}
