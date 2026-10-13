//! The reference table: every pattern the guide "The patterns that break on Cloud Run" names, and every finding of
//! the cabinet's sample projects acme-api, helpdesk and ledger, as the smallest repository that holds it and the rule and line
//! that must catch it. A pattern with no row here has no rule.
//!
//! Not in the table, because code does not show them: a long poll (an ordinary request that waits), and a worker
//! known only from a Procfile line, which `processes` lists as a worker without a pattern.

use std::path::PathBuf;

struct Row {
    /// Where the pattern comes from: `guide N` is a section of the guide, the rest a prototype project.
    from: &'static str,
    what: &'static str,
    files: &'static [(&'static str, &'static str)],
    rule: &'static str,
    at: &'static str,
}

const fn row(
    from: &'static str,
    what: &'static str,
    files: &'static [(&'static str, &'static str)],
    rule: &'static str,
    at: &'static str,
) -> Row {
    Row {
        from,
        what,
        files,
        rule,
        at,
    }
}

const ROWS: &[Row] = &[
    // 1. A worker waiting on a queue
    row(
        "guide 1, acme-api",
        "BullMQ worker",
        &[(
            "src/worker.ts",
            "import { Worker } from 'bullmq';\nnew Worker('jobs', handle, { connection: redis });\n",
        )],
        "queue-worker",
        "src/worker.ts:2",
    ),
    row(
        "guide 1",
        "Sidekiq worker",
        &[(
            "app/workers/mail_worker.rb",
            "class MailWorker\n  include Sidekiq::Worker\nend\n",
        )],
        "queue-worker",
        "app/workers/mail_worker.rb:2",
    ),
    row(
        "guide 1, helpdesk",
        "Active Job on Sidekiq",
        &[(
            "config/environments/production.rb",
            "Rails.application.configure do\n  config.active_job.queue_adapter = :sidekiq\nend\n",
        )],
        "queue-worker",
        "config/environments/production.rb:2",
    ),
    row(
        "guide 1, ledger",
        "Celery app",
        &[(
            "books/celery.py",
            "from celery import Celery\napp = Celery('books', broker=BROKER)\n",
        )],
        "queue-worker",
        "books/celery.py:2",
    ),
    row(
        "guide 1",
        "RQ worker",
        &[(
            "worker.py",
            "from rq import Worker, Queue\nWorker([Queue(connection=conn)]).work()\n",
        )],
        "queue-worker",
        "worker.py:1",
    ),
    row(
        "guide 1",
        "RQ queue, the worker started by `rq worker`",
        &[(
            "tasks.py",
            "from redis import Redis\nfrom rq import Queue\nq = Queue(connection=Redis())\nq.enqueue(send_email, user)\n",
        )],
        "queue-worker",
        "tasks.py:2",
    ),
    row(
        "guide 1",
        "asynq server in Go",
        &[(
            "cmd/worker/main.go",
            "package main\nfunc main() {\n\tsrv := asynq.NewServer(opt, cfg)\n}\n",
        )],
        "queue-worker",
        "cmd/worker/main.go:3",
    ),
    // 2. Work after the response
    row(
        "guide 2",
        "sendEmail() after res.send()",
        &[(
            "src/routes.js",
            "app.post('/x', (req, res) => {\n  res.send('ok');\n  sendEmail(req.body);\n});\n",
        )],
        "work-after-response",
        "src/routes.js:3",
    ),
    row(
        "guide 2",
        "a promise nobody awaits",
        &[(
            "src/routes.js",
            "app.post('/x', async (req, res) => {\n  res.send('ok');\n  void sendEmail(req.body);\n});\n",
        )],
        "work-after-response",
        "src/routes.js:3",
    ),
    row(
        "guide 2",
        "an unawaited asyncio task",
        &[(
            "app/main.py",
            "@app.post('/x')\nasync def x():\n    asyncio.create_task(send_email())\n    return {}\n",
        )],
        "work-after-response",
        "app/main.py:3",
    ),
    row(
        "guide 2",
        "FastAPI background task",
        &[(
            "app/main.py",
            "def x(tasks: BackgroundTasks):\n    tasks.add_task(send_email)\n",
        )],
        "work-after-response",
        "app/main.py:2",
    ),
    // 3. Timers and cron inside the app
    row(
        "guide 3, acme-api",
        "node-cron in a clock process",
        &[(
            "src/clock.ts",
            "import cron from 'node-cron';\ncron.schedule('0 * * * *', cleanup);\n",
        )],
        "in-process-timer",
        "src/clock.ts:2",
    ),
    row(
        "guide 3",
        "setInterval",
        &[("src/server.js", "app.listen(3000);\nsetInterval(sweep, 60000);\n")],
        "in-process-timer",
        "src/server.js:2",
    ),
    row(
        "guide 3",
        "APScheduler",
        &[(
            "app/jobs.py",
            "from apscheduler.schedulers.background import BackgroundScheduler\nsched = BackgroundScheduler()\n",
        )],
        "in-process-timer",
        "app/jobs.py:2",
    ),
    row(
        "guide 3",
        "whenever",
        &[(
            "config/schedule.rb",
            "every 1.day, at: '4:30 am' do\n  runner 'Report.send'\nend\n",
        )],
        "in-process-timer",
        "config/schedule.rb:1",
    ),
    row(
        "guide 3, ledger",
        "Celery beat",
        &[(
            "books/celery.py",
            "app.conf.beat_schedule = {\n  'nightly': {\n    'task': 'exports.nightly',\n    'schedule': crontab(hour=2, minute=0),\n  },\n}\n",
        )],
        "in-process-timer",
        "books/celery.py:4",
    ),
    row(
        "guide 3",
        "a ticker in Go",
        &[("main.go", "func main() {\n\tt := time.NewTicker(time.Minute)\n}\n")],
        "in-process-timer",
        "main.go:2",
    ),
    // 4. Files on local disk
    row(
        "guide 4",
        "Multer dest",
        &[("src/upload.js", "const upload = multer({ dest: 'uploads/' });\n")],
        "local-disk-write",
        "src/upload.js:1",
    ),
    row(
        "guide 4, acme-api",
        "fs.writeFile into a data folder",
        &[(
            "src/routes/upload.ts",
            "export async function save(name, buf) {\n  await fs.writeFile(path.join(UPLOAD_DIR, name), buf);\n}\n",
        )],
        "local-disk-write",
        "src/routes/upload.ts:2",
    ),
    row(
        "guide 4, helpdesk",
        "Active Storage :local",
        &[(
            "config/environments/production.rb",
            "Rails.application.configure do\n  config.active_storage.service = :local\nend\n",
        )],
        "local-disk-write",
        "config/environments/production.rb:2",
    ),
    row(
        "guide 4, ledger",
        "a file written under MEDIA_ROOT",
        &[(
            "exports/tasks.py",
            "def nightly():\n    path = settings.MEDIA_ROOT / 'export.csv'\n    with open(path, 'w') as f:\n        f.write('x')\n",
        )],
        "local-disk-write",
        "exports/tasks.py:3",
    ),
    row(
        "guide 4",
        "Django's FileSystemStorage",
        &[(
            "app/storage.py",
            "from django.core.files.storage import FileSystemStorage\nfs = FileSystemStorage(location='/data')\n",
        )],
        "local-disk-write",
        "app/storage.py:1",
    ),
    row(
        "guide 4",
        "a file written in Go",
        &[(
            "main.go",
            "func save(b []byte) error {\n\treturn os.WriteFile(\"data/x\", b, 0o644)\n}\n",
        )],
        "local-disk-write",
        "main.go:2",
    ),
    row(
        "guide 4",
        "SQLite in Node",
        &[(
            "src/db.js",
            "const Database = require('better-sqlite3');\nconst db = new Database('app.db');\n",
        )],
        "sqlite-database",
        "src/db.js:1",
    ),
    row(
        "guide 4",
        "SQLite in Python",
        &[("app/db.py", "import sqlite3\nconn = sqlite3.connect('app.db')\n")],
        "sqlite-database",
        "app/db.py:2",
    ),
    row(
        "guide 4",
        "SQLite in Go",
        &[("main.go", "db, err := sql.Open(\"sqlite\", path)\n")],
        "sqlite-database",
        "main.go:1",
    ),
    // 5. Sessions and caches in memory
    row(
        "guide 5, acme-api",
        "express-session with its MemoryStore",
        &[(
            "src/server.js",
            "app.use(session({ secret: process.env.S, resave: false }));\n",
        )],
        "memory-sessions",
        "src/server.js:1",
    ),
    row(
        "guide 5",
        "Django sessions in the local cache",
        &[(
            "app/settings.py",
            "SESSION_ENGINE = 'django.contrib.sessions.backends.cache'\n",
        )],
        "memory-sessions",
        "app/settings.py:1",
    ),
    row(
        "guide 5",
        "Flask-Session on the filesystem",
        &[("app/__init__.py", "SESSION_TYPE = 'filesystem'\n")],
        "memory-sessions",
        "app/__init__.py:1",
    ),
    row(
        "guide 5",
        "Rails sessions in a memory store",
        &[(
            "config/initializers/session_store.rb",
            "Rails.application.config.session_store :memory_store\n",
        )],
        "memory-sessions",
        "config/initializers/session_store.rb:1",
    ),
    row(
        "guide 5",
        "an in-process map of logged-in users",
        &[("src/auth.js", "const loggedInUsers = new Map();\n")],
        "memory-sessions",
        "src/auth.js:1",
    ),
    row(
        "guide 5",
        "an in-process dict of sessions",
        &[("app/auth.py", "SESSIONS = {}\n")],
        "memory-sessions",
        "app/auth.py:1",
    ),
    row(
        "guide 5, acme-api",
        "connect-redis, Redis for sessions",
        &[(
            "src/app.js",
            "const RedisStore = require('connect-redis')(session);\napp.use(session({ store: new RedisStore({ client }) }));\n",
        )],
        "redis-sessions",
        "src/app.js:1",
    ),
    row(
        "guide 5",
        "Rails Redis session store",
        &[(
            "config/initializers/session_store.rb",
            "Rails.application.config.session_store :redis_store, servers: ENV['REDIS_URL']\n",
        )],
        "redis-sessions",
        "config/initializers/session_store.rb:1",
    ),
    row(
        "guide 5",
        "Flask-Session on Redis",
        &[("app/__init__.py", "SESSION_TYPE = 'redis'\n")],
        "redis-sessions",
        "app/__init__.py:1",
    ),
    // 6. Long-lived connections
    row(
        "guide 6",
        "WebSockets with ws",
        &[("src/ws.js", "const { WebSocketServer } = require('ws');\n")],
        "long-lived-connection",
        "src/ws.js:1",
    ),
    row(
        "guide 6",
        "socket.io",
        &[("src/io.ts", "import { Server } from 'socket.io';\n")],
        "long-lived-connection",
        "src/io.ts:1",
    ),
    row(
        "guide 6",
        "server-sent events",
        &[(
            "src/sse.js",
            "res.writeHead(200, { 'Content-Type': 'text/event-stream' });\n",
        )],
        "long-lived-connection",
        "src/sse.js:1",
    ),
    row(
        "guide 6, helpdesk",
        "Action Cable channel",
        &[(
            "app/channels/ticket_channel.rb",
            "class TicketChannel < ApplicationCable::Channel\nend\n",
        )],
        "long-lived-connection",
        "app/channels/ticket_channel.rb:1",
    ),
    row(
        "guide 6",
        "Django Channels consumer",
        &[("app/consumers.py", "class Chat(AsyncWebsocketConsumer):\n    pass\n")],
        "long-lived-connection",
        "app/consumers.py:1",
    ),
    row(
        "guide 6",
        "WebSockets in Go",
        &[("main.go", "var up = websocket.Upgrader{}\n")],
        "long-lived-connection",
        "main.go:1",
    ),
    // Beyond the six: what a move carries over
    row(
        "helpdesk",
        "migrations in the Procfile release step",
        &[("Procfile", "web: bin/rails s\nrelease: bin/rails db:migrate\n")],
        "release-migrations",
        "Procfile:2",
    ),
    row(
        "helpdesk",
        "Heroku Scheduler",
        &[
            ("Procfile", "web: bin/rails s\n"),
            ("app.json", "{\n  \"addons\": [\"scheduler:standard\"]\n}\n"),
        ],
        "heroku-scheduler",
        "app.json:2",
    ),
    row(
        "ledger",
        "migrations in Render's preDeployCommand",
        &[(
            "render.yaml",
            "services:\n  - type: web\n    name: web\n    runtime: python\n    preDeployCommand: python manage.py migrate\n",
        )],
        "release-migrations",
        "render.yaml:5",
    ),
    row(
        "guide 1-3",
        "a thread started in the app",
        &[("app/mail.py", "t = threading.Thread(target=send)\n")],
        "background-thread",
        "app/mail.py:1",
    ),
    row(
        "guide 1-3",
        "a thread started in Ruby",
        &[("app/models/user.rb", "Thread.new { notify }\n")],
        "background-thread",
        "app/models/user.rb:1",
    ),
    row(
        "guide 1-3",
        "a scanner goroutine started with a Go server",
        &[(
            "internal/manager/manager.go",
            "func (m *Manager) Run() {\n\tgo m.scanCampaigns(time.Second * 5)\n}\n",
        )],
        "background-thread",
        "internal/manager/manager.go:2",
    ),
    row(
        "guide 1-3",
        "a Rust worker blocking on a Redis list",
        &[(
            "src/bin/worker.rs",
            "async fn main() {\n    loop {\n        let job: Option<(String, String)> = redis.brpop(\"jobs\", 5.0).await.unwrap();\n    }\n}\n",
        )],
        "queue-worker",
        "src/bin/worker.rs:3",
    ),
    row(
        "guide 1-3",
        "Rust sessions in Redis",
        &[(
            "src/app.rs",
            "fn app(pool: Pool) -> Router {\n    Router::new().layer(SessionManagerLayer::new(RedisStore::new(pool)))\n}\n",
        )],
        "redis-sessions",
        "src/app.rs:2",
    ),
    row(
        "guide 4",
        "a file written in Rust",
        &[(
            "src/save.rs",
            "pub fn save(b: &[u8]) {\n    std::fs::write(\"data/x\", b).ok();\n}\n",
        )],
        "local-disk-write",
        "src/save.rs:2",
    ),
    row(
        "guide 4",
        "an upload moved in PHP",
        &[(
            "public/up.php",
            "<?php\nmove_uploaded_file($_FILES['f']['tmp_name'], 'uploads/x');\n",
        )],
        "local-disk-write",
        "public/up.php:2",
    ),
    row(
        "guide 4",
        "a file written in Elixir",
        &[(
            "lib/app/save.ex",
            "defmodule Save do\n  def run(b), do: File.write!(\"data/x\", b)\nend\n",
        )],
        "local-disk-write",
        "lib/app/save.ex:2",
    ),
];

