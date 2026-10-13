//! Heroku's `app.json`: add-ons become data (Postgres, Redis) or the Scheduler, `env` gives names, the web
//! dyno's size says whether it sleeps.

use exodus_schema::{Data, Sleep};

use super::{line_path, platform, process};
use crate::{Facts, at};

const FILE: &str = "app.json";

pub fn read(f: &mut Facts, text: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    // app.json is also Expo's file; Heroku's has at least one of these keys.
    let heroku = ["addons", "formation", "buildpacks", "stack", "environments", "env"]
        .iter()
        .any(|k| v.get(k).is_some());
    if !heroku || v.get("expo").is_some() {
        return;
    }
    f.manifest(FILE, "heroku");
    if let Some(env) = v["env"].as_object() {
        f.env
            .extend(env.keys().filter(|k| crate::code::is_env_name(k)).cloned());
    }
    let sleep = v["formation"]["web"].as_object().and_then(|web| {
        let size = web.get("size").and_then(|s| s.as_str()).unwrap_or("");
        let line = line_path(text, &["\"formation\"", "\"web\""])?;
        Some(Sleep {
            on: matches!(size.to_ascii_lowercase().as_str(), "eco" | "free"),
            from: at(FILE, line),
        })
    });
    platform(f, 3, "Heroku", FILE, sleep);

    for addon in v["addons"].as_array().into_iter().flatten() {
        let Some(plan) = addon.as_str().or_else(|| addon["plan"].as_str()) else {
            continue;
        };
        let Some(line) = line_path(text, &["\"addons\"", &format!("\"{plan}\"")]) else {
            continue;
        };
        let service = plan.split(':').next().unwrap_or(plan);
        let (id, kind, name, engine, env) = match service {
            "heroku-postgresql" => ("postgres", "db", "Postgres", "PostgreSQL", "DATABASE_URL"),
            "heroku-redis" => ("redis", "cache", "Redis", "Redis", "REDIS_URL"),
            "rediscloud" => ("redis", "cache", "Redis", "Redis", "REDISCLOUD_URL"),
            "scheduler" => {
                let mut p = process("scheduler", "cron", "Heroku Scheduler", "", at(FILE, line));
                p.schedules = Some(Vec::new());
                f.processes.push(p);
                continue;
            }
            _ => continue,
        };
        f.data.push(Data {
            id: id.into(),
            kind: kind.into(),
            name: name.into(),
            engine: engine.into(),
            version: None,
            size_mb: None,
            from: at(FILE, line),
            env_name: Some(env.into()),
        });
    }
}
