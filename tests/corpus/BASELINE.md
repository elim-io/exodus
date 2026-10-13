# Corpus baseline

The audit run over the 20 pinned repositories and `synthetic/s2-shape`, compared with each `expected.json`. The
section "After the check" is the current state. The sections from "Totals" on are the state of `exodus` v0.1.3 (commit
`a08a5d9` of the corpus, scanner unchanged), the *before* of the tasks since (browser and build code, project shapes,
platform question, the check); the states in between are in git.

Reproduce (about 30 s in a debug build; CI runs it as the `corpus` job):

```sh
tests/corpus/fetch
cargo test -p exodus-audit -- --ignored corpus --nocapture
EXODUS_CORPUS_STRICT=1 cargo test -p exodus-audit -- --ignored corpus   # fails on any difference, explained or not
```

The test is `crates/exodus-audit/tests/corpus.rs`. It audits with the library (`exodus audit` on a clone writes
`.exodus/` into it, so the test does not run the binary; the findings are the same) and lists every place a rule fires
on, not the five a report lists per rule. A *break* here is a distinct (rule, file) pair, not a line: the same rule
twice in one file counts once. A false break is a `breaks` finding with no entry in `expected.json`'s `breaks`; it is
marked *trap* when `notBreaks` names it or its file lies under a `notOnServer` path. Data is compared by store (kind and
the first letters of the engine); processes by kind. A difference passes only when `expected.json`'s `known` explains
it (README, "known").

## v0.2.6: Rust rules, test harnesses, a Dockerfile behind a platform file

Two cases joined: `rust-railway` (a Rust Rocket server) and `go-compose` (a Go server with in-process workers). Both
match their `expected.json` with no difference. What changed in the scanner:

- A compose file in a folder beside a platform file, or in a test harness folder (`playwright/`, `cypress/`), is never
  the production process list. Code under those folders no longer adds env names or names the database's env var.
- Railway with `build.dockerfilePath` and no `startCommand` runs that Dockerfile's command; its `healthcheckPath` and
  `requiredMountPath` become the health route and a files volume. A symlinked file (`Dockerfile -> docker/...`) is read.
- Rust has rules for timers (a tokio interval inside a WebSocket is a heartbeat, not a schedule), background loops,
  long-lived connections, SQLite and in-memory sign-in state; Go has the last one too.
- An ORM built for several engines runs on one database; the SQLite finding is dropped when a server database is found.
  `compose`'s false SQLite break (a read-only search index beside Postgres) went with it.
- MariaDB is MariaDB, moving to Cloud SQL for MySQL.

Totals now: processes missing 8, data missing 8 / extra 1, breaks missed 10, false breaks 11 (all traps), idle false 1.

## After the check (fix-check 1/3)

SPEC F7 says the test fails on any false break or missing expected finding. The scanner could not meet that with
placement alone (A1), so this task took the rest: rules, the writes' destination, and an honest list of what stays.

What changed:

