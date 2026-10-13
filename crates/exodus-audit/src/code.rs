//! What the code says that the manifests do not: the env names it reads, the timers it runs, and the stores it
//! connects to or writes when no manifest provisions them (Railway keeps those outside the repository).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use exodus_schema::Data;
use regex::Regex;

use crate::rules::Hit;
use crate::stack::{Role, found};
use crate::{Facts, at, at_key};

/// One hit: `file:line` and the value read there (an env name, a cron line).
#[derive(Debug, Clone)]
pub struct Found {
    pub at: String,
    pub value: String,
}

pub struct Code {
    /// (name, `file:line`) for every read of an env var, in file and line order.
    pub env: Vec<(String, String)>,
    pub schedules: Vec<Found>,
}

/// Files the code scan reads: source, templates, and YAML that may embed env reads (`database.yml`).
const CODE: &[&str] = &[
    "js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx", "py", "rb", "erb", "rake", "go", "rs", "php", "ex", "exs",
    "java", "kt", "cs", "yml", "yaml",
];

fn is_code(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| CODE.contains(&ext))
}

#[allow(clippy::unwrap_used)] // constant patterns; the tests compile every one
fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

const NAME: &str = "([A-Z][A-Z0-9_]*)";

static ENV: LazyLock<Regex> = LazyLock::new(|| {
    re(&[
        format!(r"process\.env\.{NAME}"),
        format!(r#"process\.env\[\s*['"]{NAME}['"]"#),
        format!(r"import\.meta\.env\.{NAME}"),
        format!(r#"os\.environ(?:\[|\.get\(|\.setdefault\(|\.pop\()\s*['"]{NAME}['"]"#),
        format!(r#"os\.getenv\(\s*['"]{NAME}['"]"#),
        format!(r#"\bENV(?:\[|\.fetch\(\s*)['"]{NAME}['"]"#),
        format!(r#"os\.(?:Getenv|LookupEnv)\(\s*"{NAME}""#),
        format!(r#"env::var(?:_os)?\(\s*"{NAME}""#),
        format!(r#"\b(?:getenv|env)\(\s*['"]{NAME}['"]"#),
        format!(r#"System\.(?:get_env|fetch_env!?)\(\s*"{NAME}""#),
    ]
    .join("|"))
});

const CRON: &str = r"((?:[0-9*/,\-]+\s+){4,5}[0-9*/,\-]+)";

/// Timers written as a cron line: node-cron and node-schedule, `cron`'s CronJob, robfig/cron, Laravel, Spring,
/// sidekiq-cron and Oban's crontab.
static CRON_CALL: LazyLock<Regex> = LazyLock::new(|| {
    re(&[
        format!(r#"\.schedule(?:Job)?\(\s*['"]{CRON}['"]"#),
        format!(r#"new\s+CronJob\(\s*['"]{CRON}['"]"#),
        format!(r#"\.AddFunc\(\s*"{CRON}""#),
        format!(r#"->cron\(\s*['"]{CRON}['"]"#),
        format!(r#"@Scheduled\(\s*cron\s*=\s*"{CRON}""#),
        format!(r#"\bcron:\s*['"]{CRON}['"]"#),
        format!(r#"\{{\s*"{CRON}"\s*,"#),
    ]
    .join("|"))
});
static CRONTAB: LazyLock<Regex> = LazyLock::new(|| re(r"\bcrontab\(([^)]*)\)"));
static KWARG: LazyLock<Regex> = LazyLock::new(|| re(r#"(\w+)\s*=\s*['"]?([^,'")]+)"#));
static GO_TICK: LazyLock<Regex> = LazyLock::new(|| re(r"time\.(?:NewTicker|Tick)\(\s*([^)]+)\)"));
static JS_INTERVAL: LazyLock<Regex> = LazyLock::new(|| re(r"setInterval\(.*,\s*(\d[\d_]*(?:\s*\*\s*\d[\d_]*)*)\s*\)"));
static RUST_INTERVAL: LazyLock<Regex> = LazyLock::new(|| re(r"interval\(\s*Duration::from_(secs|mins)\((\d+)\)"));

static SQLITE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"sql\.Open\(\s*"sqlite3?"|sqlite3\.connect\(|new\s+(?:Database|sqlite3\.Database)\(|Connection::open\(|SQLite3::Database\.new|django\.db\.backends\.sqlite3"#,
    )
});
static UPPER: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z][A-Z0-9_]{2,}\b"));

pub fn is_env_name(s: &str) -> bool {
    s.len() >= 2
        && s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `books-redis` -> `books-redis`, `Active Storage` -> `active-storage`.
pub fn slug(s: &str) -> String {
    let s: String = s
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.trim_matches('-').to_string()
}

/// A CI pipeline's file: its `cron:` lines schedule the CI, not the app, and its variables are the CI's.
fn is_ci(path: &str) -> bool {
    [
        ".github/",
        ".gitlab-ci",
        ".circleci/",
        "bitbucket-pipelines",
        ".buildkite/",
    ]
    .iter()
    .any(|c| path.starts_with(c))
}

fn code_lines(files: &BTreeMap<String, String>) -> impl Iterator<Item = (&str, usize, &str)> {
    files
        .iter()
        .filter(|(p, _)| is_code(p) && !is_ci(p))
        .flat_map(|(p, t)| t.lines().enumerate().map(move |(i, l)| (p.as_str(), i + 1, l)))
}

pub fn scan(files: &BTreeMap<String, String>) -> Code {
    let mut env = Vec::new();
    let mut schedules = Vec::new();
    for (path, n, line) in code_lines(files) {
        for c in ENV.captures_iter(line) {
            if let Some(m) = c.iter().skip(1).flatten().next() {
                env.push((m.as_str().to_string(), at(path, n)));
            }
        }
        if let Some(cron) = schedule(line) {
            schedules.push(Found {
                at: at(path, n),
                value: cron,
            });
        }
    }
    Code { env, schedules }
}

/// The cron line of a timer on this line, if it starts one.
fn schedule(line: &str) -> Option<String> {
    if let Some(c) = CRON_CALL.captures(line) {
        let expr = c.iter().skip(1).flatten().next()?.as_str();
        return Some(expr.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if let Some(c) = CRONTAB.captures(line) {
        let mut f: BTreeMap<String, String> = BTreeMap::new();
        for kw in KWARG.captures_iter(&c[1]) {
            f.insert(kw[1].to_string(), kw[2].trim().to_string());
        }
        let get = |k: &str| f.get(k).cloned().unwrap_or_else(|| "*".into());
        return Some(format!(
            "{} {} {} {} {}",
            get("minute"),
            get("hour"),
            get("day_of_month"),
            get("month_of_year"),
            get("day_of_week")
        ));
    }
    if let Some(c) = GO_TICK.captures(line) {
        let expr = &c[1];
        let unit = [("Millisecond", 0), ("Second", 1), ("Minute", 60), ("Hour", 3600)]
            .iter()
            .find(|(u, _)| expr.contains(&format!("time.{u}")))?;
        let n = product(expr);
        return Some(every(if unit.1 == 0 { n / 1000 } else { n * unit.1 }));
    }
    if let Some(c) = JS_INTERVAL.captures(line) {
        return Some(every(product(&c[1]) / 1000));
    }
    if let Some(c) = RUST_INTERVAL.captures(line) {
        let n: u64 = c[2].parse().ok()?;
        return Some(every(if &c[1] == "mins" { n * 60 } else { n }));
    }
    None
}

/// The product of the integers in an expression like `10 * time.Minute` or `60 * 1000`; 1 when there are none.
fn product(expr: &str) -> u64 {
    expr.split(|c: char| !(c.is_ascii_digit() || c == '_'))
        .filter_map(|n| n.replace('_', "").parse::<u64>().ok())
        .product()
}

/// An interval in seconds as the nearest cron line.
fn every(secs: u64) -> String {
    match secs {
        s if s >= 86_400 && s % 86_400 == 0 => match s / 86_400 {
            1 => "0 0 * * *".into(),
            d => format!("0 0 */{d} * *"),
        },
        s if s >= 3600 && s % 3600 == 0 => match s / 3600 {
            1 => "0 * * * *".into(),
            h => format!("0 */{h} * * *"),
        },
        s if s >= 120 && s % 60 == 0 => format!("*/{} * * * *", s / 60),
        _ => "* * * * *".into(),
    }
}

/// Which env names point at an engine's server.
fn points_at(engine: &str, name: &str) -> bool {
    match engine {
        "PostgreSQL" => name == "DATABASE_URL" || name.contains("POSTGRES") || name.starts_with("PG"),
        "MySQL" => name == "DATABASE_URL" || name.contains("MYSQL"),
        "MongoDB" => name.contains("MONGO"),
        "Redis" => name.contains("REDIS") || name.ends_with("BROKER_URL"),
        _ => false,
    }
}

/// Stores the dependencies use but no manifest provisions, an SQLite file, and the files the code writes when no
/// disk is provisioned for them (the `local-disk-write` hit, so a rule and the data item never disagree). Each
/// points at the code line that connects or writes (else the dependency).
pub fn data(f: &mut Facts, code: &Code, files: &BTreeMap<String, String>, hits: &[Hit]) {
    let stores = found(&f.deps, Role::Store);
    for (engine, id, kind, name) in [
        ("PostgreSQL", "postgres", "db", "Postgres"),
        ("MySQL", "mysql", "db", "MySQL"),
        ("MongoDB", "mongodb", "db", "MongoDB"),
        ("Redis", "redis", "cache", "Redis"),
    ] {
        let Some((_, dep)) = stores.iter().find(|(l, _)| l.badge == engine) else {
            continue;
        };
        if f.has_engine(engine) {
            continue;
        }
        let hit = code.env.iter().find(|(n, _)| points_at(engine, n));
        f.data.push(store(
            id,
            kind,
            name,
            engine,
            hit.map_or(dep.at.clone(), |h| h.1.clone()),
            hit.map(|h| h.0.clone()),
        ));
    }

    let sqlite_line = code_lines(files).find(|(_, _, l)| SQLITE.is_match(l));
    let sqlite_dep = stores.iter().find(|(l, _)| l.badge == "SQLite");
    let has_server_db = f.data.iter().any(|d| d.kind == "db");
    if (sqlite_line.is_some() || sqlite_dep.is_some()) && !has_server_db {
        let env = sqlite_line.and_then(|(p, n, _)| env_at(code, &at(p, n)));
        if let Some(mount) = f.data.iter_mut().find(|d| d.kind == "files") {
            // The volume the platform mounts is where the database file lives.
            mount.id = "sqlite".into();
            mount.kind = "sqlite".into();
            mount.engine = "SQLite".into();
            mount.env_name = env;
        } else {
            let from = sqlite_line
                .map(|(p, n, _)| at(p, n))
                .or_else(|| sqlite_dep.map(|d| d.1.at.clone()));
            f.data.push(store(
                "sqlite",
                "sqlite",
                "SQLite",
                "SQLite",
                from.unwrap_or_default(),
                env,
            ));
        }
    }

    if f.data.iter().any(|d| d.kind == "files") {
        return;
    }
    // a file the code creates where it opens the SQLite database is that database, not a second store
    let sqlite_file = sqlite_line.map(|(p, _, _)| p);
    let Some(hit) = hits
        .iter()
        .find(|h| h.rule.id == "local-disk-write" && Some(at_key(&h.at).0) != sqlite_file)
    else {
        return;
    };
    let mut d = if hit.code.contains("active_storage.service") {
        store("storage", "files", "Active Storage", "disk", hit.at.clone(), None)
    } else {
        let env = UPPER
            .find_iter(&hit.code)
            .map(|m| m.as_str())
            .find(|w| f.env.contains(*w))
            .map(String::from);
        let name = env
            .as_deref()
            .and_then(|e| default_of(&files[at_key(&hit.at).0], e))
            .or_else(|| dir_constant(&hit.code))
            .unwrap_or_else(|| "files".into());
        store(&slug(&name), "files", &name, "disk", hit.at.clone(), env)
    };
    d.id = slug(&d.id);
    f.data.push(d);
}

/// The env name read on a `file:line`, if one is.
fn env_at(code: &Code, at: &str) -> Option<String> {
    code.env.iter().find(|(_, a)| a == at).map(|(n, _)| n.clone())
}

/// The default the code gives an env var: `X = process.env.X || 'uploads'`, `os.environ.get("X", "uploads")`.
fn default_of(text: &str, env: &str) -> Option<String> {
    let pat = format!(r#"{}\s*=[^\n]*?(?:\|\||\?\?|,)\s*['"]([^'"]+)['"]"#, regex::escape(env));
    Regex::new(&pat)
        .ok()?
        .captures(text)
        .map(|c| c[1].trim_end_matches('/').to_string())
}

/// A folder constant on the line: `MEDIA_ROOT` -> `media`.
fn dir_constant(line: &str) -> Option<String> {
    UPPER.find_iter(line).find_map(|m| {
        ["_DIR", "_ROOT", "_PATH", "_FOLDER"]
            .iter()
            .find_map(|s| m.as_str().strip_suffix(s))
            .map(|base| base.to_ascii_lowercase())
    })
}

fn store(id: &str, kind: &str, name: &str, engine: &str, from: String, env_name: Option<String>) -> Data {
    Data {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        engine: engine.into(),
        version: None,
        size_mb: None,
        from,
        env_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_compile_and_read() {
        assert_eq!(
            schedule("cron.schedule('0 * * * *', cleanup);").as_deref(),
            Some("0 * * * *")
        );
        assert_eq!(
            schedule(r#""schedule": crontab(hour=2, minute=0),"#).as_deref(),
            Some("0 2 * * *")
        );
        assert_eq!(
            schedule("ticker := time.NewTicker(10 * time.Minute)").as_deref(),
            Some("*/10 * * * *")
        );
        assert_eq!(
            schedule("setInterval(tick, 60 * 60 * 1000);").as_deref(),
            Some("0 * * * *")
        );
        assert_eq!(
            schedule("let mut t = interval(Duration::from_secs(30));").as_deref(),
            Some("* * * * *")
        );
        assert!(schedule("app.schedule('nightly')").is_none());
        // a CI pipeline's cron is the CI's schedule, not the app's
        let files = BTreeMap::from([
            (
                ".github/workflows/nightly.yml".to_string(),
                "    - cron: '0 2 * * *'\n".to_string(),
            ),
            ("config/schedule.yml".to_string(), "  cron: '0 3 * * *'\n".to_string()),
        ]);
        let found: Vec<_> = scan(&files).schedules.into_iter().map(|s| s.value).collect();
        assert_eq!(found, ["0 3 * * *"]);
        let names: Vec<_> = ENV
            .captures_iter(r#"os.environ.setdefault("A_B", 1); ENV.fetch("PORT", 3000); os.Getenv("DB_PATH")"#)
            .filter_map(|c| c.iter().skip(1).flatten().next().map(|m| m.as_str().to_string()))
            .collect();
        assert_eq!(names, ["A_B", "PORT", "DB_PATH"]);
        assert!(SQLITE.is_match(r#"db, err := sql.Open("sqlite", os.Getenv("DB_PATH"))"#));
        assert_eq!(
            default_of("const UPLOAD_DIR = process.env.UPLOAD_DIR || 'uploads';", "UPLOAD_DIR").as_deref(),
            Some("uploads")
        );
        assert_eq!(
            dir_constant("path = settings.MEDIA_ROOT / name").as_deref(),
            Some("media")
        );
    }
}