fn root_of(n: usize) -> PathBuf {
    std::env::temp_dir().join(format!("exodus-reference-{}-{n}", std::process::id()))
}

#[test]
fn every_pattern_of_the_guide_and_the_prototype_is_caught() {
    let mut missed = Vec::new();
    for (n, r) in ROWS.iter().enumerate() {
        let root = root_of(n);
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in r.files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let found = exodus_audit::audit(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        if !found.patterns.iter().any(|p| p.rule == r.rule && p.at == r.at) {
            let got: Vec<_> = found.patterns.iter().map(|p| format!("{} {}", p.rule, p.at)).collect();
            missed.push(format!(
                "[{}] {}: want {} at {}, got {got:?}",
                r.from, r.what, r.rule, r.at
            ));
        }
    }
    assert!(missed.is_empty(), "{} not caught:\n{}", missed.len(), missed.join("\n"));
}

/// A rule that fires on everything is as useless as one that fires on nothing: each rule of the table has a
/// near miss in the unit tests of `src/rules.rs`, and this one checks the table's own ids exist.
#[test]
fn every_row_names_a_rule_that_exists() {
    let ids: Vec<String> = exodus_audit::rules::builtin()
        .iter()
        .map(|r| r.file.id.clone())
        .collect();
    for r in ROWS {
        assert!(ids.contains(&r.rule.to_string()), "{}: no rule {}", r.what, r.rule);
    }
}

/// The data item and the pattern come from one hit: a write to a temporary file makes neither.
#[test]
fn a_temporary_file_is_neither_a_pattern_nor_a_data_item() {
    let root = root_of(usize::MAX);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("main.go"),
        "func f(b []byte) { os.WriteFile(\"/tmp/x\", b, 0o600) }\n",
    )
    .unwrap();
    let found = exodus_audit::audit(&root).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(found.patterns.is_empty() && found.data.is_empty(), "{found:?}");
}

/// A Procfile process is kinded by its command too: a Celery beat under another name is a scheduler, not a worker.
#[test]
fn a_celery_beat_under_any_name_is_a_scheduler() {
    let root = root_of(usize::MAX - 1);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Procfile"),
        "web: gunicorn app.wsgi\nworker: celery -A app worker -l INFO\nworker-beat: celery -A app beat -l INFO\n",
    )
    .unwrap();
    let found = exodus_audit::audit(&root).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    let kind = |id: &str| found.processes.iter().find(|p| p.id == id).map(|p| p.kind.as_str());
    assert_eq!(kind("worker"), Some("worker"));
    assert_eq!(kind("worker-beat"), Some("cron"), "{:?}", found.processes);
}
