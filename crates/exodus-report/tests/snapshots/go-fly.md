# go-fly: report 1

From Fly.io to Google Cloud Run. Report 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 from 21 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found.

Today, as the audit read it:

- Runs on: Fly.io
- Processes: app (net/http web process, always on); app timers (1 schedule inside the app process)
- Data: cards_data (SQLite file on a volume)

## Verdict

**You can move go-fly to Cloud Run after 2 changes.** The SQLite file and the timers need a change each before Cloud Run; the rest moves as it is.

Today $3.57 a month on Fly.io (estimated from its list prices), $0.60 on Cloud Run: saves $2.97 a month, $35.64 a year.

The 2 changes before the move:

1. SQLite on a volume as the database (`main.go:15`)
2. Timers inside the process (`main.go:36`)

Confidence: lower. 15 of 15 questions open (see Questions).

## What go-fly gains on Cloud Run

Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make go-fly faster to scale and simpler to run.

A weekday of go-fly:

- On Fly.io: 1 process billed every hour: 24 process-hours a day.
- On Cloud Run: CPU only while requests and jobs run, about 0.1 CPU-hours a day.

Fly.io can stop idle machines (auto_stop_machines); fly.toml keeps them running, so they run every hour. Cloud Run's hours are computed from this report's numbers.

- **Scales with the traffic.** From no copies at night to as many as the traffic needs, each taking up to 1,000 requests at once.
- **Nothing to keep running.** No server, operating system or certificate to look after. Google patches the machines, renews HTTPS and restarts a copy that fails.
- **Releases without downtime.** Each deploy is a revision. Send 5% of the traffic to it first, move everyone when it holds, and go back in one click.
- **Close to your users.** Requests enter Google's network near the visitor and travel on Google's own backbone to the region.
- **The rest of Google Cloud.** BigQuery, Pub/Sub, Cloud Storage and Gemini are one permission away, on the same bill.
- **99.95% uptime in writing.** Google's SLA covers every Cloud Run service, with credits when a month falls short.

What works differently:

- A copy that starts after a quiet spell answers in about 0.5 to 2 seconds. Startup CPU boost shortens that; one copy kept warm removes it, for a few dollars a month.
- A request can run for up to 60 minutes. Longer work runs as a Cloud Run job, for up to 24 hours.
- The disk of a copy lives in its memory and goes with it, so files are kept in Cloud Storage.

## Cost

Assumptions: 500 visitors a day, europe-west1 (Belgium), 5 GB of egress a month.
Prices: Google list prices, europe-west1, read Oct 9, 2026.

| Part | Now | On Google Cloud | Today | Cloud Run |
|---|---|---|---:|---:|
| app | net/http web process, always on | Cloud Run service, request-based, scales to zero | $3.32 | $0.00 |
| app timers | 1 schedule inside the app process | Cloud Scheduler, 1 job, calling the service | $0.00 | $0.00 |
| cards_data | SQLite file on a volume | Firestore, Standard edition, billed per read, write and GiB stored | $0.15 | $0.00 |
| builds | Fly.io builds the image | Artifact Registry, the last 3 images | $0.00 | $0.00 |
| variables | 1 variable, 0 of them secrets | No secrets; the variables stay plain | $0.00 | $0.00 |
| egress | Traffic out | Traffic out | $0.10 | $0.60 |
| **Total a month** | | | **$3.57** | **$0.60** |

Today: Fly.io's list prices for its smallest paid sizes; the bill answer replaces them.

As the visitors grow, these lines grow with them: app, cards_data and egress. Egress adds about 0.33 MB a visitor. These stay as they are: app timers, builds and variables.

How each Cloud Run number is computed:

