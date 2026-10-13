//! The pattern rules: data files in `rules/<language>/<id>.toml`, compiled into the binary, run over the
//! repository's text files. `exodus audit` turns their hits into `patterns` and `clean`; `exodus check` runs the
//! same set. How a rule is written: `rules/README.md`.

use std::collections::{BTreeMap, HashMap};

use exodus_schema::{Data, Pattern, Process};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::{at, at_key, place, stack};

/// A rule file as written.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    pub id: String,
    /// breaks, idle or note.
    pub kind: String,
    pub title: String,
    /// The `clean` label when the rule finds nothing ("in-process timers"); a platform note has none.
    pub clean: Option<String>,
    /// The rule runs only when the audit names this platform ("Heroku").
    pub platform: Option<String>,
    /// Globs of the files the rule reads, relative to the repository root.
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Matched against the whole file (multi-line mode: `^` and `$` are line ends). The hit's line is where the
    /// match starts, or where the group `(?P<at>...)` starts when the regex has one.
    pub regex: Vec<String>,
    /// A hit is dropped when the lines it spans match this ("store:" for a session with a store of its own). When
    /// the regex names the path it writes in a group `(?P<dest>...)`, this is matched against that name and against
    /// the lines above that assign it instead, so a temporary source does not hide a persistent copy.
    pub unless: Option<String>,
    /// A hit is dropped when a block it sits in opens with a line matching this (`def lifespan(` for a task started
    /// when the app starts, which no response precedes).
    pub unless_in: Option<String>,
    /// Text after the first occurrence of this is not read (Rust keeps its unit tests in the same file, after
    /// `#[cfg(test)]`).
    pub stop_at: Option<String>,
    pub why: String,
    pub fix: String,
    /// The fix in a framework's words, by its badge ("Django"): chosen when the finding's process runs that
    /// framework, else when the repository uses it; `fix` is the one for any stack.
    #[serde(default)]
    pub fixes: BTreeMap<String, String>,
    /// A knowledge base slug; the platform resolves it.
    pub kb: String,
    /// The kind of part a hit lands on: web, worker, cron, release (processes) or db, cache, files, sqlite (data).
    pub part: String,
    pub subject: Option<String>,
    /// The clause a firing rule adds to its part's `removed` ("the queue goes to Cloud Tasks").
    pub takes_over: Option<String>,
    #[serde(default)]
    pub stay: bool,
    pub task: Option<Task>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub title: String,
    pub detail: String,
    /// S, M or L.
    pub effort: String,
    /// 1 Prepare, 2 Change the code, 3 Stand up on Google Cloud, 4 Cut over.
    pub phase: u8,
}

/// A rule ready to run.
#[derive(Debug, Clone)]
pub struct Rule {
    pub file: RuleFile,
    include: GlobSet,
    exclude: GlobSet,
    regex: Vec<Regex>,
    unless: Option<Regex>,
    unless_in: Option<Regex>,
}

impl Rule {
    /// Parses and compiles one rule file; the error names what is wrong.
    pub fn parse(text: &str) -> Result<Rule, String> {
        let file: RuleFile = toml::from_str(text).map_err(|e| e.to_string())?;
        if !["breaks", "idle", "note"].contains(&file.kind.as_str()) {
            return Err(format!("{}: kind must be breaks, idle or note", file.id));
        }
        if file.regex.is_empty() || file.include.is_empty() {
            return Err(format!("{}: needs at least one regex and one include glob", file.id));
        }
        let globs = |list: &[String]| {
            let mut b = GlobSetBuilder::new();
            for g in list {
                b.add(Glob::new(g).map_err(|e| format!("{}: {e}", file.id))?);
            }
            b.build().map_err(|e| format!("{}: {e}", file.id))
        };
        let compile = |r: &str| {
            RegexBuilder::new(r)
                .multi_line(true)
                .build()
                .map_err(|e| format!("{}: {e}", file.id))
        };
        Ok(Rule {
            include: globs(&file.include)?,
            exclude: globs(&file.exclude)?,
            regex: file.regex.iter().map(|r| compile(r)).collect::<Result<_, _>>()?,
            unless: file.unless.as_deref().map(compile).transpose()?,
            unless_in: file.unless_in.as_deref().map(compile).transpose()?,
            file,
        })
    }

