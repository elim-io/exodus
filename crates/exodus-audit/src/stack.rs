//! The stack: which libraries matter (frameworks, servers, job runners, timers, store drivers), the badges, each
//! process's framework, and the container the stack runs in on Cloud Run. The facts behind the tables (which files
//! name a stack, which stacks Google's buildpacks build) are in the platform repository's research notes on stacks.

use std::collections::BTreeMap;

use exodus_schema::{Platform, Process, Runtime};

use crate::Facts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eco {
    Node,
    Python,
    Ruby,
    Go,
    Rust,
    Php,
    Elixir,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub eco: Eco,
    pub name: String,
    /// The major version, when the manifest pins one.
    pub version: Option<String>,
    /// `file:line` of the manifest line that declares it.
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Web,
    Server,
    Worker,
    Timer,
    /// A chat bot that holds one connection to its platform (long polling or a gateway) and must run as one copy.
    Bot,
    /// A driver of a store; the badge is the engine.
    Store,
}

pub struct Lib {
    pub eco: Eco,
    /// The package name; for Go a module path prefix.
    pub dep: &'static str,
    pub badge: &'static str,
    pub role: Role,
}

const fn lib(eco: Eco, dep: &'static str, badge: &'static str, role: Role) -> Lib {
    Lib { eco, dep, badge, role }
}

use Eco::*;
use Role::*;

