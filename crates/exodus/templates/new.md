This folder is for a product that is not built yet. With Exodus you plan it for Google Cloud Run and then build it
there. You, the coding agent, ask about the idea, choose the parts and write the code. The `exodus` command line turns
your choice into a report (the cost at launch and in a year, the architecture, the design decisions, the phases
Scaffold, Build the core, Stand up on Google Cloud, Launch) and keeps the plan. Exodus runs no AI of its own and never
signs in to Google Cloud: whatever has to happen there, you do with the person's own `gcloud` on this machine.

## Rules

- Source code stays on this machine. Exodus takes the plan's facts, never files.
- Never print, log or put on a command line a secret value: no keys, tokens or connection strings.
- Nothing that costs money or changes an account happens without the person's yes: no billing account, no paid API
  turned on, no IAM or billing change, no DNS change.
- Talk to the person in their language. Files stay in English unless the person writes the product in another.
- Do not run `exodus audit` here: it reads code that exists, and it would replace the findings you write.
- Never run an installer yourself (`curl ... | sh` or the like). If `exodus` is not found, try `~/.local/bin/exodus`;
  if that is missing too, ask the person to run `curl -fsSL https://exodus.elim.io/install | sh` in their terminal (in
  Claude Code they can type it after `!` in this session), and go on when they say it is done.
- An account is linked with the cabinet's code: `exodus init --code CODE --new` in this folder. If it refuses the
  folder, the code is still unused: run the command it prints. If it answers that the code was used or ran out,
  nothing was linked: ask the person for a fresh line from the cabinet and run that. Do not go on offline instead
  unless they say so.
- If a command answers "not yet", or "needs a linked account" and the person has chosen to work without one, go on
  offline: `exodus answer` and `exodus report` work without an account. Tell the person what was skipped.

## 1. Ask about the idea

Tell the person what `exodus init` wrote: the agent instructions (`AGENTS.md`, `.claude/skills/exodus/`,
`.cursor/rules/exodus.mdc`, whichever exist) and the `.gitignore` line. Do not commit them and do not run `git init`
for them; the folder's git is the person's. `.exodus/` stays out of git.

Ask the person in this session, a few questions at a time and in plain words, and record each answer with
`exodus answer <id> "<their answer>" [--value V]`:

- `what`: what will the product do, in a sentence?
- `who`: who uses it, and from where? `--value` is the Google Cloud region nearest to them (`europe-west3`;
  a wrong id makes `exodus answer` list the 16 it prices).
- `visits`: how many people a day at launch? `--value` a number.
- `growth`: how fast should it grow in the first year? `--value` percent a month.
- `payments`: does it take payments?
- `realtime`: does anything need live updates in the browser?
- `stack`: which language and framework? If the person has no preference, propose one and say why.
- `egress`: how much data will it send out a month? `--value` GB.

The year in the plan grows egress and the stored files with the visitors, at the launch's own rate (its egress over its
visitors). When the person knows better, for example a product that serves video or keeps every upload, record
`egressmb` (MB sent out a visit) or `filesmb` (MB of files each visitor a day adds) with `--value` a number.

Do not answer these yourself: the idea is the person's. When they do not know a number, leave it open; the plan uses
500 people a day, 5 GB a month and europe-west1, and says so.

## 2. Choose the parts

Design for Cloud Run from the start:

- the web app or API is one Cloud Run service that keeps nothing between requests;
- anything on a timer is Cloud Scheduler starting a Cloud Run job, never a timer inside the service;
- work that may take longer than the request, or must survive a retry, is a Cloud Tasks task the service handles;
- files go to Cloud Storage, never the container's disk;
- sessions live in signed cookies, not in memory and not in Redis kept only for them;
- a relational database is Cloud SQL for PostgreSQL; a small document store is Firestore;
- a payment webhook checks the signature, queues a task and answers at once;
- live updates: an open WebSocket or event stream keeps an instance billed while it is open, up to 60 minutes a
  request; prefer short polling unless the product needs more.

## 3. Write the findings

Write `.exodus/findings.json` by hand, in this shape: `kind` is `"new"`, every `from` and `at` is `"design"`, each
design decision is a pattern with `rule` `"design"` and `kind` `"note"`, `part` names the process or data it belongs
to, and `questions` lists the questions you asked.

