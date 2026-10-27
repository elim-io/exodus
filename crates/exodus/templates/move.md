This repository is being moved to Google Cloud Run with Exodus. You, the coding agent, do the work. The `exodus`
command line reads the repository, prices it today and on Cloud Run, builds the report and keeps the plan. Exodus runs
no AI of its own and never signs in to Google Cloud: whatever has to happen there, you do with the person's own
`gcloud` and `bq` on this machine.

## Rules

- Source code stays on this machine. Exodus takes findings (what runs, what keeps data, which lines break on Cloud
  Run), never files. Do not paste code into an answer, a need or a message to Exodus; a finding's `code` is one line of
  evidence of at most 200 characters.
- Never print, log or put on a command line a secret value: no `.env` values, keys, tokens or connection strings. The
  audit reads the names of environment variables, never their values, and so do you. Skip any command that prints
  config values (`heroku config`, `railway variables`, `fly ssh console -C env` and the like).
- Nothing that costs money or changes an account happens without the person's yes: no billing account, no paid API
  turned on, no IAM or billing change, no DNS change.
- A report can be shared by a link, so an answer and its `--found` name no account, billing account, project id or
  person: for `account`, say whether there is one and how many.
- Talk to the person in their language. Files stay in the repository's language.
- Never run an installer yourself (`curl ... | sh` or the like). If `exodus` is not found, try `~/.local/bin/exodus`;
  if that is missing too, ask the person to run `curl -fsSL https://exodus.elim.io/install | sh` in their terminal (in
  Claude Code they can type it after `!` in this session), and go on when they say it is done.
- An account is linked with the cabinet's code: `exodus init --code CODE` in this repository's top folder. If it
  refuses the folder, the code is still unused: run the command it prints. If it answers that the code was used or
  ran out, nothing was linked: ask the person for a fresh line from the cabinet and run that. Do not go on offline
  instead unless they say so.
- If a command answers "not yet", or "needs a linked account" and the person has chosen to work without one, go on
  offline: `exodus audit`, `exodus answer` and `exodus report` work without an account. Tell the person what
  was skipped.

## 1. Audit

1. Tell the person what `exodus init` wrote: the agent instructions (`AGENTS.md`, `.claude/skills/exodus/`,
   `.cursor/rules/exodus.mdc`, whichever exist) and the `.gitignore` line, and that committing them gives a
   teammate's clone and CI the same instructions. Whether and when to commit is theirs; do not commit them as a step
   of this audit. `.exodus/` stays out of git.
2. Run `exodus audit`. It reads the manifests and the code into `.exodus/findings.json` and prints what it found:
   processes, data, the names of the settings, and the patterns that break or bill the clock on Cloud Run, each with
   its `file:line`. It also reads the shape of the project: a static site (Caddy, nginx, `serve`, a Dockerfile in a
   folder) is a web process; each deployable folder of a monorepo is a part; parts already on Google (Firebase
   Functions 2nd gen run on Cloud Run functions, Firebase Hosting, Firestore) are listed as `stays as it is`; a mobile
   app, browser code and build tools are `not on the server`. None of those is moved or priced.
3. Read the code at those lines where the scanner cannot decide: is the timer really inside the web process, does the
   work after the response matter, is the local file a cache or the only copy. Do not edit `findings.json` by hand; if
   a finding is wrong, say so to the person and in your summary.
4. Change no code during the audit.

## 2. Questions: answer first, ask only the rest

`.exodus/findings.json` lists the `questions` the audit could not answer, each with a `hint` of where the answer lives.