/// Badges within a role follow this order, whatever order the manifest lists them in.
pub const LIBS: &[Lib] = &[
    lib(Node, "express", "Express", Web),
    lib(Node, "fastify", "Fastify", Web),
    lib(Node, "@nestjs/core", "NestJS", Web),
    lib(Node, "next", "Next.js", Web),
    lib(Node, "@remix-run/node", "Remix", Web),
    lib(Node, "@react-router/node", "React Router", Web),
    lib(Node, "@slack/bolt", "Bolt for Slack", Web),
    lib(Node, "hono", "Hono", Web),
    lib(Node, "koa", "Koa", Web),
    lib(Node, "bullmq", "BullMQ", Worker),
    lib(Node, "bull", "Bull", Worker),
    lib(Node, "agenda", "Agenda", Worker),
    lib(Node, "node-cron", "node-cron", Timer),
    lib(Node, "cron", "cron", Timer),
    lib(Node, "node-schedule", "node-schedule", Timer),
    lib(Node, "telegraf", "Telegraf", Bot),
    lib(Node, "grammy", "grammY", Bot),
    lib(Node, "node-telegram-bot-api", "node-telegram-bot-api", Bot),
    lib(Node, "discord.js", "discord.js", Bot),
    lib(Node, "pg", "PostgreSQL", Store),
    lib(Node, "postgres", "PostgreSQL", Store),
    lib(Node, "mysql2", "MySQL", Store),
    lib(Node, "mongodb", "MongoDB", Store),
    lib(Node, "mongoose", "MongoDB", Store),
    lib(Node, "ioredis", "Redis", Store),
    lib(Node, "redis", "Redis", Store),
    lib(Node, "bullmq", "Redis", Store),
    lib(Node, "bull", "Redis", Store),
    lib(Node, "better-sqlite3", "SQLite", Store),
    lib(Node, "sqlite3", "SQLite", Store),
    lib(Python, "django", "Django", Web),
    lib(Python, "flask", "Flask", Web),
    lib(Python, "fastapi", "FastAPI", Web),
    lib(Python, "gunicorn", "Gunicorn", Server),
    lib(Python, "uvicorn", "Uvicorn", Server),
    lib(Python, "celery", "Celery", Worker),
    lib(Python, "rq", "RQ", Worker),
    lib(Python, "dramatiq", "Dramatiq", Worker),
    lib(Python, "apscheduler", "APScheduler", Timer),
    lib(Python, "python-telegram-bot", "python-telegram-bot", Bot),
    lib(Python, "aiogram", "aiogram", Bot),
    lib(Python, "pytelegrambotapi", "pyTelegramBotAPI", Bot),
    lib(Python, "discord.py", "discord.py", Bot),
    lib(Python, "psycopg", "PostgreSQL", Store),
    lib(Python, "psycopg2", "PostgreSQL", Store),
    lib(Python, "psycopg2-binary", "PostgreSQL", Store),
    lib(Python, "asyncpg", "PostgreSQL", Store),
    lib(Python, "mysqlclient", "MySQL", Store),
    lib(Python, "pymysql", "MySQL", Store),
    lib(Python, "pymongo", "MongoDB", Store),
    lib(Python, "redis", "Redis", Store),
    lib(Python, "rq", "Redis", Store),
    lib(Python, "celery[redis]", "Redis", Store),
    lib(Ruby, "rails", "Rails", Web),
    lib(Ruby, "sinatra", "Sinatra", Web),
    lib(Ruby, "puma", "Puma", Server),
    lib(Ruby, "unicorn", "Unicorn", Server),
    lib(Ruby, "sidekiq", "Sidekiq", Worker),
    lib(Ruby, "good_job", "GoodJob", Worker),
    lib(Ruby, "solid_queue", "Solid Queue", Worker),
    lib(Ruby, "resque", "Resque", Worker),
    lib(Ruby, "delayed_job", "Delayed Job", Worker),
    lib(Ruby, "whenever", "whenever", Timer),
    lib(Ruby, "sidekiq-cron", "sidekiq-cron", Timer),
    lib(Ruby, "pg", "PostgreSQL", Store),
    lib(Ruby, "mysql2", "MySQL", Store),
    lib(Ruby, "mongoid", "MongoDB", Store),
    lib(Ruby, "redis", "Redis", Store),
    lib(Ruby, "sidekiq", "Redis", Store),
    lib(Ruby, "sqlite3", "SQLite", Store),
    lib(Go, "github.com/gin-gonic/gin", "Gin", Web),
    lib(Go, "github.com/labstack/echo", "Echo", Web),
    lib(Go, "github.com/gofiber/fiber", "Fiber", Web),
    lib(Go, "github.com/go-chi/chi", "chi", Web),
    lib(Go, "github.com/hibiken/asynq", "asynq", Worker),
    lib(Go, "github.com/riverqueue/river", "River", Worker),
    lib(Go, "github.com/robfig/cron", "cron", Timer),
    lib(Go, "github.com/lib/pq", "PostgreSQL", Store),
    lib(Go, "github.com/jackc/pgx", "PostgreSQL", Store),
    lib(Go, "github.com/go-sql-driver/mysql", "MySQL", Store),
    lib(Go, "go.mongodb.org/mongo-driver", "MongoDB", Store),
    lib(Go, "github.com/redis/go-redis", "Redis", Store),
    lib(Go, "github.com/go-redis/redis", "Redis", Store),
    lib(Go, "github.com/hibiken/asynq", "Redis", Store),
    lib(Go, "modernc.org/sqlite", "SQLite", Store),
    lib(Go, "github.com/mattn/go-sqlite3", "SQLite", Store),
    lib(Rust, "axum", "Axum", Web),
    lib(Rust, "actix-web", "Actix", Web),
    lib(Rust, "rocket", "Rocket", Web),
    lib(Rust, "apalis", "apalis", Worker),
    lib(Rust, "tokio-cron-scheduler", "tokio-cron-scheduler", Timer),
    lib(Rust, "tokio-postgres", "PostgreSQL", Store),
    lib(Rust, "postgres", "PostgreSQL", Store),
    lib(Rust, "mongodb", "MongoDB", Store),
    lib(Rust, "redis", "Redis", Store),
    lib(Rust, "rusqlite", "SQLite", Store),
    lib(Rust, "sqlx[postgres]", "PostgreSQL", Store),
    lib(Rust, "sqlx[mysql]", "MySQL", Store),
    lib(Rust, "sqlx[sqlite]", "SQLite", Store),
    lib(Rust, "diesel[postgres]", "PostgreSQL", Store),
    lib(Rust, "diesel[mysql]", "MySQL", Store),
    lib(Rust, "diesel[sqlite]", "SQLite", Store),
    lib(Rust, "sea-orm[sqlx-postgres]", "PostgreSQL", Store),
    lib(Rust, "sea-orm[sqlx-mysql]", "MySQL", Store),
    lib(Rust, "sea-orm[sqlx-sqlite]", "SQLite", Store),
    lib(Php, "laravel/framework", "Laravel", Web),
    lib(Php, "symfony/framework-bundle", "Symfony", Web),
    lib(Php, "laravel/horizon", "Horizon", Worker),
    lib(Php, "predis/predis", "Redis", Store),
    lib(Elixir, "phoenix", "Phoenix", Web),
    lib(Elixir, "oban", "Oban", Worker),
    lib(Elixir, "quantum", "Quantum", Timer),
    lib(Elixir, "postgrex", "PostgreSQL", Store),
    lib(Elixir, "myxql", "MySQL", Store),
    lib(Elixir, "redix", "Redis", Store),
    lib(Elixir, "ecto_sqlite3", "SQLite", Store),
];

