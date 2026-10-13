# node-railway: report 1

From Railway to Google Cloud Run. Report 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 from 26 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found.

Today, as the audit read it:

- Runs on: Railway
- Processes: web (Express web process, always on); worker (BullMQ worker, always on); clock (node-cron process, 3 schedules)
- Data: Postgres (PostgreSQL); Redis (Redis: the BullMQ queue); uploads (Files on Railway's disk)

## Verdict

**You can move node-railway to Cloud Run after 4 changes.** The timers, the sessions in memory, the files on disk and the worker need a change each before Cloud Run; the rest moves as it is.

Today $35.55 a month on Railway (estimated from its list prices), $10.02 on Cloud Run: saves $25.53 a month, $306.36 a year.

The 4 changes before the move:

1. Timers inside the process (`src/clock.js:9`)
2. Sessions kept in process memory (`src/server.js:14`)
3. Files written to local disk (`src/server.js:30`)
4. A worker waits on a Redis queue (`src/worker.js:9`)

Confidence: lower. 15 of 15 questions open (see Questions).

## What node-railway gains on Cloud Run

Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make node-railway faster to scale and simpler to run.

A weekday of node-railway:

- On Railway: 3 processes billed every hour: 72 process-hours a day.
- On Cloud Run: CPU only while requests and jobs run, about 0.6 CPU-hours a day.

Railway can put a service to sleep when it has no traffic and wake it on the next request; that setting is off unless a service turns it on, so these processes run every hour. Cloud Run's hours are computed from this report's numbers.

- **Scales with the traffic.** From no copies at night to as many as the traffic needs, each taking up to 1,000 requests at once.
- **Nothing to keep running.** No server, operating system or certificate to look after. Google patches the machines, renews HTTPS and restarts a copy that fails.
- **Releases without downtime.** Each deploy is a revision. Send 5% of the traffic to it first, move everyone when it holds, and go back in one click.
- **A private line to the database.** Cloud Run reaches Cloud SQL over Google's network with IAM, without a public address; backups and point-in-time recovery are on.
- **The rest of Google Cloud.** BigQuery, Pub/Sub, Cloud Storage and Gemini are one permission away, on the same bill.
- **99.95% uptime in writing.** Google's SLA covers every Cloud Run service, with credits when a month falls short.

What works differently:

- A copy that starts after a quiet spell answers in about 0.5 to 2 seconds. Startup CPU boost shortens that; one copy kept warm removes it, for a few dollars a month.
- A request can run for up to 60 minutes. Longer work runs as a Cloud Run job, for up to 24 hours.
- The disk of a copy lives in its memory and goes with it, so files are kept in Cloud Storage (task 4).

## Cost

Assumptions: 500 visitors a day, europe-west1 (Belgium), 5 GB of egress a month.
Prices: Google list prices, europe-west1, read Oct 9, 2026.

| Part | Now | On Google Cloud | Today | Cloud Run |
|---|---|---|---:|---:|
| web | Express web process, always on | Cloud Run service, request-based, scales to zero | $7.00 | $0.00 |
| worker | BullMQ worker, always on | Cloud Tasks queue calling the service's /tasks handler | $7.00 | $0.00 |
| clock | node-cron process, 3 schedules | Cloud Scheduler, 3 jobs, starting a Cloud Run job | $7.00 | $0.00 |
| Postgres | PostgreSQL | Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD | $7.15 | $9.37 |
| Redis | Redis: the BullMQ queue | Removed: the queue goes to Cloud Tasks | $7.00 | $0.00 |
| uploads | Files on Railway's disk | Cloud Storage bucket, Standard | $0.15 | $0.02 |
| builds | Railway builds the image | Artifact Registry, the last 3 images | $0.00 | $0.03 |
| variables | 5 variables, 3 of them secrets | Secret Manager, 3 secrets; the rest stay plain | $0.00 | $0.00 |
| egress | Traffic out | Traffic out | $0.25 | $0.60 |
| **Total a month** | | | **$35.55** | **$10.02** |

Today: Railway's list prices for its smallest paid sizes; the bill answer replaces them.

As the visitors grow, these lines grow with them: web, worker, uploads and egress. Egress adds about 0.33 MB a visitor and the stored files about 2 MB a visitor a day. These stay as they are: clock, Postgres, Redis, builds and variables.

How each Cloud Run number is computed:

- web, Cloud Run CPU, request-based: 52,500 vCPU-s x $0.000024 = $1.26, less $1.26 free = $0.00 (10 requests a visitor, 0.35 s of 1 vCPU and 0.5 GiB each)
- web, Cloud Run memory, request-based: 26,250 GiB-s x $0.0000025 = $0.07, less $0.07 free = $0.00 (10 requests a visitor, 0.35 s of 1 vCPU and 0.5 GiB each)
- web, Cloud Run requests: 150,000 requests x $0.0000004 = $0.06, less $0.06 free = $0.00 (10 requests a visitor)
- worker, Cloud Run CPU, request-based: 3,750 vCPU-s x $0.000024 = $0.09, less $0.09 free = $0.00 (a job for every second visitor, 0.5 s of 1 vCPU each, on the service)
- worker, Cloud Run memory, request-based: 1,875 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (the same jobs at 0.5 GiB)
- worker, Cloud Run requests: 7,500 requests x $0.0000004 = $0.00, less $0.00 free = $0.00 (one request a job)
- worker, Cloud Tasks operations: 7,500 operations x $0.0000004 = $0.00, less $0.00 free = $0.00 (one task a job)
- clock, Cloud Scheduler jobs: 3 jobs x $0.10 = $0.30, less $0.30 free = $0.00 (0 * * * *, 0 2 * * *, 0 8 * * 1)
- clock, Cloud Run jobs CPU: 7,540 vCPU-s x $0.000018 = $0.14, less $0.14 free = $0.00 (754 job runs a month, each taken as 10 s at 1 vCPU)
- clock, Cloud Run jobs memory: 3,770 GiB-s x $0.000002 = $0.01, less $0.01 free = $0.00 (the same runs at 0.5 GiB)
- Postgres, Cloud SQL db-f1-micro: 730 hours x $0.0105 = $7.67 (always on, 730 hours)
- Postgres, Cloud SQL SSD storage: 10 GB-months x $0.17 = $1.70 (10 GB, Cloud SQL's smallest disk)
- uploads, Cloud Storage, Standard: 1 GB-months x $0.02 = $0.02 (1 GB of files, taken as the size)
- uploads, Cloud Storage writes, class A: 2,000 writes x $0.000005 = $0.01, less $0.01 free = $0.00 (about 2,000 files written a month, taken as the count)
- builds, Artifact Registry storage: 0.81 GB-months x $0.10 = $0.08, less $0.05 free = $0.03 (3 images of 270 MB)
- variables, Secret Manager active versions: 3 versions x $0.06 = $0.18, less $0.18 free = $0.00 (3 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Now and after

- web: Express web process, always on -> Cloud Run service, request-based, scales to zero
- worker: BullMQ worker, always on -> Cloud Tasks queue calling the service's /tasks handler
- clock: node-cron process, 3 schedules -> Cloud Scheduler, 3 jobs, starting a Cloud Run job
- Postgres: PostgreSQL -> Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD
- ~~Redis: Redis: the BullMQ queue~~ -> removed: the queue goes to Cloud Tasks
- uploads: Files on Railway's disk -> Cloud Storage bucket, Standard
- builds: Railway builds the image -> Artifact Registry, the last 3 images
- variables: 5 variables, 3 of them secrets -> Secret Manager, 3 secrets; the rest stay plain

Read from the repository: Node 20, Express 4, BullMQ 5, PostgreSQL, Redis, railway.json. The container: Node 20 container running Express; Buildpacks or your Dockerfile, on node:20-slim.

## What needs a change

### Would not work on Cloud Run (4)

**1. Timers inside the process**

`src/clock.js:9`: `cron.schedule('0 * * * *', cleanup);`

Why: A service at zero has no process to run a timer, a running one gets no CPU between requests, and with several instances the timer fires once per instance: late, never, or twice.

Instead: Cloud Scheduler on the same cron lines: short work calls POST /cron/<name> on the service with an OIDC token, long work starts a Cloud Run job. Set each job's time zone; the timer ran on the server's, usually UTC.

Read: https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler

Task: 2. Timers to Cloud Scheduler

**2. Sessions kept in process memory**

`src/server.js:14`: `app.use(session({ secret: process.env.SESSION_SECRET, resave: false, saveUninitialized: false }));`

Why: Each instance has its own memory and loses it when it stops, so a user is signed out whenever a request lands on another instance or a new one.

Instead: Signed cookies (cookie-session), with the key in Secret Manager.

Read: https://exodus.elim.io/blog/sessions-without-redis

Task: 3. Sessions to signed cookies

**3. Files written to local disk**

`src/server.js:30`: `await fs.writeFile(path.join(UPLOAD_DIR, name), req.file.buffer);`

Why: An instance's disk lives in its memory: a file counts against the memory limit, is gone when the instance stops, and other instances never see it, so an upload saved on one is a 404 on the next.

Instead: Write to a Cloud Storage bucket and hand out signed URLs; for uploads, let the browser PUT the file to a signed URL so the bytes never pass through the service.

Read: https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage

Task: 4. Files to a Cloud Storage bucket

**4. A worker waits on a Redis queue**

`src/worker.js:9`: `new Worker('jobs', handle, { connection: redis });`

Why: Cloud Run gives an instance CPU only while it answers a request, so a worker waiting on a queue gets no CPU between requests and its jobs stall. With CPU always allocated it runs, and bills every second whether there is work or not.

Instead: Cloud Tasks: where the code adds a job, it creates a task that POSTs to /tasks/<name> on the service, and a handler there does the job and answers 2xx. Retries, backoff and rate limits move to the queue; check the OIDC token and make the handler safe to run twice.

Read: https://exodus.elim.io/blog/background-workers-with-cloud-tasks

Task: 5. Worker: queue jobs to a Cloud Tasks handler

Looked for and not found: work after the response, sign-in state in memory, sessions kept in Redis, long-lived connections, migrations in a release step, background threads, SQLite or files used as a database.

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Dockerfile and the health check: A Node 20 image that listens on $PORT; its health route /healthz is the startup probe and answers 200 once Postgres answers. | - | S | To do |
| 2 | Timers to Cloud Scheduler: Give each schedule an endpoint (POST /cron/<name>, OIDC token checked) or a Cloud Run job, create a Cloud Scheduler job per schedule with its time zone, then remove the timer and its process. | Finding 1 | M | To do |
| 3 | Sessions to signed cookies: Keep a user id, a role and an expiry in a signed cookie (key in Secret Manager; Secure, HttpOnly, SameSite=Lax; under 4 KB). For log out everywhere, keep a session version on the user row. | Finding 2 | S | To do |
| 4 | Files to a Cloud Storage bucket: Write to a bucket instead of the disk; for uploads, sign a PUT URL so the browser sends the file straight to the bucket and the service stores only the key. Copy the existing files over before the switch. | Finding 3 | M | To do |
| 5 | Worker: queue jobs to a Cloud Tasks handler: Replace each queue.add() with a Cloud Tasks task that POSTs to /tasks/<name>, move the processor into that route, check the OIDC token, then drop the worker process and its Redis. | Finding 4 | L | To do |
| 6 | Infrastructure: service, Cloud SQL, bucket, queue, scheduler: Max 3 instances and a database pool of 5 a copy, because db-f1-micro takes 25 connections. | - | M | To do |
| 7 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 8 | Data: copy Postgres and uploads and compare: pg_dump into Cloud SQL, then row counts and checksums per table; the files with gcloud storage rsync. | - | M | To do |
| 9 | Switch the domain and stop Railway: Point the domains at Cloud Run in a quiet hour; Railway stays stopped, not deleted, for a week. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Prepare: Dockerfile and the health check.
2. Change the code: Timers to Cloud Scheduler; Sessions to signed cookies; Files to a Cloud Storage bucket; Worker: queue jobs to a Cloud Tasks handler.
3. Stand up on Google Cloud: Infrastructure: service, Cloud SQL, bucket, queue, scheduler; Keyless deploys from CI.
4. Cut over: Data: copy Postgres and uploads and compare; Switch the domain and stop Railway.

## Questions

15 questions: 0 found by your agent, 0 answered by you, 15 open.

1. How many people use the app a day? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the web line.
2. What does Railway bill you a month, per service? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the Today column.
3. Where are most of your users? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Region.
4. How much data does the app send out a month? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: egress.
5. Do clients keep connections open: WebSockets, server-sent events, long polling? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Findings.
6. When is the busiest hour, and how busy is it? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the instance limit.
7. Which domains point at the app? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: the domains to switch.
8. Can the app be down for a few minutes during the switch? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: the cutover.
9. May the worker's jobs run in parallel, or twice? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: the worker task.
10. Where do the tests and the deploys run? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: keyless deploys.
11. Which outside services does the app call? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: webhooks and allow lists to update.
12. Which environments does the app run in? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the services to create.
13. How fast is the traffic growing? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the forecast.
14. Is there a Google Cloud billing account to use? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: phase 3 needs it.
15. May the scheduled jobs run twice, or must each run exactly once? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: the scheduled jobs.

## Everything the audit sent

26 findings: 3 processes, 3 data, 3 build, settings and traffic, 6 stack, 4 patterns found, 7 looked for, not found.

### Processes (3)

- web: Express web process, always on
- worker: BullMQ worker, always on
- clock: node-cron process, 3 schedules

### Data (3)

- Postgres: PostgreSQL
- Redis: Redis: the BullMQ queue
- uploads: Files on Railway's disk

### Build, settings and traffic (3)

- builds: Railway builds the image
- variables: 5 variables, 3 of them secrets
- egress: Traffic out

### Stack (6)

- Node 20
- Express 4
- BullMQ 5
- PostgreSQL
- Redis
- railway.json

### Patterns found (4)

- Timers inside the process (src/clock.js:9)
- Sessions kept in process memory (src/server.js:14)
- Files written to local disk (src/server.js:30)
- A worker waits on a Redis queue (src/worker.js:9)

### Looked for, not found (7)

- No work after the response
- No sign-in state in memory
- No sessions kept in Redis
- No long-lived connections
- No migrations in a release step
- No background threads
- No SQLite or files used as a database

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/railway-or-render-to-cloud-run (cited in Now and after)
- https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler (cited in Finding 1)
- https://exodus.elim.io/blog/sessions-without-redis (cited in Finding 2)
- https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage (cited in Finding 3)
- https://exodus.elim.io/blog/background-workers-with-cloud-tasks (cited in Finding 4)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Cloud Tasks pricing: https://cloud.google.com/tasks/pricing
- Cloud Scheduler pricing: https://cloud.google.com/scheduler/pricing
- Cloud SQL pricing: https://cloud.google.com/sql/pricing
- Cloud Storage pricing: https://cloud.google.com/storage/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Secret Manager pricing: https://cloud.google.com/secret-manager/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this report was made: exodus 0.1.0 read 12 files of node-railway on your machine and found 26 findings (7 KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.
