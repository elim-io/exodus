# Pattern rules

Each file here is one rule for one language: `<language>/<id>.toml`. `any/` holds the rules that read manifests
(Procfile, render.yaml, fly.toml, railway.json, app.json, Dockerfile) rather than code. The files are compiled into
the binary (`BUILTIN` in `src/rules.rs`, which a test keeps in step with this folder), `exodus audit` turns their hits
into `patterns` and `clean`, and `exodus check` runs the same set.

The rules are the patterns that break on Cloud Run, or bill it while idle: a worker waiting on a queue, work after the
response, timers inside the process, files written to local disk, sessions in memory, sessions kept in Redis
(`idle`: it works, but a managed Redis bills all month), long-lived connections,
migrations in a release step, background threads, and SQLite as the database.

## Fields

| field | | |
|---|---|---|
| `id` | required | the rule id and the file name; one id may have a file in several language folders |
| `kind` | required | `breaks`, `idle` (works, but bills while nothing happens) or `note` (something to carry over) |
| `title` | required | one line, the finding's title |
| `clean` | optional | the label `clean` lists when the rule finds nothing ("in-process timers"); a platform note has none |
| `platform` | optional | the rule runs only when the audit names this platform ("Heroku") |
| `include` | required | globs of the files it reads, relative to the repository root (`**/*.py`) |
| `exclude` | optional | globs it skips (a frontend folder) |
| `regex` | required | one or more; each is matched against the whole file in multi-line mode, so `^` and `$` are line ends and `[^}]*` may cross lines. The hit's line is where the match starts, or where a group `(?P<at>...)` starts |
| `unless` | optional | a hit is dropped when the lines it spans match this (`store:` for a session with a store of its own) |
| `unless_in` | optional | a hit is dropped when a block it sits in opens with a line matching this: each enclosing line, walking up (`def lifespan(` for a task the app starts at startup, before any response) |
| `stop_at` | optional | the text after the first occurrence of this is not read (`#[cfg(test)]` for Rust, whose unit tests share the file) |
| `why` | required | why it breaks on Cloud Run, in a sentence or two |
| `fix` | required | what replaces it, in this language's words, for any framework |
| `fixes` | optional | the fix in a framework's words, by its badge (`Django = "..."`): used when the finding's process runs that framework, else when the repository uses it |
| `kb` | required | the slug of the guide that explains it, as https://exodus.elim.io/blog/SLUG serves it |
| `part` | required | the kind of part a hit lands on: `web`, `worker`, `cron`, `release` (a process) or `files`, `sqlite`, `db`, `cache` (a data item) |
| `subject` | optional | how the report names what moves ("the worker") |
| `takes_over` | optional | the clause the rule adds to a part the move drops ("the queue goes to Cloud Tasks") |
| `stay` | optional | `true` when the rule's firing explains a "stay" verdict |
| `[task]` | optional | the checklist task: `title`, `detail`, `effort` (S, M, L), `phase` (1 Prepare, 2 Change the code, 3 Stand up on Google Cloud, 4 Cut over) |

## How hits become findings

- A rule reports the first hit in each file. Its finding's id is the rule id; the second file's is `<id>-2`, and so
  on, in file then line order. A finding lists at most 5 places per rule (`PLACES_PER_RULE`); `exodus check` sees all.
- A line that starts with `//`, `/*`, `* `, `--` or `#` is a comment and never a hit.
- A hit counts only in code that runs on the server. Browser code (front-end files in `public/`, `static/`,
  `assets/`, `www/`, `site/`, `website/` and the usual front-end folders, a package with a UI library and no server
  library, a file that uses `document`, `window` or `addEventListener` and no Node module), build and tool code
  (`scripts/`, `tools/`, `bin/`, `dev/`, rake and mix tasks, Django management commands, `build.*`, `generate*`,
  `setup.*`, `seed.*`, configs, notebooks), mobile apps (`ios/`, `android/`, `*.swift`, `*.dart`, Android
  modules), desktop apps (`src-tauri/`, Electron), tests and docs do not; a hit there is named in `clean` as
  `not on the server: <place> (<what runs there>)` instead, unless a process runs the file. The folder and file
  names, the UI and server libraries and the DOM test are data in `places.toml`; the verdict is
  `Placer::off_server` in `src/place.rs`.
- A hit lands on the process or data item declared on its line, else the process whose schedule or entry file it is
  in, else the first part of the rule's `part` kind, else the web process.

## Writing one

Keep a regex narrow enough that a false hit is rare: a finding asks a person to change code, and a wrong one costs
their trust. Add a line to `rules_fire_where_they_should_and_not_elsewhere` in `src/rules.rs` for the nearest case the rule
must not catch, a row to the table in `tests/reference.rs` for the one it must, add the file to `BUILTIN`, and run `cargo test`. If a sample in
`examples/` should show the pattern, add it there and to its `expected.json`.