impl Lib {
    pub fn matches(&self, d: &Dep) -> bool {
        self.eco == d.eco
            && match self.eco {
                Go => d.name == self.dep || d.name.starts_with(&format!("{}/", self.dep)),
                _ => d.name == self.dep,
            }
    }
}

/// Whether a process's framework is a chat bot library: one copy holding its platform's connection all the time.
pub fn is_bot(framework: &str) -> bool {
    LIBS.iter().any(|l| l.role == Bot && l.badge == framework)
}

/// The libraries of a role the dependencies list, in table order, each with the dependency that matched.
pub fn found(deps: &[Dep], role: Role) -> Vec<(&'static Lib, &Dep)> {
    LIBS.iter()
        .filter(|l| l.role == role)
        .filter_map(|l| deps.iter().find(|d| l.matches(d)).map(|d| (l, d)))
        .collect()
}

/// The first `parts` numeric parts of a version spec: `^4.19.2` -> `4`, `3.12.7` -> `3.12`, `v1.33.1` -> `1`.
pub fn version(spec: &str, parts: usize) -> Option<String> {
    let start = spec.find(|c: char| c.is_ascii_digit())?;
    let digits: Vec<&str> = spec[start..]
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()?
        .split('.')
        .filter(|s| !s.is_empty())
        .collect();
    // A 0.x major says nothing: `0.8.4` -> `0.8`.
    let parts = if digits.first() == Some(&"0") {
        parts.max(2)
    } else {
        parts
    };
    (!digits.is_empty()).then(|| digits.into_iter().take(parts).collect::<Vec<_>>().join("."))
}

/// Order of store badges.
const ENGINES: &[&str] = &["PostgreSQL", "MySQL", "MariaDB", "MongoDB", "SQLite", "Redis"];

/// Language and version, frameworks, servers, job runners, the stores in use, the platform's file.
pub fn badges(f: &Facts, platform: &Platform, files: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((_, lang, v)) = &f.language {
        out.push(with_version(lang, v.as_deref()));
    }
    for role in [Web, Server, Worker, Bot] {
        for (lib, dep) in found(&f.deps, role) {
            let badge = with_version(lib.badge, dep.version.as_deref());
            if !out
                .iter()
                .any(|b| *b == lib.badge || b.starts_with(&format!("{} ", lib.badge)))
            {
                out.push(badge);
            }
        }
    }
    if go_net_http(f, files) {
        out.push("net/http".into());
    }
    for engine in ENGINES {
        if let Some(d) = f.data.iter().find(|d| d.engine == *engine) {
            out.push(with_version(engine, d.version.as_deref()));
        }
    }
    if !platform.file.is_empty() {
        out.push(platform.file.clone());
    }
    out
}

/// A Go service with no web framework serves with the standard library.
fn go_net_http(f: &Facts, files: &BTreeMap<String, String>) -> bool {
    f.language.as_ref().is_some_and(|l| l.1 == "Go")
        && found(&f.deps, Web).is_empty()
        && files
            .iter()
            .any(|(p, t)| p.ends_with(".go") && t.contains("\"net/http\""))
}

fn with_version(name: &str, v: Option<&str>) -> String {
    match v {
        Some(v) => format!("{name} {v}"),
        None => name.to_string(),
    }
}

/// The code file a command runs, when it names one: `node src/worker.js` -> `src/worker.js`.
pub fn entry_file(command: &str, files: &BTreeMap<String, String>) -> Option<String> {
    command
        .split_whitespace()
        .map(|w| w.trim_start_matches("./"))
        .find(|w| files.contains_key(*w))
        .map(String::from)
}

/// Command words that name the runner whatever the dependencies say.
const COMMANDS: &[(&str, &str)] = &[
    ("sidekiq", "Sidekiq"),
    ("celery", "Celery"),
    ("rq worker", "RQ"),
    ("good_job", "GoodJob"),
    ("bin/jobs", "Solid Queue"),
    ("resque", "Resque"),
    ("artisan horizon", "Horizon"),
    ("artisan queue:work", "Laravel"),
    ("artisan schedule:", "Laravel"),
];