1. Answer each one yourself first:
   - `platform` comes first when the audit asks it (no file in the repository names the host): try the hosts' command
     lines in this folder, `railway status`, `fly status`, `heroku apps:info`, `render services`, `vercel project ls`,
     `netlify status`, `firebase projects:list`, and record the one that knows this app:
     `exodus answer platform "Railway, service web" --found "railway status" --value Railway`. Print no token,
     variable value or connection string on the way; when no CLI knows it, ask the person;
   - the platform's command line, when it is installed and signed in, read-only commands only:
     `railway status`; `heroku apps:info`, `heroku ps`, `heroku domains`, `heroku addons`, `heroku pg:info`;
     `render services`; `fly status`, `fly scale show`, `fly certs list`, `fly volumes list`. Check `<tool> --help`
     before a command you do not know, and run none that creates, scales, deploys or prints a secret (some `domain`
     commands create a domain when there is none);
   - the platform's usage or metrics pages through its API, when the person has a token in the environment already;
   - the repository: CI files, the Dockerfile, the dependencies, `.env.example` names, the code.

   Record what you found and how:
   `exodus answer <id> "<the answer in words>" --found "<how you found it>" [--value V]`.
2. Ask the person only what is left, in this session, a few questions at a time and in plain words. Record their
   answer the same way without `--found`: `exodus answer <id> "<their answer>" [--value V]`.
3. A guess is not an answer. When neither of you knows, leave the question open: the report uses its defaults (500
   people a day, 5 GB out a month, europe-west1) and says so.

`--value` is what the report computes from, beside the words it shows:

- `visits`: people a day, a number (`--value 2000`);
- `egress`: GB sent out a month, a number;
- `growth`: traffic growth in percent a month, a number;
- `users`: the Google Cloud region nearest to most users (`--value europe-west3`; a wrong id makes
  `exodus answer` list the 16 it prices);
