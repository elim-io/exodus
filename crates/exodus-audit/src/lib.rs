//! The audit: reads a repository's manifests and code into `findings.json` (processes, data, settings, stack,
//! build), runs the rules and picks the questions. Deterministic, no network; reads env names, never env values.

mod code;
mod manifests;
mod place;
mod redact;
pub mod rules;
mod scan;
mod stack;
mod yaml;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use exodus_schema::{Data, Findings, Manifest, Platform, Process, Question, Repo, Settings, Stats};
use serde::Deserialize;

use manifests::STAYS;
use place::NOT_ON_SERVER;
pub use stack::runtime_of;

/// The `clean` entries of findings by kind, the prefixes taken off.
#[derive(Default)]
pub struct Clean<'a> {
    /// What the rules looked for and did not find ("in-process timers").
    pub looked_for: Vec<&'a str>,
    /// "website/ (browser code)": places whose code does not run on the server, so no rule counted it.
    pub not_on_server: Vec<&'a str>,
    /// Parts that stay where they are ("Firestore (firebase.json)").
    pub stays: Vec<&'a str>,
}

/// Sorts `clean` into its three kinds.
pub fn split_clean(clean: &[String]) -> Clean<'_> {
    let mut out = Clean::default();
    for c in clean {
        if let Some(c) = c.strip_prefix(NOT_ON_SERVER) {
            out.not_on_server.push(c);
        } else if let Some(c) = c.strip_prefix(STAYS) {
            out.stays.push(c);
        } else {
            out.looked_for.push(c);
        }
    }
    out
}

/// What the readers gather before it becomes findings.
#[derive(Default)]
struct Facts {
    manifests: Vec<Manifest>,
    /// (rank, platform): the lowest rank names the platform.
    platforms: Vec<(u8, Platform)>,
    processes: Vec<Process>,
    data: Vec<Data>,
    env: BTreeSet<String>,
    deps: Vec<stack::Dep>,
    /// (rank, language, version): the lowest rank is the stack's language.
    language: Option<(u8, &'static str, Option<String>)>,
    dockerfile: Option<String>,
    docker_cmd: Option<String>,
    /// The route the app answers health checks on (`/alive`), from the platform file or the code.
    health: Option<String>,
    /// The platform deploys the root folder's app itself (Firebase Hosting of a framework): no fallback process.
    hosted_root: bool,
    /// Parts that stay where they are (`clean` entries starting with `STAYS`).
    stays: Vec<String>,
    /// Folders (or project files) of mobile apps: named "not on the server", never moved.
    apps: Vec<String>,
}

impl Facts {
    fn manifest(&mut self, file: &str, kind: &str) {
        self.manifests.push(Manifest {
            file: file.into(),
            kind: kind.into(),
        });
    }

    fn language(&mut self, rank: u8, name: &'static str, version: Option<String>) {
        if self.language.as_ref().is_none_or(|l| rank < l.0) {
            self.language = Some((rank, name, version));
        }
    }