/// Each process's framework: the runner its command names, else the library its entry file imports first, else
/// the stack's web framework (web) or job runner (worker) or timer library (cron). A release step has none.
pub fn frameworks(processes: &mut [Process], deps: &[Dep], files: &BTreeMap<String, String>) {
    let web = found(deps, Web).first().map(|(l, _)| l.badge).or_else(|| {
        files
            .iter()
            .any(|(p, t)| p.ends_with(".go") && t.contains("\"net/http\""))
            .then_some("net/http")
    });
    for p in processes
        .iter_mut()
        .filter(|p| p.kind != "release" && !p.command.is_empty())
    {
        let by_command = COMMANDS.iter().find(|(w, _)| p.command.contains(w)).map(|(_, n)| *n);
        let by_import = entry_file(&p.command, files).and_then(|file| first_import(&files[&file], deps));
        let by_kind = match p.kind.as_str() {
            "web" => web,
            "worker" => found(deps, Worker)
                .first()
                .or(found(deps, Bot).first())
                .map(|(l, _)| l.badge),
            "cron" => found(deps, Timer).first().map(|(l, _)| l.badge),
            _ => None,
        };
        p.framework = by_command.or(by_import).or(by_kind).map(String::from);
    }
}

/// The framework, job runner or timer library a file imports first.
fn first_import(text: &str, deps: &[Dep]) -> Option<&'static str> {
    LIBS.iter()
        .filter(|l| l.role != Store && deps.iter().any(|d| l.matches(d)))
        .filter_map(|l| {
            let module = l.dep.replace('-', "_");
            let needles = [
                format!("'{}'", l.dep),
                format!("\"{}\"", l.dep),
                format!("import {module}"),
                format!("from {module} "),
                format!("from {module}."),
            ];
            needles
                .iter()
                .filter_map(|n| text.find(n.as_str()))
                .min()
                .map(|pos| (pos, l.badge))
        })
        .min_by_key(|(pos, _)| *pos)
        .map(|(_, badge)| badge)
}

/// Languages, whether Google's buildpacks build them, and the usual base image.
const LANGS: &[(&str, bool, &str)] = &[
    ("Node", true, "node:{24}-slim"),
    ("Python", true, "python:{3.13}-slim"),
    ("Ruby", true, "ruby:{3.4}-slim"),
    ("PHP", true, "php-fpm with nginx"),
    ("Go", true, "distroless, a static binary"),
    ("Java", true, "eclipse-temurin:{21}-jre"),
    ("Kotlin", true, "eclipse-temurin:{21}-jre"),
    (".NET", true, "dotnet/aspnet"),
    ("Rust", false, "debian slim or distroless"),
    ("Elixir", false, "a mix release on debian slim"),
];

const FRAMEWORKS: &[&str] = &[
    "Express",
    "Fastify",
    "NestJS",
    "Next.js",
    "Remix",
    "React Router",
    "Django",
    "Flask",
    "FastAPI",
    "Rails",
    "Laravel",
    "Spring Boot",
    "ASP.NET",
    "Phoenix",
    "Axum",
    "Actix",
    "Gin",
    "Echo",
    "Fiber",
    "chi",
    "net/http",
    "Bolt for Slack",
];

