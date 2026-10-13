//! `fly.toml`: process groups (or the one `app` group running the image's command), the release command,
//! mounts, `[env]` names and whether machines stop when idle.

use exodus_schema::{Data, Sleep};

use super::{line_where, platform, process};
use crate::{Facts, at};

const FILE: &str = "fly.toml";

pub fn read(f: &mut Facts, text: &str) {
    f.manifest(FILE, "fly");
    let Ok(t) = text.parse::<toml::Table>() else {
        platform(f, 2, "Fly.io", FILE, None);
        return;
    };
    // The line of `[section]` (or the n-th `[[section]]`), and of a key under it.
    let header = |section: &str, n: usize| {
        let (a, b) = (format!("[{section}]"), format!("[[{section}]]"));
        text.lines()
            .enumerate()
            .filter(|(_, l)| l.trim() == a || l.trim() == b)
            .nth(n)
            .map(|(i, _)| i + 1)
    };
    let key_line = |section: &str, key: &str| {
        let start = header(section, 0)?;
        line_where(text, start + 1, |l| {
            l.trim_start().starts_with(key) || l.trim_start().starts_with('[')
        })
        .filter(|&n| text.lines().nth(n - 1).is_some_and(|l| l.trim_start().starts_with(key)))
    };

    if let Some(env) = t.get("env").and_then(|e| e.as_table()) {
        f.env
            .extend(env.keys().filter(|k| crate::code::is_env_name(k)).cloned());
    }

    let http = t.get("http_service").and_then(|h| h.as_table());
    let sleep = http.and_then(|h| h.get("auto_stop_machines")).and_then(|v| {
        let on = match v {
            toml::Value::Boolean(b) => *b,
            toml::Value::String(s) => s != "off",
            _ => return None,
        };
        // `min_machines_running = 1` keeps a machine up however idle the app is
        let kept = http
            .and_then(|h| h.get("min_machines_running"))
            .and_then(|m| m.as_integer())
            .is_some_and(|m| m >= 1);
        let key = if on && kept {
            "min_machines_running"
        } else {
            "auto_stop_machines"
        };
        Some(Sleep {
            on: on && !kept,
            from: at(FILE, key_line("http_service", key)?),
        })
    });
    platform(f, 2, "Fly.io", FILE, sleep);
    if let Some(path) = http
        .and_then(|h| h.get("checks"))
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .find_map(|c| c.get("path").and_then(|p| p.as_str()))
    {
        super::health(f, path);
    }

    let web_groups: Vec<String> = http
        .and_then(|h| h.get("processes"))
        .and_then(|p| p.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["app".into()]);
    let served = http.is_some() || t.contains_key("services");
    let serve_line = header("http_service", 0).or_else(|| header("services", 0)).unwrap_or(1);
    match t.get("processes").and_then(|p| p.as_table()) {
        Some(groups) => {
            for (name, cmd) in groups {
                let kind = if served && web_groups.contains(name) {
                    "web"
                } else {
                    "worker"
                };
                let line = key_line("processes", name).unwrap_or(serve_line);
                f.processes
                    .push(process(kind, kind, name, cmd.as_str().unwrap_or(""), at(FILE, line)));
            }
        }
        None => {
            let kind = if served { "web" } else { "worker" };
            let cmd = f.docker_cmd.clone().unwrap_or_default();
            f.processes.push(process(kind, kind, "app", &cmd, at(FILE, serve_line)));
        }
    }
    if let Some(cmd) = t
        .get("deploy")
        .and_then(|d| d.get("release_command"))
        .and_then(|c| c.as_str())
    {
        let line = key_line("deploy", "release_command").unwrap_or(1);
        f.processes
            .push(process("release", "release", "release", cmd, at(FILE, line)));
    }

    let mounts: Vec<&toml::Value> = match t.get("mounts") {
        Some(toml::Value::Array(a)) => a.iter().collect(),
        Some(m) => vec![m],
        None => vec![],
    };
    for (n, m) in mounts.into_iter().enumerate() {
        let source = m.get("source").and_then(|s| s.as_str()).unwrap_or("data");
        f.data.push(Data {
            id: crate::code::slug(source),
            kind: "files".into(),
            name: source.into(),
            engine: "disk".into(),
            version: None,
            size_mb: None,
            from: at(FILE, header("mounts", n).unwrap_or(1)),
            env_name: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleep(http: &str) -> Option<Sleep> {
        let mut f = Facts::default();
        read(&mut f, &format!("app = \"x\"\n[http_service]\n{http}"));
        f.platforms.pop().and_then(|(_, p)| p.sleep)
    }

    #[test]
    fn a_kept_machine_means_the_app_does_not_sleep() {
        let stop = "  auto_stop_machines = \"stop\"\n";
        assert_eq!(sleep(stop).map(|s| (s.on, s.from)), Some((true, "fly.toml:3".into())));
        let kept = sleep(&format!("{stop}  min_machines_running = 1\n")).unwrap();
        assert_eq!((kept.on, kept.from.as_str()), (false, "fly.toml:4"));
        let none = sleep(&format!("{stop}  min_machines_running = 0\n")).unwrap();
        assert!(none.on);
    }
}