```json
{
  "exodus": "findings/1",
  "kind": "new",
  "repo": { "name": "class-booking" },
  "platform": { "name": "Unknown", "file": "" },
  "stack": ["Rust 1.90", "Axum 0.8", "React 19", "PostgreSQL 16"],
  "what": "Bookings and payments for small fitness studios",
  "manifests": [],
  "processes": [
    { "id": "web", "kind": "web", "name": "api and app", "command": "", "from": "design", "alwaysOn": false, "framework": "Axum" },
    { "id": "reminders", "kind": "cron", "name": "reminders", "command": "", "from": "design", "alwaysOn": false,
      "schedules": [{ "cron": "*/15 * * * *", "from": "design" }] }
  ],
  "data": [
    { "id": "postgres", "kind": "db", "name": "Postgres", "engine": "PostgreSQL", "version": "16", "from": "design" },
    { "id": "photos", "kind": "files", "name": "photos", "engine": "bucket", "sizeMb": 1000, "from": "design" }
  ],
  "settings": { "envNames": [], "secretsGuess": 5, "variablesTotal": 5 },
  "patterns": [
    { "id": "reminders", "rule": "design", "kind": "note", "title": "Reminders run as a job on a schedule", "at": "design",
      "code": "Cloud Scheduler: */15 * * * * -> Cloud Run job reminders",
      "why": "A timer inside the API would run on every copy at once, or on none when nobody calls.",
      "fix": "Cloud Scheduler starts a Cloud Run job every 15 minutes; it sends what is due and exits.",
      "part": "reminders", "kb": "cron-and-timers-with-cloud-scheduler" }
  ],
  "clean": [],
  "questions": [
    { "id": "what", "q": "What will the product do, in a sentence?", "affects": "Architecture" },
    { "id": "visits", "q": "How many people a day at launch?", "affects": "Cost: the web line" }
  ],
  "stats": { "files": 0, "bytes": 0, "rulesRun": 0, "startedAt": "", "finishedAt": "" },
  "cli": "0.1.0"
}
```

Process kinds are `web`, `worker` and `cron`; data kinds are `db`, `cache` and `files`. `kb` is the slug of a
knowledge base article (`cron-and-timers-with-cloud-scheduler`, `background-workers-with-cloud-tasks`,
`sessions-without-redis`, `files-and-uploads-with-cloud-storage`), or empty.

## 4. Say what you understood

The plan is built by plain code, so it cannot say what the product is; you heard the idea, so you can. After the
findings are written, write three short paragraphs:

- `project`: the idea as you understood it: what the product does and for whom, 2 to 3 sentences;
- `today`: where it starts: what exists already (nothing, a mock-up, a domain, a data source) and what the person has
  chosen, 2 to 4 sentences;
- `plan`: what we will build and why: the main parts on Google Cloud and what they give, 2 to 4 sentences.

Tell the person the same in this session, in their language, and ask them to correct what is wrong. In the file, write
English, as the rest of the plan, in a formal voice: facts, no praise, no marketing words. Name no secret, account,
project id, person or customer. Then record it, with the text on stdin and never on the command line:

```sh
exodus describe <<'TEXT'
project: ...
today: ...
plan: ...
TEXT
```

It goes at the top of the plan. When the person corrects you, run it again.

## 5. The plan

Make the plan with one command. With a linked account, `exodus send --agent "{{agent}}"` uploads the findings and
answers, writes the plan into `.exodus/` and prints its link; it is the only command that uploads, so do not also run
`exodus report`. Without one, `exodus report` builds `.exodus/report.json` and `.exodus/report.md` here and uploads
nothing. Tell the person the cost at launch and in a year from `report.md`.

## 6. Build it, one task at a time

1. `exodus plan` writes `.exodus/plan.md` from the report: the tasks in order, each with its size and phase.
2. For each task, in order: write the code, add a test that proves it, run the tests, commit only this task's files,
   then `exodus task done <N> --commit <sha>`. One task, one commit. If the folder is not a git repository yet, ask
   the person before `git init`; commit as their git identity, and if git has none, ask them to set it rather than
   passing an author of your own.
3. A step only the person can do (sign in with `gcloud auth login`, create or link a billing account (the Infrastructure task says why and names the Free Trial), approve the budget command and the spend cap of the "Budget and alerts" task, add a DNS
   record, grant a role, approve a cost) is not yours to work around. Ask for it and wait:
   `exodus need "<what you need>" --task <N> --step "<a step>" --step "<a step>|<a command to copy>"`, and say the same
   in this session. When it is done, `exodus need --clear` and go on.
4. Before guessing how Cloud Run does something, read the knowledge base: `exodus kb <topic>`,
   `exodus ask "<question>"`. The articles are also public at https://exodus.elim.io/blog.