    /// The first line the rule fires on in `path`, or None when the rule does not read the file. A file off the
    /// server is read so that `clean` can name it; one the rule excludes is read only when it is off the server.
    fn hit<'t>(&self, path: &str, text: &'t str, off_server: bool) -> Option<(usize, &'t str)> {
        if !self.include.is_match(path) || (!off_server && self.exclude.is_match(path)) {
            return None;
        }
        let text = match &self.file.stop_at {
            Some(stop) => text.split(stop.as_str()).next().unwrap_or(text),
            None => text,
        };
        self.first_hit(text)
    }

    /// The first line in `text` the rule fires on, with that line.
    fn first_hit<'t>(&self, text: &'t str) -> Option<(usize, &'t str)> {
        if !self.regex.iter().any(|r| r.is_match(text)) {
            return None;
        }
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let line_of = |offset: usize| starts.partition_point(|&s| s <= offset);
        let line_text = |n: usize| {
            let end = starts.get(n).map_or(text.len(), |&e| e - 1);
            text[starts[n - 1]..end].trim_end_matches('\r')
        };
        let mut best: Option<usize> = None;
        for re in &self.regex {
            for c in re.captures_iter(text) {
                let Some(m) = c.get(0) else { continue };
                // A match may begin with the newline a `[^.\w]` consumed; its line is where the text starts.
                let lead = m.as_str().len() - m.as_str().trim_start().len();
                let n = line_of(c.name("at").map_or(m.start() + lead, |a| a.start()));
                if best.is_some_and(|b| b <= n) {
                    break;
                }
                if is_comment(line_text(n)) {
                    continue;
                }
                let spanned = &text[starts[line_of(m.start()) - 1]
                    ..starts
                        .get(line_of(m.end().max(m.start() + 1) - 1))
                        .copied()
                        .unwrap_or(text.len())];
                // A write names where it goes in the group `dest`: that is what `unless` reads, the name itself and the
                // assignment of it a few lines up, not the source on the same line.
                let temp = match (c.name("dest"), &self.unless) {
                    (Some(d), Some(u)) => scratch(u, text, n, d.as_str()),
                    (_, Some(u)) => u.is_match(spanned),
                    _ => false,
                };
                if temp {
                    continue;
                }
                if self
                    .unless_in
                    .as_ref()
                    .is_some_and(|u| enclosing(text, n).iter().any(|l| u.is_match(l)))
                {
                    continue;
                }
                best = Some(n);
                break;
            }
        }
        best.map(|n| (n, line_text(n)))
    }
}

/// Whether `dest`, written to on line `n` (from 1), is named for `unless` (a temporary path) or was assigned from
/// something that is, within the ten lines above.
fn scratch(unless: &Regex, text: &str, n: usize, dest: &str) -> bool {
    if unless.is_match(dest) {
        return true;
    }
    // only a line above that is temporary can make `dest` so: compile the assignment's pattern only for those
    let temporary: Vec<&str> = text
        .lines()
        .take(n - 1)
        .skip((n - 1).saturating_sub(10))
        .filter(|l| unless.is_match(l))
        .collect();
    if temporary.is_empty() {
        return false;
    }
    Regex::new(&format!(r"(?:^|[\s(,]){}\s*(?::=|=)[^=]", regex::escape(dest)))
        .is_ok_and(|assign| temporary.iter().any(|l| assign.is_match(l)))
}

/// The lines that open the blocks line `n` (from 1) sits in: walking up, each line indented less than the one taken
/// before it.
fn enclosing(text: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let Some(first) = lines.get(n - 1) else {
        return Vec::new();
    };
    let mut at = indent(first);
    let mut out = Vec::new();
    for l in lines[..n - 1].iter().rev().filter(|l| !l.trim().is_empty()) {
        if indent(l) < at {
            at = indent(l);
            out.push(*l);
        }
    }
    out
}

fn is_comment(line: &str) -> bool {
    let l = line.trim_start();
    ["//", "/*", "* ", "--"].iter().any(|c| l.starts_with(c)) || (l.starts_with('#') && !l.starts_with("#["))
}

macro_rules! builtin {
    ($($path:literal),* $(,)?) => {
        /// The rule files compiled in, in the order `clean` lists them.
        pub const BUILTIN: &[(&str, &str)] = &[$(($path, include_str!(concat!("../rules/", $path)))),*];
    };
}