/// The container a service runs in on Cloud Run, from the stack's badges: the language and its version, the
/// framework, and how the image is built. Buildpacks build Node, Python, Ruby, PHP, Go, Java and .NET from source;
/// Rust and Elixir need a Dockerfile.
pub fn runtime_of(stack: &[String]) -> Runtime {
    let lang_of = |s: &str| LANGS.iter().find(|(l, ..)| s == *l || s.starts_with(&format!("{l} ")));
    let lang = stack.iter().find(|s| lang_of(s).is_some());
    let framework = stack
        .iter()
        .find(|s| FRAMEWORKS.iter().any(|f| s.starts_with(f)))
        .map(|s| {
            // "Express 4" -> "Express": drop a trailing version.
            match s.rsplit_once(' ') {
                Some((name, v)) if v.starts_with(|c: char| c.is_ascii_digit()) => name.to_string(),
                _ => s.clone(),
            }
        });
    let build = match lang.and_then(|l| lang_of(l).map(|x| (l, x))) {
        Some((l, (_, packs, base))) => {
            // `{24}` is the default version; the stack's own ("Node 20", from engines or .nvmrc) replaces it
            let base = match (base.split_once('{'), l.split_once(' ')) {
                (Some((head, rest)), own) => {
                    let (default, tail) = rest.split_once('}').unwrap_or((rest, ""));
                    format!("{head}{}{tail}", own.map_or(default, |(_, v)| v))
                }
                (None, _) => base.to_string(),
            };
            if *packs {
                format!("Buildpacks or your Dockerfile, on {base}")
            } else {
                format!("Your Dockerfile, on {base}")
            }
        }
        None => "Your Dockerfile".into(),
    };
    Runtime {
        name: lang.map_or_else(|| "Your container".into(), |l| format!("{l} container")),
        framework,
        build,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_that_runs_a_chat_bot_is_named_by_its_library() {
        let dep = |eco, name: &str| Dep {
            eco,
            name: name.into(),
            version: None,
            at: "manifest:1".into(),
        };
        let worker = |command: &str| Process {
            id: "bot".into(),
            kind: "worker".into(),
            name: "bot".into(),
            command: command.into(),
            from: "Procfile:2".into(),
            always_on: true,
            schedules: None,
            framework: None,
        };
        let files = BTreeMap::from([(
            "bot.js".to_string(),
            "const { Telegraf } = require('telegraf')".to_string(),
        )]);
        let mut ps = [worker("node bot.js"), worker("python bot.py")];
        frameworks(&mut ps[..1], &[dep(Node, "telegraf")], &files);
        frameworks(&mut ps[1..], &[dep(Python, "aiogram")], &files);
        assert_eq!(ps[0].framework.as_deref(), Some("Telegraf"));
        assert_eq!(ps[1].framework.as_deref(), Some("aiogram"));
        assert!(is_bot("Telegraf") && is_bot("aiogram") && !is_bot("BullMQ") && !is_bot("Express"));
        // a queue runner wins over a bot library when the stack has both
        let mut q = [worker("python bot.py")];
        frameworks(&mut q, &[dep(Python, "celery"), dep(Python, "aiogram")], &files);
        assert_eq!(q[0].framework.as_deref(), Some("Celery"));
    }

    #[test]
    fn versions() {
        assert_eq!(version("^4.19.2", 1).as_deref(), Some("4"));
        assert_eq!(version("~> 7.2.1", 1).as_deref(), Some("7"));
        assert_eq!(version("3.12.7", 2).as_deref(), Some("3.12"));
        assert_eq!(version("20.x", 1).as_deref(), Some("20"));
        assert_eq!(version(">=3.11", 2).as_deref(), Some("3.11"));
        assert_eq!(version("0.8.4", 1).as_deref(), Some("0.8"));
        assert_eq!(version("latest", 1), None);
    }

    #[test]
    fn runtime_as_the_cabinet_draws_it() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let node = runtime_of(&s(&["Node 20", "Express 4", "BullMQ 5", "PostgreSQL", "railway.json"]));
        assert_eq!(node.name, "Node 20 container");
        assert_eq!(node.framework.as_deref(), Some("Express"));
        assert_eq!(node.build, "Buildpacks or your Dockerfile, on node:20-slim");
        let py = runtime_of(&s(&["Python 3.11", "Django 5"]));
        assert_eq!(py.build, "Buildpacks or your Dockerfile, on python:3.11-slim");
        assert_eq!(
            runtime_of(&s(&["Node", "Express 4"])).build,
            "Buildpacks or your Dockerfile, on node:24-slim"
        );
        let go = runtime_of(&s(&["Go 1.23", "net/http", "SQLite", "fly.toml"]));
        assert_eq!(go.framework.as_deref(), Some("net/http"));
        assert_eq!(
            go.build,
            "Buildpacks or your Dockerfile, on distroless, a static binary"
        );
        let rust = runtime_of(&s(&["Rust", "Axum 0"]));
        assert_eq!(
            (rust.name.as_str(), rust.build.as_str()),
            ("Rust container", "Your Dockerfile, on debian slim or distroless")
        );
        assert_eq!(runtime_of(&[]).name, "Your container");
    }
}