    fn has_engine(&self, engine: &str) -> bool {
        self.data.iter().any(|d| d.engine == engine)
    }
}

/// `file:line`, lines from 1.
fn at(file: &str, line: usize) -> String {
    format!("{file}:{line}")
}

/// Splits `file:line` for sorting: by file bytewise, then by line as a number.
fn at_key(at: &str) -> (&str, usize) {
    match at.rsplit_once(':') {
        Some((f, l)) => (f, l.parse().unwrap_or(0)),
        None => (at, 0),
    }
}

/// Audits the repository at `root` with the compiled-in rules. `repo`, `stats.startedAt`/`finishedAt` and `cli`
/// are the caller's.
pub fn audit(root: &Path) -> io::Result<Findings> {
    audit_with(root, &rules::builtin(), rules::PLACES_PER_RULE)
}

/// Audits with the given rules (`exodus check --rules` adds its own to the built-in ones), listing at most `limit`
/// places per rule.
pub fn audit_with(root: &Path, rules: &[rules::Rule], limit: usize) -> io::Result<Findings> {
    let files = scan::read(root)?;
    let mut f = Facts::default();
    manifests::read(&mut f, &files);
    for p in &f.processes {
        f.env.extend(redact::assigned(&p.command).into_iter().map(String::from));
    }
    let code = code::scan(&files);
    // the names the server's code reads; a test harness or a build script reads its own
    let placer = place::Placer::new(&files);
    f.env.extend(
        code.env
            .iter()
            .filter(|(_, a)| placer.off_server(at_key(a).0).is_none())
            .map(|(name, _)| name.clone()),
    );
    if f.health.is_none() {
        f.health.clone_from(&code.health);
    }
    f.platforms.sort_by_key(|p| p.0);
    let platform = f.platforms.first().map(|p| p.1.clone()).unwrap_or_else(|| Platform {
        name: "Unknown".into(),
        file: String::new(),
        sleep: None,
    });
    let runs: Vec<String> = f
        .processes
        .iter()
        .filter_map(|p| stack::entry_file(&p.command, &files))
        .collect();
    let (hits, mut off_server) = rules::run(rules, &files, &platform.name, &runs);
    // A mobile app is named once, by its folder, whether or not a rule matched in it.
    for app in &f.apps {
        off_server.retain(|at, why| !(at.starts_with(app.as_str()) && *why == place::MOBILE));
        off_server.insert(app.clone(), place::MOBILE);
    }
    code::data(&mut f, &code, &files, &hits);
    stack::frameworks(&mut f.processes, &f.deps, &files);
    attach(&mut f.processes, code.schedules, &files);
    dedupe(f.processes.iter_mut().map(|p| &mut p.id));
    dedupe(f.data.iter_mut().map(|d| &mut d.id));
    f.processes.sort_by(|a, b| at_key(&a.from).cmp(&at_key(&b.from)));
    f.data.sort_by(|a, b| at_key(&a.from).cmp(&at_key(&b.from)));
    f.manifests.sort_by(|a, b| a.file.cmp(&b.file));

    let stack = stack::badges(&f, &platform, &files);
    let web: Vec<&str> = stack::found(&f.deps, stack::Role::Web)
        .iter()
        .map(|(l, _)| l.badge)
        .collect();
    // an app built for SQLite and a server database (an ORM with both) runs on the server one when it has it, so
    // its SQLite code is no finding (decision of 2026-10-13: SQLite moves to Firestore only when it is the database)
    let server_db = f.data.iter().any(|d| d.kind == "db");
    let hits: Vec<rules::Hit> = hits
        .into_iter()
        .filter(|h| !(server_db && h.rule.id == "sqlite-database"))
        .collect();
    // a file written beside the SQLite open in the same file is the database file: one change, not two
    let db_files: Vec<&str> = hits
        .iter()
        .filter(|h| h.rule.id == "sqlite-database")
        .map(|h| at_key(&h.at).0)
        .collect();
    let changes: Vec<rules::Hit> = hits
        .iter()
        .filter(|h| {
            !(h.rule.id == "local-disk-write"
                && db_files.contains(&at_key(&h.at).0)
                && h.code.to_ascii_lowercase().contains("db"))
        })
        .cloned()
        .collect();
    let patterns = rules::patterns(&changes, &f.processes, &f.data, &files, &web, limit);
    let mut clean = rules::clean(rules, &hits);
    clean.extend(
        off_server
            .into_iter()
            .map(|(at, why)| format!("{}{at} ({why})", place::NOT_ON_SERVER)),
    );
    clean.extend(f.stays.iter().map(|s| format!("{STAYS}{s}")));
    let rules_run = rules
        .iter()
        .filter(|r| r.file.platform.as_deref().is_none_or(|p| p == platform.name))
        .count();
    let questions = questions(&f.processes, &platform.name);
    let env_names: Vec<String> = f.env.into_iter().collect();
    let settings = Settings {
        secrets_guess: env_names.iter().filter(|n| secret_like(n)).count(),
        variables_total: env_names.len(),
        env_names,
        dockerfile: f.dockerfile,
        ci: ci_file(&files),
        domains: None,
        health: f.health,
    };
    Ok(Findings {
        exodus: exodus_schema::FINDINGS.into(),
        kind: "move".into(),
        repo: Repo {
            owner: None,
            name: root
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default(),
        },
        platform,
        stack,
        what: None,
        traffic: None,
        manifests: f.manifests,
        processes: f.processes,
        data: f.data,
        settings,
        patterns,
        clean,
        questions,
        stats: Stats {
            files: files.len(),
            bytes: files.values().map(|t| t.len() as u64).sum(),
            rules_run,
            ..Stats::default()
        },
        cli: String::new(),
        understanding: None,
    })
}

#[derive(Deserialize)]
struct Bank {
    question: Vec<BankQuestion>,
}

/// One question of `questions.toml` (architecture §6.4).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BankQuestion {
    pub id: String,
    /// always, web, worker, schedule, web-timer, agent or new.
    pub when: String,
    pub q: String,
    pub affects: String,
    pub hint: Option<String>,
    /// The machine value an answer may carry (A14): visits, egress, growth, users or bill.
    pub value: Option<String>,
}