builtin![
    "node/queue-worker.toml",
    "python/queue-worker.toml",
    "ruby/queue-worker.toml",
    "go/queue-worker.toml",
    "elixir/queue-worker.toml",
    "node/work-after-response.toml",
    "python/work-after-response.toml",
    "ruby/work-after-response.toml",
    "node/in-process-timer.toml",
    "python/in-process-timer.toml",
    "ruby/in-process-timer.toml",
    "go/in-process-timer.toml",
    "elixir/in-process-timer.toml",
    "rust/in-process-timer.toml",
    "node/local-disk-write.toml",
    "python/local-disk-write.toml",
    "ruby/local-disk-write.toml",
    "go/local-disk-write.toml",
    "rust/local-disk-write.toml",
    "php/local-disk-write.toml",
    "elixir/local-disk-write.toml",
    "node/memory-sessions.toml",
    "python/memory-sessions.toml",
    "ruby/memory-sessions.toml",
    "go/memory-sessions.toml",
    "go/memory-state.toml",
    "rust/memory-state.toml",
    "node/redis-sessions.toml",
    "python/redis-sessions.toml",
    "ruby/redis-sessions.toml",
    "go/redis-sessions.toml",
    "node/long-lived-connection.toml",
    "python/long-lived-connection.toml",
    "ruby/long-lived-connection.toml",
    "go/long-lived-connection.toml",
    "elixir/long-lived-connection.toml",
    "rust/long-lived-connection.toml",
    "any/release-migrations.toml",
    "node/background-thread.toml",
    "python/background-thread.toml",
    "ruby/background-thread.toml",
    "go/background-thread.toml",
    "rust/background-thread.toml",
    "node/sqlite-database.toml",
    "python/sqlite-database.toml",
    "ruby/sqlite-database.toml",
    "go/sqlite-database.toml",
    "rust/sqlite-database.toml",
    "any/heroku-scheduler.toml",
];

/// The compiled-in rules. A broken file is a bug the tests catch, so it is skipped here rather than failing an
/// audit.
pub fn builtin() -> Vec<Rule> {
    BUILTIN.iter().filter_map(|(_, text)| Rule::parse(text).ok()).collect()
}

/// One hit: the rule, `file:line` and the line.
#[derive(Clone)]
pub struct Hit<'r> {
    pub rule: &'r RuleFile,
    pub at: String,
    pub code: String,
}