- app, Cloud Run CPU, request-based: 7,200 vCPU-s x $0.000024 = $0.17, less $0.17 free = $0.00 (8 requests a visitor, 0.06 s of 1 vCPU and 0.5 GiB each)
- app, Cloud Run memory, request-based: 3,600 GiB-s x $0.0000025 = $0.01, less $0.01 free = $0.00 (8 requests a visitor, 0.06 s of 1 vCPU and 0.5 GiB each)
- app, Cloud Run requests: 120,000 requests x $0.0000004 = $0.05, less $0.05 free = $0.00 (8 requests a visitor)
- app timers, Cloud Scheduler jobs: 1 jobs x $0.10 = $0.10, less $0.10 free = $0.00 (*/10 * * * *)
- app timers, Cloud Run CPU, request-based: 259.2 vCPU-s x $0.000024 = $0.01, less $0.01 free = $0.00 (4,320 passes a month called on the service, 0.06 s of 1 vCPU each, as one of its requests)
- app timers, Cloud Run memory, request-based: 129.6 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (the same passes at 0.5 GiB)
- app timers, Cloud Run requests: 4,320 requests x $0.0000004 = $0.00, less $0.00 free = $0.00 (one request a pass)
- cards_data, Firestore document reads: 240,000 reads x $0.00000033 = $0.08, less $0.08 free = $0.00 (8 requests a visitor, 2 document reads each)
- cards_data, Firestore document writes: 24,000 writes x $0.00000099 = $0.02, less $0.02 free = $0.00 (8 requests a visitor, 0.2 document writes each)
- cards_data, Firestore stored data: 1 GiB-months x $0.165 = $0.17, less $0.17 free = $0.00 (the size is not in the repository; taken as 1 GiB)
- builds, Artifact Registry storage: 0.09 GB-months x $0.10 = $0.01, less $0.01 free = $0.00 (3 images of 30 MB)
- variables, Secret Manager active versions: 0 versions x $0.06 = $0.00 (0 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Now and after

- app: net/http web process, always on -> Cloud Run service, request-based, scales to zero
- app timers: 1 schedule inside the app process -> Cloud Scheduler, 1 job, calling the service
- cards_data: SQLite file on a volume -> Firestore, Standard edition, billed per read, write and GiB stored
- builds: Fly.io builds the image -> Artifact Registry, the last 3 images
- variables: 1 variable, 0 of them secrets -> No secrets; the variables stay plain

Read from the repository: Go 1.23, net/http, SQLite, fly.toml. The container: Go 1.23 container running net/http; Buildpacks or your Dockerfile, on distroless, a static binary.

## What needs a change

### Would not work on Cloud Run (2)

**1. SQLite on a volume as the database**

`main.go:15`: `db, err := sql.Open("sqlite", os.Getenv("DB_PATH"))`

Why: Cloud Run has no disk a database file can live on: the filesystem is memory, and each instance would hold its own copy that vanishes when it stops.

Instead: Move the data to Firestore before the move: each table becomes a collection and each row a document, the database/sql calls become reads and writes through cloud.google.com/go/firestore, and a join becomes a second read or a field copied into the document.

Read: https://exodus.elim.io/blog/cloud-sql-firestore-or-serverless-postgres

Task: 2. SQLite to Firestore

**2. Timers inside the process**

`main.go:36`: `ticker := time.NewTicker(10 * time.Minute)`

Why: A service at zero has no process to run a timer, a running one gets no CPU between requests, and with several instances the timer fires once per instance: late, never, or twice.

Instead: Cloud Scheduler on the same cron lines: short work calls POST /cron/<name> on the service with an OIDC token, long work starts a Cloud Run job. Set each job's time zone; the timer ran on the server's, usually UTC.

Read: https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler

Task: 3. Timers to Cloud Scheduler

Looked for and not found: workers waiting on a queue, work after the response, files written to disk, sessions in memory, sign-in state in memory, sessions kept in Redis, long-lived connections, migrations in a release step, background threads.

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Dockerfile and a health check route: Dockerfile exists: check that it listens on $PORT; GET /healthz answers 200 once cards_data answers. | - | S | To do |
| 2 | SQLite to Firestore: Replace the database/sql calls with cloud.google.com/go/firestore: a collection per table, a document per row, a transaction where the code needs one, and a composite index for each query that filters or sorts on two fields. The tests run against the Firestore emulator. | Finding 1 | L | To do |
| 3 | Timers to Cloud Scheduler: Give each schedule an endpoint (POST /cron/<name>, OIDC token checked) or a Cloud Run job, create a Cloud Scheduler job per schedule with its time zone, then remove the timer and its process. | Finding 2 | M | To do |
| 4 | Infrastructure: service, Firestore, scheduler: Max 3 instances to begin with; the busiest hour decides when to raise it. Firestore's (default) database in Native mode in the service's region, the one database the free quota covers; the service's account gets roles/datastore.user. | - | M | To do |
| 5 | Budget and alerts: A Cloud Billing budget of $5 a month on this project (the Cloud Run estimate rounded up, at least $5), with email alerts to the billing admins at 50%, 90% and 100% of actual spend and at 100% of the forecast: `gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID --display-name="go-fly on Cloud Run" --budget-amount=5USD --calendar-period=month --filter-projects=projects/PROJECT_ID --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0 --threshold-rule=percent=1.0,basis=forecasted-spend` (the currency is the billing account's own; `--notifications-rule-pubsub-topic` adds a Pub/Sub topic for automated reactions). It needs Billing Account Administrator or Billing Account Costs Manager, or the project's owner for a budget on this project only. It changes billing settings: the agent shows the command, the person approves, and it runs with the person's own gcloud. How budgets work: https://exodus.elim.io/blog/budget-alerts-and-the-billing-export. | - | S | To do |
| 6 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 7 | Data: copy cards_data and compare: a one-off script copies each SQLite table into a Firestore collection, a row a document, then the documents of each collection are counted against the table's rows. | - | M | To do |
| 8 | Switch the domain and stop Fly.io: Point the domains at Cloud Run in a quiet hour; Fly.io stays stopped, not deleted, for a week. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Prepare: Dockerfile and a health check route.
2. Change the code: SQLite to Firestore; Timers to Cloud Scheduler.
3. Stand up on Google Cloud: Infrastructure: service, Firestore, scheduler; Budget and alerts; Keyless deploys from CI.
4. Cut over: Data: copy cards_data and compare; Switch the domain and stop Fly.io.

## Questions

15 questions: 0 found by your agent, 0 answered by you, 15 open.

1. How many people use the app a day? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the web line.
2. What does Fly.io bill you a month, per service? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the Today column.
3. Where are most of your users? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Region.
4. How much data does the app send out a month? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: egress.
5. Do clients keep connections open: WebSockets, server-sent events, long polling? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Findings.
6. When is the busiest hour, and how busy is it? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the instance limit.
7. Which domains point at the app? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: the domains to switch.
8. Can the app be down for a few minutes during the switch? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: the cutover.
9. Where do the tests and the deploys run? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: keyless deploys.
10. Which outside services does the app call? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: webhooks and allow lists to update.
11. Which environments does the app run in? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the services to create.
12. How fast is the traffic growing? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the forecast.
13. Is there a Google Cloud billing account to use? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: phase 3 needs it.
14. How busy is the CPU over a day? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Verdict and cost: always-on billing.
15. May the scheduled jobs run twice, or must each run exactly once? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: the scheduled jobs.

## Everything the audit sent

21 findings: 2 processes, 1 data, 3 build, settings and traffic, 4 stack, 2 patterns found, 9 looked for, not found.

### Processes (2)

- app: net/http web process, always on
- app timers: 1 schedule inside the app process

### Data (1)

- cards_data: SQLite file on a volume

### Build, settings and traffic (3)

- builds: Fly.io builds the image
- variables: 1 variable, 0 of them secrets
- egress: Traffic out

### Stack (4)

- Go 1.23
- net/http
- SQLite
- fly.toml

### Patterns found (2)

- SQLite on a volume as the database (main.go:15)
- Timers inside the process (main.go:36)

### Looked for, not found (9)

- No workers waiting on a queue
- No work after the response
- No files written to disk
- No sessions in memory
- No sign-in state in memory
- No sessions kept in Redis
- No long-lived connections
- No migrations in a release step
- No background threads

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/fly-to-cloud-run (cited in Now and after)
- https://exodus.elim.io/blog/cloud-sql-firestore-or-serverless-postgres (cited in Finding 1)
- https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler (cited in Finding 2)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Cloud Scheduler pricing: https://cloud.google.com/scheduler/pricing
- Firestore pricing: https://cloud.google.com/firestore/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this report was made: exodus 0.1.0 read 5 files of go-fly on your machine and found 21 findings (5.1 KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.