/// The whole question bank, in its order.
pub fn bank() -> Vec<BankQuestion> {
    toml::from_str::<Bank>(include_str!("../questions.toml")).map_or_else(|_| Vec::new(), |b| b.question)
}

/// The questions of a move the repository cannot answer (`questions.toml`), in the bank's order.
fn questions(processes: &[Process], platform: &str) -> Vec<Question> {
    let bank = bank();
    let has = |kind: &str| processes.iter().any(|p| p.kind == kind);
    let scheduled = processes
        .iter()
        .any(|p| p.kind == "cron" || p.schedules.as_ref().is_some_and(|s| !s.is_empty()));
    let web_timer = processes
        .iter()
        .any(|p| p.kind == "web" && p.schedules.as_ref().is_some_and(|s| !s.is_empty()));
    let name = if platform == "Unknown" { "your host" } else { platform };
    bank.into_iter()
        .filter(|q| match q.when.as_str() {
            "always" => true,
            "web" => has("web"),
            "worker" => has("worker"),
            "schedule" => scheduled,
            "web-timer" => web_timer,
            "unknown-platform" => platform == "Unknown",
            _ => false,
        })
        .map(|q| Question {
            id: q.id,
            q: q.q.replace("{platform}", name),
            affects: q.affects,
            hint: q.hint,
        })
        .collect()
}

/// Names that hold a secret or a connection string with credentials in it.
fn secret_like(name: &str) -> bool {
    ["SECRET", "_KEY", "TOKEN", "PASSWORD"].iter().any(|s| name.contains(s))
        || (name.ends_with("_URL")
            && [
                "DATABASE", "POSTGRES", "MYSQL", "MONGO", "REDIS", "BROKER", "AMQP", "RABBIT",
            ]
            .iter()
            .any(|s| name.contains(s)))
        || name.ends_with("_DSN")
}

/// The CI file `exodus check` would run in: the first workflow, else GitLab, CircleCI or Jenkins.
fn ci_file(files: &BTreeMap<String, String>) -> Option<String> {
    files
        .keys()
        .find(|p| p.starts_with(".github/workflows/") && (p.ends_with(".yml") || p.ends_with(".yaml")))
        .or_else(|| {
            [".gitlab-ci.yml", ".circleci/config.yml", "Jenkinsfile"]
                .iter()
                .find_map(|c| files.get_key_value(*c).map(|(k, _)| k))
        })
        .cloned()
}

/// Puts each schedule found in code on the process that runs it: the one whose entry file holds it, a Celery
/// beat, a cron process with a command, the web process, else the first.
fn attach(processes: &mut [Process], schedules: Vec<code::Found>, files: &BTreeMap<String, String>) {
    for s in schedules {
        let file = at_key(&s.at).0;
        let i = processes
            .iter()
            .position(|p| stack::entry_file(&p.command, files).as_deref() == Some(file))
            .or_else(|| {
                (file.ends_with(".py"))
                    .then(|| processes.iter().position(|p| p.command.contains("beat")))
                    .flatten()
            })
            .or_else(|| processes.iter().position(|p| p.kind == "cron" && !p.command.is_empty()))
            .or_else(|| processes.iter().position(|p| p.kind == "web"))
            .or(if processes.is_empty() { None } else { Some(0) });
        if let Some(i) = i {
            let sched = exodus_schema::Schedule {
                cron: s.value,
                from: s.at,
            };
            processes[i].schedules.get_or_insert_with(Vec::new).push(sched);
        }
    }
    for p in processes.iter_mut() {
        if let Some(s) = p.schedules.as_mut() {
            s.sort_by(|a, b| at_key(&a.from).cmp(&at_key(&b.from)));
        }
        if p.kind == "cron" && p.schedules.is_none() {
            p.schedules = Some(Vec::new());
        }
    }
}

/// The second `web` becomes `web-2`, the third `web-3`.
fn dedupe<'a>(ids: impl Iterator<Item = &'a mut String>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for id in ids {
        let n = seen.entry(id.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            *id = format!("{id}-{n}");
        }
    }
}
