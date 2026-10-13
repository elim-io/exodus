//! `railway.json` / `railway.toml` (config as code): the start command, a cron schedule and app sleeping.
//! Railway keeps databases and volumes outside the repository, so they come from the code.

use exodus_schema::{Schedule, Sleep};

use super::{line_path, line_where, platform, process};
use crate::{Facts, at};

pub fn read(f: &mut Facts, file: &str, text: &str) {
    let deploy = if file.ends_with(".json") {
        serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .map(|v| v["deploy"].clone())
    } else {
        text.parse::<toml::Table>()
            .ok()
            .and_then(|t| t.get("deploy").cloned())
            .and_then(|d| serde_json::to_value(d).ok())
    };
    f.manifest(file, "railway");
    let deploy = deploy.unwrap_or_default();
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

    let Some(command) = deploy["startCommand"].as_str() else {
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
