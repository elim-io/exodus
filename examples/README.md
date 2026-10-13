# Sample repositories

Four small applications that the audit is tested on (`exodus audit` in each, the result compared with `expected.json`).
Each one is shaped like a common move to Cloud Run and carries the patterns the scanner must find. They are inputs for
the audit, not working apps: dependencies are not installed or locked, and nothing here is meant to be built or run.

| folder | platform | what it is | patterns |
|---|---|---|---|
| `node-railway` | Railway (`railway.json`, `Procfile`) | Express API, a BullMQ worker on Redis, a node-cron clock, uploads to `uploads/`, sessions in memory, Postgres by `DATABASE_URL` | queue worker, timers, disk writes, memory sessions |
| `rails-heroku` | Heroku (`app.json`, `Procfile` web/worker/release) | Rails with Sidekiq, Active Storage on disk, Heroku Scheduler as an add-on | queue worker, disk writes, release migrations, Heroku Scheduler |
| `django-render` | Render (`render.yaml`) | Django under gunicorn, a Celery worker with beat, Postgres 16, Redis, exports written to a disk | queue worker, timers, disk writes, release migrations |
| `go-fly` | Fly.io (`fly.toml`) | net/http service, a goroutine with a ticker, SQLite on a volume | timers, SQLite |

## expected.json

Written by hand before the scanner existed. It is a `findings/1` document (the types are in `crates/exodus-schema`) without the fields
that depend on the machine or the run: `repo`, `what`, `traffic`, `stats` and `cli` are left out.
`crates/exodus-audit/tests/examples.rs` runs the audit on each folder and compares `platform`, `stack`, `manifests`,
`processes`, `data`, `settings`, `clean`, `patterns` and the question ids. The rules themselves are in
`crates/exodus-audit/rules/` (README there). The comparison rules:

- `platform`, `stack`, `manifests`, `processes`, `data`, `settings`, `clean` and every pattern's `id`, `rule`, `kind`,
  `at`, `code`, `part`, `kb` must be equal.
- A pattern's `title`, `why`, `fix` and `subject` are the rule file's text. The values here say what the text means;
  a rule may word it differently, so these four are compared as present and non-empty.
- `questions` lists ids only; `q`, `affects` and `hint` come from `questions.toml`.
- `at` and every `from` are `file:line`, paths relative to the example's root, lines from 1. `code` is that line with
  leading whitespace trimmed.
- Lists of located things (`processes`, `data`, `patterns`, `schedules`) are sorted by file (bytewise) and then line.
  `manifests` and `envNames` are sorted by name. `clean` follows the rule order below.
- No value of an environment variable appears anywhere: `.env.example`, `fly.toml [env]` and `app.json env` hold values
  (`change-me`, `/data/cards.db`, `enabled`), and the audit reads only their names.
  Files that hold values (`.env`, `.env.*` except the examples, `*.env`, `.envrc`, `.dev.vars`, `*.tfvars`, `*.pem`,
  `*.key`) are not opened. A command or a source line copied into the findings loses its leading `NAME=value`
  assignments, secret-named flags, `user:password@` in URLs and the string after a secret-named key (`redact.rs`).

## Conventions these files fix

- Rule ids and their `clean` labels, in order: `queue-worker` "workers waiting on a queue", `work-after-response` "work
  after the response", `in-process-timer` "in-process timers", `local-disk-write` "files written to disk",
  `memory-sessions` "sessions in memory", `redis-sessions` "sessions kept in Redis", `long-lived-connection` "long-lived connections", `release-migrations`
  "migrations in a release step", `background-thread` "background threads", `sqlite-database` "SQLite or files used as a
  database". Platform notes such as `heroku-scheduler` fire only on their platform and never appear in `clean`.
- A pattern's `id` is its rule id; one rule reports one location per file (the first), and a second file's hit gets the
  id `<rule>-2`. The three node-cron schedules are one finding and three `schedules`. A finding lists at most 5 places
  per rule.
- `background-thread` matches thread APIs (`threading.Thread`, `Thread.new`, `worker_threads`), not a `go` statement,
  which every Go server has. The goroutine with a ticker in `go-fly` is an `in-process-timer` at the ticker line.
- A data item's `from` is the manifest line that provisions it (an add-on, a `databases:` entry, a disk, a mount); with
  no such line, the code line that connects to it or writes it. Railway configures databases and volumes outside
  `railway.json`, so `node-railway`'s data points at code.
- The Heroku Scheduler's jobs live in the Heroku dashboard, so the audit records a `cron` process with an empty command
  and no schedules, plus the `heroku-scheduler` note; the agent reads the jobs from the dashboard or asks.
- `platform.file` is the file only that platform reads (`app.json`, not the `Procfile` Railway also reads) and is the
  last stack badge. `platform.sleep` points at the line that decides it (`sleepApplication`, the dyno size, the Render
  plan, `auto_stop_machines`).
- `secretsGuess` counts names with `SECRET`, `_KEY`, `TOKEN` or `PASSWORD`, and the connection URLs of a database or a
  cache (`DATABASE_URL`, `REDIS_URL`, `CELERY_BROKER_URL`).
- Questions: the always-asked ids, `connections` and `peak` when there is a web process, `jobs` with a worker, `schedules`
  when anything runs on a schedule, `cpu` when a timer lives inside the web process.

## Reports

`crates/exodus-report/tests/snapshots.rs` builds the offline report of each sample (`exodus report --offline`, no
answers, a fixed date) and compares `report.json` and `report.md` with `crates/exodus-report/tests/snapshots/<name>`.
Two more snapshots: `node-railway.2` (report 2 after the answers in `tests/inputs/node-railway.answers.json`, with
what changed since report 1) and `new-product` (a plan from scratch, `tests/inputs/new-product.*.json`). When a change
to the report is meant, `UPDATE_SNAPSHOTS=1 cargo test -p exodus-report` rewrites them; read the diff before the commit.