- `connections`: who holds the open connections, `public` (the app's users: a chat, live notifications), `admin`
  (only an admin page) or `none`; with `public` the report prices the service as one instance always on;
- `platform`: the host's name, `Railway`, `Render`, `Fly.io`, `Heroku`, `Vercel`, `Netlify`, `Firebase`, or the
  name of another; the report then prices the Today column from that host's list;
- `bill`: what the platform bills a month in cents, per part id from `findings.json` (processes and data), `egress`,
  and `other` for the rest: `--value '{"web": 2000, "postgres": 1500, "egress": 100, "other": 400}'`. When the
  invoice gives one sum, record it as one: `--value '{"total": 500}'`; the report shows it on a line of its own and
  leaves the parts unknown rather than guessing a split. Read it from the invoice or the usage API and ask the person
  to confirm the total.
- `egressmb`, `filesmb` (your own, when you know them): MB sent out a visit, and MB of files each visitor a day adds.
  The forecast grows egress and the stored files with the visitors at these rates; without them, at the report's
  own (its egress over its visitors).

## 3. Say what you understood

The report is built by plain code, so it cannot say what the project means; you read the code, so you can. Once your
own answers are recorded, before the report, write three short paragraphs from what you read, not from the findings
alone:

- `project`: what the product is and who it is for, 2 to 3 sentences;
- `today`: how it is built and runs now: its parts, where the data lives, where and how it is deployed, which outside
  services it calls, 3 to 5 sentences;
- `plan`: what the move changes and why: the main changes and what they give, 2 to 4 sentences. Name the targets the
  report prices, so the words and the cost agree: a SQLite database moves to Firestore, PostgreSQL and MySQL to Cloud
  SQL, files on disk to Cloud Storage, a queue's worker to Cloud Tasks, schedules to Cloud Scheduler.

Tell the person the same in this session, in their language, and ask them to correct what is wrong. In the file, write
English, as the rest of the report, in a formal voice: facts, no praise, no marketing words. Name no secret, account,
project id, person or customer. Then record it, with the text on stdin and never on the command line:

```sh
exodus describe <<'TEXT'
project: ...
today: ...
plan: ...
TEXT
```

It goes at the top of the report, before the verdict. When the person corrects you, or a new audit changes the
picture, run it again; the next report says it changed.

## 4. The report

As soon as your own answers are recorded and the project is described, in the same turn as your questions to the person (do not wait for their
answers: the report marks the open questions and the next one fills them in), make the report with one command:

- with a linked account, `exodus send --agent "{{agent}}"`: it uploads the findings and answers, writes the report into
  `.exodus/` and prints its link. It is the only command that uploads; do not also run `exodus report`.
- without one, `exodus report`: it builds `.exodus/report.json` and `.exodus/report.md` here and uploads nothing.

Tell the person the verdict and the two monthly totals from `report.md`, and where the whole report is. When they
answer, record it and run the same command again: a new answer or a new audit makes the next report, which says what
changed, and a resend with nothing new keeps the latest one. `exodus report --pull` fetches the latest report from the
platform, for example after the person changed an answer in the cabinet.

## 5. The move, one task at a time

1. `exodus plan` writes `.exodus/plan.md` from the report: the tasks in order, each with its size and phase.
2. For each task, in order: change the code, add or change a test that proves it, run the tests, commit only this
   task's files, then `exodus task done <N> --commit <sha>`. One task, one commit. Commit as the person's git
   identity: if git has none, ask them to set it; never pass an author of your own.
3. A step only the person can do (sign in with `gcloud auth login`, create or link a billing account, approve the budget command and the spend cap of the "Budget and alerts" task, add a DNS
   record, grant a role, approve a cost) is not yours to work around. Ask for it and wait:
   `exodus need "<what you need>" --task <N> --step "<a step>" --step "<a step>|<a command to copy>"`, and say the same
   in this session. When it is done, `exodus need --clear` and go on.
4. Before guessing how Cloud Run does something, read the knowledge base: `exodus kb <topic>`,
   `exodus ask "<question>"`. The articles are also public at https://exodus.elim.io/blog.

## 6. After the move: the real bill

After a month on Cloud Run, read what Google billed and send it with
`exodus bill --project PROJECT --month YYYY-MM` (default gcloud's project and last month; `--table P.D.T` when you know
the export table, `--dry-run` to see the rows and send nothing), which runs the steps below. If you run them by hand,
read only: `gcloud ... describe/list`, `bq ls`, `bq show` and one `bq query` with a SELECT. Never
enable an API, `bq mk`, `bq cp`, `--destination_table`, change IAM or billing, or run `gcloud auth login` yourself
(ask the person). Inputs: `PROJECT`, the Google Cloud project the app moved to, and `MONTH` as `YYYYMM`.

1. Billing account: `gcloud billing projects describe "$PROJECT" --format='value(billingAccountName,billingEnabled)'`
   gives `billingAccounts/AAAAAA-BBBBBB-CCCCCC True`; `SUFFIX` is the id with `_` for `-`. If it fails, ask the person
   for the id (Console > Billing > Account management).
2. Find the export table: `gcp_billing_export_resource_v1_$SUFFIX` (detailed, preferred: it names each Cloud Run
   service) or `gcp_billing_export_v1_$SUFFIX` (standard). Candidate projects:
   `gcloud billing projects list --billing-account="$ACCOUNT" --format='value(projectId)'`. For each, stop at the
   first hit: `bq ls --project_id="$P" --format=json -n 1000`, then `bq ls --format=json -n 1000 "$P:$DATASET"`. An
   error means no BigQuery there: skip it, do not enable it.
3. Coverage: one query of `MIN(usage_start_time)`, `MAX(usage_end_time)`, `MAX(export_time)` and `COUNT(*)` where
   `invoice.month = @month AND project.id = @project`. No rows: the export does not cover this project that month. The
   month is final once the last export is 3 or more days after its last day; before that it is partial.
4. The month: one query grouped by service, `sku.id`, sku, region, unit, `cost_type` and currency, summing usage,
   `cost`, `cost_at_list` and the credits by type (free tier and discounts, promotion, commitment, all), with
   `--dry_run` first to see the bytes, `--use_legacy_sql=false` and the month and project as `--parameter`s.
5. No export at all: tell the person the export is the only place Google keeps cost per SKU, and give the steps:
   Console > Billing > Billing export > BigQuery export > Standard usage cost > Edit settings, any project on that
   account, a new dataset in the multi-region EU or US, Save (and the detailed export too). It needs Billing Account
   Administrator or Costs Manager. Send it as a need, and look again in a later session.