/// Runs the rules that apply to `platform` over every file: one hit per rule and file, the first, in code that runs
/// on the server. `runs` are the files a process starts, on the server wherever they live. The second value is the
/// places with a hit that do not run on the server, each with what runs there (`place::off_server`).
pub fn run<'r>(
    rules: &'r [Rule],
    files: &BTreeMap<String, String>,
    platform: &str,
    runs: &[String],
) -> (Vec<Hit<'r>>, BTreeMap<String, &'static str>) {
    let placer = place::Placer::new(files);
    let place: BTreeMap<&str, Option<(String, &'static str)>> = files
        .keys()
        .map(|p| {
            let off = if runs.contains(p) { None } else { placer.off_server(p) };
            (p.as_str(), off)
        })
        .collect();
    let mut hits: Vec<Hit> = Vec::new();
    let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
    let mut off_server: BTreeMap<String, &'static str> = BTreeMap::new();
    for rule in rules
        .iter()
        .filter(|r| r.file.platform.as_deref().is_none_or(|p| p == platform))
    {
        for (path, text) in files {
            let off = place[path.as_str()].as_ref();
            let Some((n, line)) = rule.hit(path, text, off.is_some()) else {
                continue;
            };
            if let Some((at, why)) = off {
                off_server.insert(at.clone(), why);
                continue;
            }
            let at = at(path, n);
            // The same rule id in two language folders may both read one file: the earlier line wins.
            if let Some(&i) = seen.get(&(rule.file.id.as_str(), path.as_str())) {
                if n < at_key(&hits[i].at).1 {
                    hits[i].at = at;
                    hits[i].code = one_line(line);
                }
                continue;
            }
            seen.insert((rule.file.id.as_str(), path.as_str()), hits.len());
            hits.push(Hit {
                rule: &rule.file,
                at,
                code: one_line(line),
            });
        }
    }
    hits.sort_by(|a, b| at_key(&a.at).cmp(&at_key(&b.at)).then(a.rule.id.cmp(&b.rule.id)));
    // A folder named stands for the places inside it that run in the same place.
    let folders: Vec<(String, &str)> = off_server
        .iter()
        .filter(|(p, _)| p.ends_with('/'))
        .map(|(p, w)| (p.clone(), *w))
        .collect();
    off_server.retain(|p, w| {
        !folders
            .iter()
            .any(|(f, fw)| p != f && p.starts_with(f.as_str()) && w == fw)
    });
    (hits, off_server)
}

/// The line trimmed and cut to 200 characters (findings/1 `code`).
fn one_line(line: &str) -> String {
    crate::redact::line(line.trim()).chars().take(200).collect()
}

/// A report lists this many places per rule; a large app has a hundred job classes, and the first five say what
/// the rest would. `exodus check` and the corpus test ask for all of them (`audit_with`'s `limit`).
pub const PLACES_PER_RULE: usize = 5;

/// The hits as findings' patterns, each on its part: the process or data item at that line, the process that runs
/// the file or the schedule, else the first of the rule's kind, else the web process. `frameworks` are the
/// repository's web frameworks, for the fix in its words. At most `limit` places per rule.
pub fn patterns(
    hits: &[Hit],
    processes: &[Process],
    data: &[Data],
    files: &BTreeMap<String, String>,
    frameworks: &[&str],
    limit: usize,
) -> Vec<Pattern> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    hits.iter()
        .filter_map(|h| {
            let n = seen.entry(h.rule.id.as_str()).or_insert(0);
            *n += 1;
            if *n > limit {
                return None;
            }
            let part = part_of(&h.rule.part, &h.at, processes, data, files);
            let own = processes
                .iter()
                .find(|p| p.id == part)
                .and_then(|p| p.framework.as_deref());
            let fix = own
                .into_iter()
                .chain(frameworks.iter().copied())
                .find_map(|f| h.rule.fixes.get(f))
                .unwrap_or(&h.rule.fix);
            Some(Pattern {
                id: if *n == 1 {
                    h.rule.id.clone()
                } else {
                    format!("{}-{n}", h.rule.id)
                },
                rule: h.rule.id.clone(),
                kind: h.rule.kind.clone(),
                title: h.rule.title.clone(),
                at: h.at.clone(),
                code: h.code.clone(),
                why: h.rule.why.clone(),
                fix: fix.clone(),
                part,
                kb: h.rule.kb.clone(),
                subject: h.rule.subject.clone(),
            })
        })
        .collect()
}

fn part_of(kind: &str, at: &str, processes: &[Process], data: &[Data], files: &BTreeMap<String, String>) -> String {
    if ["db", "cache", "files", "sqlite"].contains(&kind) {
        return data
            .iter()
            .find(|d| d.from == at)
            .or_else(|| data.iter().find(|d| d.kind == kind))
            .map_or_else(|| kind.to_string(), |d| d.id.clone());
    }
    let file = at_key(at).0;
    processes
        .iter()
        .find(|p| p.from == at || p.schedules.iter().flatten().any(|s| s.from == at))
        .or_else(|| {
            processes
                .iter()
                .find(|p| stack::entry_file(&p.command, files).as_deref() == Some(file))
        })
        .or_else(|| processes.iter().find(|p| p.kind == kind))
        .or_else(|| processes.iter().find(|p| p.kind == "web"))
        .map_or_else(|| kind.to_string(), |p| p.id.clone())
}

