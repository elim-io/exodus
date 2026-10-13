# Corpus of real repositories

`examples/` holds four small apps made up for the audit. This folder holds what the audit meets in the wild: public
open-source repositories of the shapes people move to Cloud Run, each pinned at a commit, plus synthetic cases under
`synthetic/` for shapes no public repository shows cleanly. The repositories are not committed here: `./fetch` makes a
shallow clone of each at its commit in `~/.cache/exodus-corpus` (or `$EXODUS_CORPUS`), keeps clones already at their
commit, removes folders no longer listed and fails when the cache passes 2 GB.

`repos.txt` is the list: a name, the repository, the commit and the shape. Each name has a folder here with an
`expected.json`, written by reading the repository by hand before the scanner was changed for it, so it says what is
true about the code, not what the audit printed.

| name | repository | shape |
|---|---|---|
| `caddy-site` | caddyserver/website | static site behind Caddy |
| `static-generator` | rust-lang/blog.rust-lang.org | static site built by a generator |
| `nginx-spa` | excalidraw/excalidraw | browser app built by Vite, served by nginx |
| `nextjs-vercel` | nextjs/saas-starter | Next.js on Vercel with Postgres |
| `express-render` | render-examples/express-hello-world | Express on Render |
| `django-celery` | HackSoftware/Django-Styleguide-Example | Django with a Celery worker and beat |
| `rails-sidekiq` | maybe-finance/maybe | Rails with Sidekiq |
| `go-fly` | cubny/lite-reader | Go service with SQLite on Fly.io |
| `node-fly` | nas5w/govsky | TypeScript API and bots on Fly.io |
| `monorepo` | hoppscotch/hoppscotch | monorepo: backend, web apps, desktop app, CLI |
| `firebase` | jspsych/datapipe | Firebase Hosting and Functions (2nd gen) |
| `ios-firebase` | ertembiyik/Haptics | iOS app beside Firebase Functions |
| `mobile-backend` | immich-app/immich | mobile app beside a server, a web app and an ML service |
| `laravel` | koel/koel | Laravel with queues |
| `fastapi-tasks` | mealie-recipes/mealie | FastAPI with background and scheduled tasks |
| `phoenix` | plausible/analytics | Elixir/Phoenix with Oban |
| `axum` | launchbadge/realworld-axum-sqlx | Rust Axum with Postgres |
| `compose` | healthchecks/healthchecks | Django with docker-compose and a long-running sender |
| `synthetic/s2-shape` | (in this folder) | static site in a subfolder behind Caddy with browser timers, a build script, a tools script, Firebase Functions, an iOS app |

## expected.json (`corpus/1`)

The examples' `expected.json` fixes every line; a real repository is too large for that and its lines are not ours to
pin, so this format names files and kinds, and a test compares the audit with it on what matters to a person: no
finding that is false, no finding that is missing, the parts and the platform recognised.

```json
{
  "exodus": "corpus/1",
  "name": "caddy-site",
  "shape": "static site behind Caddy",
  "platform": { "name": "Unknown", "file": null, "ask": true, "why": "nothing in the repository names a host" },
  "processes": [
    { "kind": "web", "what": "static site", "from": "Dockerfile", "note": "Caddy serves src/ as files" }
  ],
  "stays": [],
  "data": [],
  "breaks": [],
  "idle": [],
  "notBreaks": [
    { "rule": "in-process-timer", "file": "src/resources/js/common.js", "why": "browser code, a setTimeout in the page" }
  ],
  "notOnServer": ["src/resources/js/"],
  "questions": ["platform"],
  "notes": "free text: what a reader needs to check the file"
}
```

- `platform.name` is the host the repository shows (`Railway`, `Render`, `Fly.io`, `Heroku`, `Vercel`, `Firebase`,
  `Netlify`, `GitHub Pages`) or `Unknown`; `file` is the file that says so; `ask` is true when the audit cannot know
  and must ask the `platform` question.
- `processes` are what runs on a server after the move: `kind` is `web`, `worker`, `cron` or `release`, `what` names it
  ("static site", "Celery worker", "Next.js"), `from` is the file that declares it (a Procfile, a compose file, a
  Dockerfile, a package.json script) or, with nothing declaring it, the entry file.
- `stays` are parts that are already serverless on Google and stay as they are (Firebase Functions 2nd gen run on Cloud
  Run functions; Firebase Hosting serves files), each `{ "what", "from", "note" }`.
- `data` are the stores: `kind` is `db`, `cache`, `files` or `sqlite`, with the `engine` and the file that shows it.
- `breaks` are the patterns that really break on Cloud Run in code that runs on the server, by rule id (the rule ids
  of `crates/exodus-audit/rules`) and file, with a `note`; a test fails when one is missing. A real problem is listed
  even when no rule's regex reaches it today (an Oban queue, sidekiq-cron in YAML, SQLite through an ORM): the
  corpus measures the scanner, it does not follow it. `release-migrations` is a rule of kind `note`, so a release step
  is a `release` process, never a break; migrations run at start-up are neither. `idle` is the same for the rules of kind
  `idle` (works, bills while nothing happens).
- `notBreaks` are traps: places that look like a rule's pattern but are not a problem on the server (browser code,
  build scripts, tools, mobile sources, tests, a timer the platform replaces). Any "breaks" finding not listed in
  `breaks` is a false finding; `notBreaks` names the ones a scanner is likely to raise, with the reason.
- `notOnServer` are the paths of code that never runs on the server (browser assets, build and tool scripts, mobile
  apps); the audit should name them in `clean` as "not on the server".
- `questions` lists question ids the audit must ask beyond the ones it always asks (`platform` when the host is
  unknown).

## Running it

```sh
tests/corpus/fetch
cargo test -p exodus-audit -- --ignored corpus --nocapture
```

`crates/exodus-audit/tests/corpus.rs` audits every case and prints a table and the differences from `expected.json`:
platform and the `platform` question, processes by kind, data by store (Firebase's stores count when named as
staying), the parts that stay, missed and false breaks and idle findings, and the paths named "not on the server".
`EXODUS_CORPUS_VERBOSE=1` also prints the processes the audit found. It is ignored by default (CI runs it in the
`corpus` job), lists every place a rule fires on (a report lists five per rule; the cap is not what is measured), and
fails on a missing clone, on a difference that `known` does not explain, and on a `known` line that no longer occurs;
`EXODUS_CORPUS_STRICT=1` fails on any difference, explained or not. `BASELINE.md` is its output over the tasks.

## known

A difference the scanner cannot remove stays in the table, and `expected.json` explains it in `known`: the line the test
prints as the key, why as the value.

```json
"known": {
  "FALSE break: in-process-timer src/lease.ts": "a heartbeat that lives for one request; the rule cannot read when the timer is cleared",
  "process missing: 1 x worker": "started by a shell script no reader opens"
}
```

Keys are the difference lines without the trap note (`MISSED break: <rule> <file>`, `FALSE break: ...`, `FALSE idle: ...`,
`process missing: N x <kind>`, `data missing: <kind> <engine>`, `platform: ...`, `questions: platform ...`). A new
difference fails the test until it is fixed or written here with its reason; a fixed one fails it until its line is
removed, so the list cannot go stale.