- **The cap was hiding things.** A report lists five places per rule, and `exodus check` used the same cap although
  its comments said it saw all. Both the corpus test and `exodus check` now see every place (`audit_with`'s `limit`).
  This surfaced real findings the labels had not listed (4 breaks and 2 idle added to `expected.json`, each read in the
  code: mealie's export, migration and asset uploads, koel's scan log, two Turbo Streams pages of the Rails app) and
  three false ones (traps added).
- **A write is judged by where it goes** (`(?P<dest>...)` in a rule's regex, `unless` read on that name and on the
  assignment of it within ten lines above). The scratch path no longer hides a persistent copy beside it (a temporary
  source copied to a data folder is still a break), and a temporary destination named on another line is no longer one:
  4 of the 15 false breaks A1 listed went this way (`aio_run.mjs`, `RemoteDiskStorage.php`, `admin_debug.py`,
  `controller_migrations.py`), and so did two the cap had hidden (`template_service.py`, `openai.py`).
- **`unless_in`**: a hit inside `def lifespan`, `startup` or `start` is the app starting, not work after a response
  (2 of the 15: `scheduler_service.py`, `main.py`). 9 of the 15 stay, and 3 more false ones joined them once the cap
  was lifted and the new rules ran (`MacroProvider.php`, `rate_limit.ex`, `internal_stats_api_version.ex`): 12.
- **New rule files and patterns** (data, `rules/`): Elixir (Oban queues and cron, `Process.send_after`,
  `:timer.send_interval`, cache warmers' intervals, LiveView sockets), PHP (`File::put/move/copy`, an uploaded file's
  `->move(`), Python (`Path.open('wb')`, `.write_text`, `sqlite:///name`), Node (`@Cron`, a typed `CronJob<..>`,
  GraphQL subscriptions), Ruby (sidekiq-cron YAML, `turbo_stream_from`, the storage service chosen with a default),
  Rust (sqlx, diesel and sea-orm features name the engine). Laravel's `Console/Commands` are tool code.
- A file the code creates where it opens the SQLite database is that database, not a second `files` store.

| | after F4 | after the check |
|---|---|---|
| breaks expected / raised / right | 48 / 37 / 22 | 52 / 54 / 42 |
| precision, recall of breaks | 59 %, 46 % | 78 %, 81 % |
| false breaks | 15 | 12 (all in `known`) |
| breaks missed | 26 | 10 (all in `known`) |
| idle missed / false | 3 / 1 | 0 / 1 (in `known`) |
| processes missing / extra | 8 / 0 | 8 / 0 (5 in `known`; one line counts several) |
| data stores missing / extra | 10 / 2 | 8 / 1 |
| cases equal to expected.json | 7 of 19 | 9 of 19 |

The test now fails on a difference that `known` does not explain and on a `known` line that stopped occurring. There
are 37 lines, each with its reason in `expected.json`. By cause:

| cause | lines | cases |
|---|---|---|
| a timer that keeps a lease, a health check or a cache fresh (the rule sees `setInterval` or `send_after`; what the callback does is no pattern) | 6 | mobile-backend 3, firebase 1, phoenix 2 |
| a write into a cache or scratch folder no line marks as temporary | 2 | mobile-backend 2 (model cache) |
| a write behind a development-only flag | 2 | mobile-backend `misc.ts`, monorepo `gql-schema.ts` |
| a test-only macro, a read-only SQLite index | 2 | laravel 1, compose 1 |
| the write goes through the project's own wrapper (a repository class, `Plausible.File.mv!`) | 4 | mobile-backend 2, phoenix 2 |
| the storage, database or search engine is chosen in a settings module or config key (ClickHouse's driver is in no table either) | 2 breaks, 8 stores | django-celery, fastapi-tasks, laravel, compose, phoenix |
| a polling loop in a management command run by a process manager | 2 | compose |
| sessions in a cache behind the project's own adapter | 1 | phoenix |
| a secret written to a file named `tmp_file` that the temporary-path check reads as scratch | 1 | fastapi-tasks |
| a stream that ends with its request (`EventSourceResponse`), kept as the safe side | 1 idle | fastapi-tasks |
| a process started by a process manager, a script or a compose file no reader opens | 5 | compose 3 (uwsgi `attach-daemon`), node-fly 2 (shell loops), laravel 1 (scheduler in `routes/console.php`), mobile-backend 1 (a worker thread), rails-sidekiq 1 |
| a dependency with the store's name the audit does not use by default | 1 | laravel (`predis` in `composer.json`) |

(`known` writes one line per process kind of a case, so those 8 missing processes are 5 lines.)

Why these stay: each is a decision the code makes at run time (what a timer's callback does, whether a flag is set, what a
settings module picks) or a place no reader opens. A rule would have to be narrowed to the project to remove it, and a
rule fitted to its test case finds nothing in the next repository. The cost is stated where the person reads the report:
a finding asks to check, and the person decides.

The "traps off the server named in `clean`" figure is unchanged (52 of 61, not part of the gate): 11 sit in folders the
audit never opens (`vendor/`), Swift files no rule reads, a Rust test module and a browser file with no hit.

## Totals

| | expected | found right | missed | false / extra |
|---|---|---|---|---|
| platform named (host other than Unknown) | 9 | 4 | 5 (found Unknown) | 0 |
| `platform` question (cases that must ask) | 10 | 0 | 10 (no such question in the bank) | 0 |
| processes (by kind) | 35 | 12 | 23 | 2 |
| data stores | 35 | 18 | 17 | 5 |
| breaks | 46 | 16 | 30 | 47 false |
| idle | 8 | 5 | 3 | 1 false |
| parts that stay (Firebase Functions, Hosting) | 5 | 0 | 5 (no field in findings) | 0 |
| paths named "not on the server" in `clean` | 110 | 0 | 110 | 0 |

Processes missing 23 / extra 2, data missing 17 / extra 5, breaks missed 30, false breaks 47 (trap 47 / other 0), idle missed 3 / false 1, not on the server named 0 of 110; 18 case(s) differ

Of the 63 `breaks` findings the audit raises, 16 are real: 25 % precision. 47 of 47 false ones are traps listed in the
`expected.json` files (a false one that nobody listed: none, once `sessions/ann/loader.py` of mobile-backend, the one the
labelling had missed, was added as a trap). The two `data extra` that are not obvious: `laravel` finds Redis from
Laravel's stock `config/database.php`, which koel carries but does not use by default (expected.json does not list it);
the `files` extra in `go-fly`, `nextjs-vercel`, `static-generator` and `s2-shape` is a build or setup script's write.

## Per case

| case | platform | platform asked | processes missing / extra | data missing / extra | breaks missed | false breaks (trap / other) | idle missed / false | not on the server named |
|---|---|---|---|---|---|---|---|---|
| axum | ok | not asked | 1 / 0 | 1 / 0 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 1 |
| caddy-site | ok | not asked | 1 / 0 | 0 / 0 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 2 |
| compose | ok | not asked | 4 / 0 | 0 / 0 | 2 | 3 (3 / 0) | 0 / 0 | 0 of 9 |
| django-celery | ok | ok | 0 / 0 | 1 / 0 | 1 | 1 (1 / 0) | 0 / 0 | 0 of 2 |
| express-render | ok | ok | 0 / 0 | 0 / 0 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 0 |
| fastapi-tasks | ok | not asked | 1 / 0 | 2 / 0 | 6 | 6 (6 / 0) | 0 / 1 | 0 of 6 |
| firebase | Unknown, expected Firebase | ok | 0 / 1 | 3 / 0 | 0 | 1 (1 / 0) | 0 / 0 | 0 of 11 |
| go-fly | ok | ok | 0 / 0 | 0 / 1 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 5 |
| ios-firebase | Unknown, expected Firebase | ok | 0 / 0 | 2 / 0 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 9 |
| laravel | ok | not asked | 2 / 0 | 2 / 1 | 4 | 1 (1 / 0) | 0 / 0 | 0 of 7 |
| mobile-backend | ok | not asked | 3 / 0 | 2 / 0 | 4 | 11 (11 / 0) | 0 / 0 | 0 of 18 |
| monorepo | ok | not asked | 5 / 1 | 0 / 0 | 2 | 9 (9 / 0) | 1 / 0 | 0 of 11 |
| nextjs-vercel | Unknown, expected Vercel | ok | 0 / 0 | 0 / 1 | 0 | 1 (1 / 0) | 0 / 0 | 0 of 3 |
| nginx-spa | Unknown, expected Vercel | ok | 1 / 0 | 1 / 0 | 0 | 3 (3 / 0) | 0 / 0 | 0 of 6 |
| node-fly | ok | ok | 2 / 0 | 0 / 0 | 0 | 0 (0 / 0) | 0 / 0 | 0 of 3 |
| phoenix | ok | not asked | 0 / 0 | 1 / 0 | 9 | 7 (7 / 0) | 1 / 0 | 0 of 5 |
| rails-sidekiq | ok | not asked | 1 / 0 | 1 / 0 | 2 | 0 (0 / 0) | 1 / 0 | 0 of 5 |
| static-generator | Unknown, expected GitHub Pages | ok | 1 / 0 | 0 / 1 | 0 | 2 (2 / 0) | 0 / 0 | 0 of 3 |
| s2-shape | ok | not asked | 1 / 0 | 1 / 1 | 0 | 2 (2 / 0) | 0 / 0 | 0 of 4 |

## Where the false breaks come from (47)

| cause | findings | cases |
|---|---|---|
| build, code generation, setup and dev tools (`dev/`, `scripts`-like folders, `lib/mix/tasks/`, generators, `aio_run.mjs`) | 23 | compose 2, django-celery 1, fastapi-tasks 5, monorepo 5, nextjs-vercel 1, nginx-spa 1, phoenix 5, static-generator 2, s2-shape 1 |
| browser code (timers in pages: `web/src`, `packages/*-common`, `tracker/`, `website/start`) | 11 | mobile-backend 2, monorepo 4, nginx-spa 2, phoenix 2, s2-shape 1 |
| server code the rule misreads (an availability check, an idle shutdown loop, a cache refresh, a model-cache file, a scheduler seen as work-after-response, a temp file) | 10 | compose 1, fastapi-tasks 1, firebase 1, laravel 1, mobile-backend 6 |
| tests and e2e | 3 | mobile-backend 3 |

By rule: `local-disk-write` 28, `in-process-timer` 15, `sqlite-database` 2, `work-after-response` 2. The first two
causes (34 of 47) are SPEC F3's: code that does not run on the server. The third (10) is not about where the code runs;
it needs rule or context changes, or stays as noise the report must word as "check".

## Missed findings (30 breaks, 3 idle)

By rule: `local-disk-write` 17, `in-process-timer` 8, `queue-worker` 2, `sqlite-database` 2, `memory-sessions` 1; idle:
`long-lived-connection` 3.

- phoenix 9 (Elixir: Oban queue and cron in `config/runtime.exs`, `Process.send_after` and `:timer` loops, `File.write`
  in exports and CSV import, an ETS cache), fastapi-tasks 6 (writes through `Path.write_*`/shutil on uploads and
  backups, SQLite chosen by a SQLAlchemy setting), laravel 4 (`File::`/`Storage::disk('local')` writes, Scout's
  SQLite), mobile-backend 4 (NestJS `@Cron` through a repository, `fs` writes in interceptors), compose 2 (the
  `sendalerts` and `sendreports` loops of a management command), monorepo 2, rails-sidekiq 2 (`sidekiq-cron` in
  `config/schedule.yml`, `config/storage.yml` local disk), django-celery 1 (the `local` upload default in a settings
  module).
- Rules that do not reach the stack's idiom at all: Elixir (Oban, `Process.send_after`), PHP facades, YAML schedules,
  settings-driven storage defaults, ORM-chosen SQLite. These need new rule patterns or manifest readers, not path
  filtering; they are outside F3 and F4 as written. The SPEC owner decides whether a later task takes them.

## Unrecognised

- **Platform.** Unknown where a host is named: firebase, ios-firebase (Firebase), nextjs-vercel, nginx-spa (Vercel
  from README/config), static-generator (GitHub Pages). The three weak-evidence cases (nextjs-vercel, nginx-spa,
  static-generator) may be relaxed to "Unknown, ask" by F4. None of the 10 cases that must ask gets the `platform`
  question.
- **Processes.** 23 missing: 14 `web` (axum, caddy-site, compose, fastapi-tasks, laravel, mobile-backend 2, monorepo 4,
  nginx-spa, static-generator, s2-shape: no Procfile, platform file or recognised framework manifest, so no web process,
  and none for a static site), 5 `worker` (compose 3, mobile-backend 1, rails-sidekiq 1), 3 `cron` (node-fly 2, laravel 1)
  and 1 `release` (monorepo). Extra 2: a Next.js `web` in `firebase` (the app is files and Functions, not a server) and
  a `hoppscotch-aio` worker in `monorepo`.
- **Data.** Missing: Firestore/Realtime DB/Cloud Storage (firebase 3, ios-firebase 2, nginx-spa 1, s2-shape 1), Postgres
  (axum, fastapi-tasks optional, mobile-backend), MySQL (laravel), ClickHouse (phoenix), RabbitMQ and Valkey as
  broker, SQLite (fastapi-tasks, laravel), local-disk `files` (rails-sidekiq).
- **Not on the server.** 0 of 110 paths: the audit has no such notion; `clean` lists only rules without a hit.

## Details

```text
axum
  process missing: 1 x web
  data missing: db postg

caddy-site
  process missing: 1 x web

compose
  process missing: 1 x web
  process missing: 3 x worker
  MISSED break: in-process-timer hc/api/management/commands/sendreports.py
  MISSED break: queue-worker hc/api/management/commands/sendalerts.py
  FALSE break: local-disk-write hc/front/management/commands/pygmentize.py (trap / not on the server)
  FALSE break: sqlite-database hc/front/management/commands/populate_searchdb.py (trap / not on the server)
  FALSE break: sqlite-database hc/front/views.py (trap / not on the server)

django-celery
  data missing: cache rabbi
  MISSED break: local-disk-write config/settings/files_and_storages.py
  FALSE break: local-disk-write markdown_tools/toc.py (trap / not on the server)

fastapi-tasks
  process missing: 1 x web
  data missing: db postg
  data missing: sqlite 
  MISSED break: local-disk-write mealie/core/settings/settings.py
  MISSED break: local-disk-write mealie/routes/admin/admin_backups.py
  MISSED break: local-disk-write mealie/routes/users/images.py
  MISSED break: local-disk-write mealie/services/backups_v2/backup_v2.py
  MISSED break: local-disk-write mealie/services/recipe/recipe_data_service.py
  MISSED break: sqlite-database mealie/core/settings/db_providers.py
  FALSE break: local-disk-write dev/code-generation/gen_py_pytest_routes.py (trap / not on the server)
  FALSE break: local-disk-write dev/code-generation/gen_ts_locales.py (trap / not on the server)
  FALSE break: local-disk-write dev/code-generation/gen_ts_types.py (trap / not on the server)
  FALSE break: local-disk-write dev/code-generation/gen_ts_unit_systems.py (trap / not on the server)
  FALSE break: local-disk-write dev/code-generation/utils/anonymize_backups.py (trap / not on the server)
  FALSE break: work-after-response mealie/services/scheduler/scheduler_service.py (trap / not on the server)
  FALSE idle: long-lived-connection mealie/routes/recipe/recipe_crud_routes.py (trap / not on the server)

firebase
  process extra: 1 x web
  data missing: db fireb
  data missing: db fires
  data missing: files 
  FALSE break: in-process-timer functions/src/collision-cache.ts (trap / not on the server)

go-fly
  data extra: files 

ios-firebase
  data missing: db fireb
  data missing: db fires

laravel
  process missing: 1 x web
  process missing: 1 x cron
  data missing: db mysql
  data missing: sqlite 
  data extra: cache redis
  MISSED break: local-disk-write app/Http/Controllers/API/Upload/UploadSongController.php
  MISSED break: local-disk-write app/Services/Image/ImageWriter.php
  MISSED break: local-disk-write app/Services/SongStorages/LocalStorage.php
  MISSED break: sqlite-database config/scout.php
  FALSE break: local-disk-write app/Services/SongStorages/RemoteDiskStorage.php (trap / not on the server)

mobile-backend
  process missing: 2 x web
  process missing: 1 x worker
  data missing: cache valke
  data missing: db postg
  MISSED break: in-process-timer server/src/repositories/cron.repository.ts
  MISSED break: local-disk-write server/src/middleware/file-upload.interceptor.ts
  MISSED break: local-disk-write server/src/repositories/storage.repository.ts
  MISSED break: local-disk-write server/src/services/database-backup.service.ts
  FALSE break: in-process-timer server/src/repositories/job.repository.ts (trap / not on the server)
  FALSE break: in-process-timer server/src/repositories/machine-learning.repository.ts (trap / not on the server)
  FALSE break: in-process-timer server/src/services/transcoding.service.ts (trap / not on the server)
  FALSE break: in-process-timer web/src/lib/managers/memory-manager.svelte.ts (trap / not on the server)
  FALSE break: in-process-timer web/src/lib/managers/queue-manager.svelte.ts (trap / not on the server)
  FALSE break: local-disk-write e2e/src/specs/server/api/asset.e2e-spec.ts (trap / not on the server)
  FALSE break: local-disk-write e2e/src/ui/generators/timeline/model-objects.ts (trap / not on the server)
  FALSE break: local-disk-write e2e/src/utils.ts (trap / not on the server)
  FALSE break: local-disk-write machine-learning/immich_ml/sessions/ann/loader.py (trap / not on the server)
  FALSE break: local-disk-write machine-learning/immich_ml/sessions/prepare.py (trap / not on the server)
  FALSE break: work-after-response machine-learning/immich_ml/main.py (trap / not on the server)

monorepo
  process missing: 4 x web
  process extra: 1 x worker
  process missing: 1 x release
  MISSED break: in-process-timer packages/hoppscotch-backend/src/posthog/posthog.service.ts
  MISSED break: local-disk-write packages/hoppscotch-selfhost-web/webapp-server/internal/crypto/keys.go
  FALSE break: in-process-timer packages/hoppscotch-common/src/composables/graphql.ts (trap / not on the server)
  FALSE break: in-process-timer packages/hoppscotch-common/src/composables/poll.ts (trap / not on the server)
  FALSE break: in-process-timer packages/hoppscotch-common/src/platform/std/kernel-interceptors/extension/index.ts (trap / not on the server)
  FALSE break: in-process-timer packages/hoppscotch-common/src/platform/std/kernel-interceptors/extension/store.ts (trap / not on the server)
  FALSE break: local-disk-write aio_run.mjs (trap / not on the server)
  FALSE break: local-disk-write packages/hoppscotch-agent/src-tauri/src/webview/mod.rs (trap / not on the server)
  FALSE break: local-disk-write packages/hoppscotch-backend/src/gql-schema.ts (trap / not on the server)
  FALSE break: local-disk-write packages/hoppscotch-cli/src/utils/reporters/junit.ts (trap / not on the server)
  FALSE break: local-disk-write packages/hoppscotch-desktop/crates/webapp-bundler/src/main.rs (trap / not on the server)
  MISSED idle: long-lived-connection packages/hoppscotch-backend/src/app.module.ts

nextjs-vercel
  data extra: files 
  FALSE break: local-disk-write lib/db/setup.ts (trap / not on the server)

nginx-spa
  process missing: 1 x web
  data missing: db fires
  FALSE break: in-process-timer packages/common/debug.ts (trap / not on the server)
  FALSE break: in-process-timer packages/excalidraw/subset/woff2/woff2-bindings.ts (trap / not on the server)
  FALSE break: local-disk-write packages/excalidraw/index-node.ts (trap / not on the server)

node-fly
  process missing: 2 x cron

phoenix
  data missing: db click
  MISSED break: in-process-timer config/runtime.exs
  MISSED break: in-process-timer lib/plausible/application.ex
  MISSED break: in-process-timer lib/plausible/ingestion/write_buffer.ex
  MISSED break: in-process-timer lib/plausible/session/salts.ex
  MISSED break: local-disk-write lib/plausible/file.ex
  MISSED break: local-disk-write lib/plausible_web/live/csv_import.ex
  MISSED break: local-disk-write lib/workers/export_analytics.ex
  MISSED break: memory-sessions lib/plausible/session/cache_store.ex
  MISSED break: queue-worker config/runtime.exs
  FALSE break: in-process-timer tracker/installation_support/verifier.js (trap / not on the server)
  FALSE break: in-process-timer tracker/src/engagement.js (trap / not on the server)
  FALSE break: local-disk-write lib/mix/tasks/create_paddle_prod_plans.ex (trap / not on the server)
  FALSE break: local-disk-write lib/mix/tasks/create_paddle_sandbox_plans.ex (trap / not on the server)
  FALSE break: local-disk-write lib/mix/tasks/download_country_database.ex (trap / not on the server)
  FALSE break: local-disk-write lib/mix/tasks/download_ip_registries.ex (trap / not on the server)
  FALSE break: local-disk-write lib/mix/tasks/generate_countries_meta.ex (trap / not on the server)
  MISSED idle: long-lived-connection lib/plausible_web/endpoint.ex

rails-sidekiq
  process missing: 1 x worker
  data missing: files 
  MISSED break: in-process-timer config/schedule.yml
  MISSED break: local-disk-write config/environments/production.rb
  MISSED idle: long-lived-connection app/views/layouts/shared/_htmldoc.html.erb

static-generator
  process missing: 1 x web
  data extra: files 
  FALSE break: local-disk-write crates/front_matter/src/lib.rs (trap / not on the server)
  FALSE break: local-disk-write crates/generate_blog/src/main.rs (trap / not on the server)

s2-shape
  process missing: 1 x web
  data missing: db fires
  data extra: files 
  FALSE break: in-process-timer website/start/carousel.js (trap / not on the server)
  FALSE break: local-disk-write website/build.py (trap / not on the server)
```