/// The labels of the rules that ran and found nothing, in rule order; platform notes have no label.
pub fn clean(rules: &[Rule], hits: &[Hit]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in rules {
        let Some(label) = &r.file.clean else { continue };
        if !hits.iter().any(|h| h.rule.id == r.file.id) && !out.contains(label) {
            out.push(label.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    #[test]
    fn every_rule_file_is_compiled_in_and_parses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rules");
        let mut on_disk = Vec::new();
        for lang in std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.path().is_dir()) {
            for f in std::fs::read_dir(lang.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                if name.ends_with(".toml") {
                    on_disk.push(format!("{}/{name}", lang.file_name().to_string_lossy()));
                }
            }
        }
        on_disk.sort();
        let mut listed: Vec<String> = BUILTIN.iter().map(|(p, _)| p.to_string()).collect();
        listed.sort();
        assert_eq!(on_disk, listed, "rules/ and BUILTIN differ");
        for (path, text) in BUILTIN {
            let rule = Rule::parse(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            let stem = path.rsplit('/').next().unwrap().trim_end_matches(".toml");
            assert_eq!(rule.file.id, stem, "{path}: the id is the file name");
        }
    }

    static RULES: LazyLock<Vec<Rule>> = LazyLock::new(builtin);

    fn fires(id: &str, path: &str, text: &str) -> Option<usize> {
        let files = BTreeMap::from([(path.to_string(), text.to_string())]);
        let off = place::Placer::new(&files).off_server(path);
        RULES
            .iter()
            .filter(|r| r.file.id == id)
            .filter(|_| off.is_none())
            .filter_map(|r| r.hit(path, text, false))
            .map(|h| h.0)
            .min()
    }

    #[test]
    fn the_fix_follows_the_stack() {
        let rule = RULES
            .iter()
            .find(|r| r.file.id == "local-disk-write" && r.file.include.iter().any(|g| g.ends_with(".py")))
            .unwrap();
        let hit = Hit {
            rule: &rule.file,
            at: "tool.py:1".into(),
            code: String::new(),
        };
        let fix = |frameworks: &[&str]| {
            patterns(
                std::slice::from_ref(&hit),
                &[],
                &[],
                &BTreeMap::new(),
                frameworks,
                PLACES_PER_RULE,
            )[0]
            .fix
            .clone()
        };
        assert!(fix(&["Django"]).contains("django-storages"));
        assert!(!fix(&[]).contains("django"));
        assert!(!fix(&["Flask"]).contains("django"));
    }

    #[test]
    fn hits_off_the_server_are_named_not_raised() {
        let files: BTreeMap<String, String> = [
            ("website/start/carousel.js", "setInterval(next, 5000);\n"),
            ("website/build.py", "with open(out, 'w') as f:\n"),
            ("src/jobs.js", "setInterval(sync, 60000);\n"),
        ]
        .iter()
        .map(|(p, t)| (p.to_string(), t.to_string()))
        .collect();
        let (hits, off) = run(&RULES, &files, "Unknown", &[]);
        let at: Vec<&str> = hits.iter().map(|h| h.at.as_str()).collect();
        assert_eq!(at, ["src/jobs.js:1"]);
        assert_eq!(off.get("website/"), Some(&place::BROWSER));
        assert_eq!(off.get("website/build.py"), Some(&place::TOOLS));
        // A process that runs a script puts it on the server.
        let (hits, _) = run(&RULES, &files, "Unknown", &["website/build.py".into()]);
        assert!(hits.iter().any(|h| h.at == "website/build.py:1"));
    }

    #[test]
    fn rust_and_go_rules_tell_schedules_from_heartbeats() {
        // A job scheduler and a tokio interval loop are timers; an interval inside one WebSocket is a heartbeat.
        assert_eq!(
            fires(
                "in-process-timer",
                "src/main.rs",
                "fn jobs() {\n    let mut sched = JobScheduler::new();\n}\n"
            ),
            Some(2)
        );
        let ws = "fn hub() {\n    rocket_ws::Stream! { ws => {\n        let mut interval = tokio::time::interval(Duration::from_secs(15));\n    }}\n}\n";
        assert_eq!(fires("in-process-timer", "src/api/ws.rs", ws), None);
        let tick = "async fn poll() {\n    let mut interval = tokio::time::interval(Duration::from_secs(60));\n}\n";
        assert_eq!(fires("in-process-timer", "src/poll.rs", tick), Some(2));
        // A spawned loop is background work; a task waiting on a signal is not.
        let lp = "fn start() {\n    tokio::spawn(async move {\n        loop {\n            scan().await;\n        }\n    });\n}\n";
        assert_eq!(fires("background-thread", "src/worker.rs", lp), Some(2));
        let sig = "fn start() {\n    tokio::spawn(async move { let mut s = signal(SignalKind::user_defined1()).unwrap(); loop { s.recv().await; } });\n}\n";
        assert_eq!(fires("background-thread", "src/main.rs", sig), None);
        // WebSockets, SQLite, opendal's local filesystem, tokens in a static map.
        assert_eq!(
            fires(
                "long-lived-connection",
                "src/ws.rs",
                "async fn ws(ws: WebSocketUpgrade) {}\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires(
                "sqlite-database",
                "src/db.rs",
                "let c = rusqlite::Connection::open(\"app.db\")?;\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires(
                "sqlite-database",
                "src/db.rs",
                "let c = rusqlite::Connection::open(\":memory:\")?;\n"
            ),
            None
        );
        assert_eq!(
            fires(
                "local-disk-write",
                "src/storage.rs",
                "let b = opendal::services::Fs::default().root(path);\n"
            ),
            Some(1)
        );
        let tokens =
            "static RESET_TOKENS: LazyLock<Mutex<HashMap<String, Token>>> = LazyLock::new(Default::default);\n";
        assert_eq!(fires("memory-state", "src/auth.rs", tokens), Some(1));
        let go = "var (\n\ttokens = make(map[string]Token)\n\tmu     sync.RWMutex\n)\n";
        assert_eq!(fires("memory-state", "internal/tmptokens/tmptokens.go", go), Some(2));
        assert_eq!(fires("memory-state", "internal/tmptokens/tmptokens_test.go", go), None);
    }

    #[test]
    fn rules_fire_where_they_should_and_not_elsewhere() {
        // Work after the response: the line after res.send, not a conditional early return.
        let after = "app.post('/x', (req, res) => {\n  res.json({ ok: true });\n  sendEmail(req.body);\n});\n";
        assert_eq!(fires("work-after-response", "src/a.js", after), Some(3));
        let early = "  if (!ok) return res.status(401).end();\n  req.session.userId = 1;\n  res.end();\n});\n";
        assert_eq!(fires("work-after-response", "src/a.js", early), None);
        // A session with a store of its own is fine, across lines too.
        let stored = "app.use(session({\n  store: new RedisStore({ client }),\n  secret: s,\n}));\n";
        assert_eq!(fires("memory-sessions", "src/a.js", stored), None);
        assert_eq!(
            fires("memory-sessions", "src/a.js", "app.use(session({ secret: s }));\n"),
            Some(1)
        );
        // A temporary file inside one request may stay.
        assert_eq!(
            fires("local-disk-write", "a.py", "with open('/tmp/x', 'w') as f:\n"),
            None
        );
        assert_eq!(
            fires("local-disk-write", "a.py", "with open(path, 'wb') as f:\n"),
            Some(1)
        );
        // worker_threads is a thread; a BullMQ Worker is a queue worker.
        let threads =
            "import { Worker } from 'node:worker_threads';\nnew Worker(new URL('./w.js', import.meta.url));\n";
        assert_eq!(fires("queue-worker", "a.js", threads), None);
        assert_eq!(
            fires("queue-worker", "src/w.ts", "return new Worker(url, options);\n"),
            None
        );
        assert_eq!(
            fires(
                "queue-worker",
                "src/w.ts",
                "queue()\n  .process(\n    async function (job) {\n"
            ),
            Some(2)
        );
        assert_eq!(
            fires("queue-worker", "src/w.ts", "q()\n  .process(\n    env.CONCURRENCY,\n"),
            Some(2)
        );
        assert_eq!(fires("queue-worker", "src/w.ts", "await resolver.process();\n"), None);
        assert_eq!(
            fires("local-disk-write", "src/a.ts", "  async writeFile(file: File) {\n"),
            None
        );
        assert_eq!(
            fires("local-disk-write", "src/a.ts", "fs.createWriteStream(tmpFile)\n"),
            None
        );
        assert_eq!(fires("background-thread", "a.js", threads), Some(1));
        // Go: a goroutine started on a long-running loop is one, a goroutine per request is not; a ticker named for
        // a timeout bounds a select; a config file written at setup and an archive unpacked to read it are not data.
        assert_eq!(
            fires(
                "background-thread",
                "m.go",
                "\tgo m.scanCampaigns(m.cfg.ScanInterval)\n\tgo m.worker()\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires("background-thread", "h.go", "\tgo func() {\n\t\tsendMail(u)\n\t}()\n"),
            None
        );
        assert_eq!(
            fires("in-process-timer", "m.go", "\tt := time.NewTicker(pushTimeout)\n"),
            None
        );
        assert_eq!(
            fires("in-process-timer", "m.go", "\tt := time.NewTicker(tick)\n"),
            Some(1)
        );
        assert_eq!(
            fires(
                "local-disk-write",
                "i.go",
                "func newConfigFile(path string) error {\n\treturn os.WriteFile(path, b, 0644)\n}\n"
            ),
            None
        );
        assert_eq!(
            fires(
                "local-disk-write",
                "z.go",
                "func (s *Session) ExtractZIP(src string) error {\n\tout, err := os.OpenFile(dir+\"/\"+f, os.O_CREATE, 0644)\n}\n"
            ),
            None
        );
        assert_eq!(
            fires(
                "local-disk-write",
                "f.go",
                "func (c *Client) Put(name string) error {\n\tout, err := os.OpenFile(filepath.Join(dir, name), os.O_CREATE, 0664)\n}\n"
            ),
            Some(2)
        );
        // A promise left unawaited after the response is work after it; `void` alone, `void 0` and an assigned task are not.
        assert_eq!(
            fires("work-after-response", "a.ts", "res.send(1);\nvoid sendEmail(u);\n"),
            Some(2)
        );
        assert_eq!(fires("work-after-response", "a.ts", "void store.refresh();\n"), None);
        assert_eq!(fires("work-after-response", "a.ts", "const z = void 0;\n"), None);
        assert_eq!(
            fires("work-after-response", "a.py", "t = asyncio.create_task(f())\nawait t\n"),
            None
        );
        assert_eq!(
            fires("work-after-response", "a.py", "asyncio.create_task(f())\n"),
            Some(1)
        );
        // A cache of users by hand is a memory session; a list read from a store is not.
        assert_eq!(
            fires("memory-sessions", "a.js", "const sessions = new Map();\n"),
            Some(1)
        );
        assert_eq!(
            fires("memory-sessions", "a.js", "const sessions = await store.all();\n"),
            None
        );
        assert_eq!(fires("memory-sessions", "a.py", "    sessions = {}\n"), None);
        // Redis for sessions is idle spending, not a break; a session with its own store is not in-memory.
        assert_eq!(
            fires(
                "redis-sessions",
                "a.js",
                "const RedisStore = require('connect-redis')(session);\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires(
                "redis-sessions",
                "a.js",
                "const q = new Queue('x', { connection: redis });\n"
            ),
            None
        );
        assert_eq!(fires("queue-worker", "a.py", "from rq import Queue\n"), Some(1));
        // Comments and tests are not code that runs.
        assert_eq!(
            fires("in-process-timer", "a.js", "// cron.schedule('0 * * * *', f)\n"),
            None
        );
        assert_eq!(
            fires("in-process-timer", "src/a.test.js", "setInterval(f, 1000);\n"),
            None
        );
        // A goroutine alone is not a background thread; a ticker is a timer.
        assert_eq!(fires("background-thread", "main.go", "go serve()\n"), None);
        assert_eq!(
            fires("in-process-timer", "main.go", "t := time.NewTicker(time.Minute)\n"),
            Some(1)
        );
        assert_eq!(
            fires("long-lived-connection", "a.py", "return StreamingHttpResponse(gen())\n"),
            Some(1)
        );
        assert_eq!(
            fires(
                "long-lived-connection",
                "app/channels/application_cable/channel.rb",
                "class Channel < ActionCable::Channel::Base\n"
            ),
            None
        );
    }

    #[test]
    fn a_write_is_judged_by_where_it_goes_not_by_what_it_copies() {
        let disk = |path: &str, text: &str| fires("local-disk-write", path, text);
        // a copy into a file object is the open() that made it; a scratch path named above is scratch
        assert_eq!(disk("a.py", "shutil.copyfileobj(upload.file, buffer)\n"), None);
        assert_eq!(disk("a.py", "with dest.open(\"wb\") as buffer:\n"), Some(1));
        let scratch = "tmp = Path(base).joinpath(name)\nwith tmp.open(\"wb\") as b:\n";
        assert_eq!(disk("a.py", scratch), None);
        let named = "temp_img = temp_path.joinpath(name)\nwith temp_img.open(\"wb\") as b:\n";
        assert_eq!(disk("a.py", named), None);
        // a temporary source does not hide a persistent copy
        assert_eq!(disk("a.py", "shutil.copy(self.temp_path, export_path)\n"), Some(1));
        let moved = "dest = users_dir / \"profile.webp\"\nshutil.move(tmp_image, dest)\n";
        assert_eq!(disk("a.py", moved), Some(2));
        assert_eq!(disk("a.py", "shutil.copy(src, tmp_dir / name)\n"), None);
        assert_eq!(
            disk(
                "a.mjs",
                "const env = path.join(tmpDir, 'x');\nfs.writeFileSync(env, text);\n"
            ),
            None
        );
        assert_eq!(disk("a.mjs", "fs.writeFileSync(destination, text);\n"), Some(1));
        assert_eq!(
            disk(
                "a.php",
                "$p = artifact_path('tmp/' . $id);\n$n = file_put_contents($p, $s);\n"
            ),
            None
        );
        assert_eq!(disk("a.php", "File::put($file, $log);\n"), Some(1));
        assert_eq!(disk("a.php", "$file = $request->file->move($dir);\n"), Some(1));
        assert_eq!(disk("a.php", "Storage::disk('s3')->move($from, $to);\n"), None);
    }

    #[test]
    fn code_that_starts_with_the_app_follows_no_response() {
        let after = |text: &str| fires("work-after-response", "a.py", text);
        assert_eq!(after("async def handler(r):\n    asyncio.create_task(f())\n"), Some(2));
        assert_eq!(after("async def start():\n    asyncio.create_task(f())\n"), None);
        assert_eq!(
            after("async def lifespan(app):\n    try:\n        if on:\n            asyncio.ensure_future(f())\n"),
            None
        );
    }

    #[test]
    fn rules_read_the_idioms_of_their_stacks() {
        let oban = "config :app, Oban,\n  queues: [default: 10]\n";
        assert_eq!(fires("queue-worker", "config/runtime.exs", oban), Some(1));
        assert_eq!(fires("queue-worker", "config/test.exs", oban), None);
        assert_eq!(
            fires(
                "in-process-timer",
                "lib/a.ex",
                "Process.send_after(self(), :tick, 1000)\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires(
                "in-process-timer",
                "lib/a_live.ex",
                "Process.send_after(self(), :tick, 1000)\n"
            ),
            None
        );
        let warmers = "warmers: [\n  all: {Site.Cache.All,\n   interval: :timer.minutes(15)}\n]\n";
        assert_eq!(fires("in-process-timer", "lib/application.ex", warmers), Some(3));
        assert_eq!(
            fires(
                "in-process-timer",
                "lib/auth.ex",
                "login: %{limit: 5, interval: :timer.seconds(60)}\n"
            ),
            None
        );
        assert_eq!(
            fires(
                "long-lived-connection",
                "lib/endpoint.ex",
                "  socket(\"/live\", Phoenix.LiveView.Socket,\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires("in-process-timer", "src/a.ts", "  @Cron(CronExpression.EVERY_WEEK)\n"),
            Some(1)
        );
        assert_eq!(
            fires(
                "in-process-timer",
                "src/a.ts",
                "new CronJob<null, null>(\n  '0 * * * *',\n"
            ),
            Some(1)
        );
        assert_eq!(
            fires(
                "in-process-timer",
                "config/schedule.yml",
                "job:\n  cron: \"0 * * * *\"\n"
            ),
            Some(2)
        );
        assert_eq!(
            fires(
                "long-lived-connection",
                "app/views/a.html.erb",
                "<%= turbo_stream_from @chat %>\n"
            ),
            Some(1)
        );
        let local = "config.active_storage.service = ENV.fetch(\"SERVICE\", \"local\").to_sym\n";
        assert_eq!(
            fires("local-disk-write", "config/environments/production.rb", local),
            Some(1)
        );
        assert_eq!(
            fires("local-disk-write", "config/environments/development.rb", local),
            None
        );
        assert_eq!(
            fires("sqlite-database", "a.py", "url = f\"sqlite:///{path}\"\n"),
            Some(1)
        );
        assert_eq!(
            fires("sqlite-database", "a.py", "p = url.removeprefix(\"sqlite:///\")\n"),
            None
        );
    }
}
