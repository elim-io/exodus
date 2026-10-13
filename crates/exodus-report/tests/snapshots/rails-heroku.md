# rails-heroku: report 1

From Heroku to Google Cloud Run. Report 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 from 29 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found.

Today, as the audit read it:

- Runs on: Heroku
- Processes: web (Rails web process, always on); worker (Sidekiq worker, always on); Heroku Scheduler (Heroku Scheduler, jobs set in the dashboard)
- Data: Postgres (PostgreSQL); Redis (Redis: the Sidekiq queue); Active Storage (Files on Heroku's disk)

## Verdict

**You can move rails-heroku to Cloud Run after 2 changes.** The files on disk and the worker need a change each before Cloud Run; the rest moves as it is.

Today $22.00 a month on Heroku (estimated from its list prices), $10.05 on Cloud Run: saves $11.95 a month, $143.40 a year.

The 2 changes before the move:

1. Files written to local disk (`config/environments/production.rb:3`)
2. A worker waits on a Redis queue (`config/environments/production.rb:4`)

Confidence: lower. 15 of 15 questions open (see Questions).

## What rails-heroku gains on Cloud Run

Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make rails-heroku faster to scale and simpler to run.

A weekday of rails-heroku:

- On Heroku: 2 processes billed every hour: 48 process-hours a day.
- On Cloud Run: CPU only while requests and jobs run, about 0.4 CPU-hours a day.

Heroku sleeps only Eco dynos; this app runs on paid dynos, which run every hour. Cloud Run's hours are computed from this report's numbers.

- **Scales with the traffic.** From no copies at night to as many as the traffic needs, each taking up to 1,000 requests at once.
- **Nothing to keep running.** No server, operating system or certificate to look after. Google patches the machines, renews HTTPS and restarts a copy that fails.
- **Releases without downtime.** Each deploy is a revision. Send 5% of the traffic to it first, move everyone when it holds, and go back in one click.
- **A private line to the database.** Cloud Run reaches Cloud SQL over Google's network with IAM, without a public address; backups and point-in-time recovery are on.
- **The rest of Google Cloud.** BigQuery, Pub/Sub, Cloud Storage and Gemini are one permission away, on the same bill.
- **99.95% uptime in writing.** Google's SLA covers every Cloud Run service, with credits when a month falls short.

What works differently:

- A copy that starts after a quiet spell answers in about 0.5 to 2 seconds. Startup CPU boost shortens that; one copy kept warm removes it, for a few dollars a month.
- A request can run for up to 60 minutes. Longer work runs as a Cloud Run job, for up to 24 hours.
- The disk of a copy lives in its memory and goes with it, so files are kept in Cloud Storage (task 2).

## Cost

Assumptions: 500 visitors a day, europe-west1 (Belgium), 5 GB of egress a month.
Prices: Google list prices, europe-west1, read Oct 9, 2026.

| Part | Now | On Google Cloud | Today | Cloud Run |
|---|---|---|---:|---:|
| web | Rails web process, always on | Cloud Run service, request-based, scales to zero | $7.00 | $0.00 |
| worker | Sidekiq worker, always on | Cloud Tasks queue calling the service's /tasks handler | $7.00 | $0.00 |
| Heroku Scheduler | Heroku Scheduler, jobs set in the dashboard | Cloud Scheduler, 1 job, starting a Cloud Run job | $0.00 | $0.00 |
| Postgres | PostgreSQL | Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD | $5.00 | $9.37 |
| Redis | Redis: the Sidekiq queue | Removed: the queue goes to Cloud Tasks | $3.00 | $0.00 |
| Active Storage | Files on Heroku's disk | Cloud Storage bucket, Standard | $0.00 | $0.02 |
| builds | Heroku builds the image | Artifact Registry, the last 3 images | $0.00 | $0.06 |
| variables | 6 variables, 3 of them secrets | Secret Manager, 3 secrets; the rest stay plain | $0.00 | $0.00 |
| egress | Traffic out | Traffic out | $0.00 | $0.60 |
| **Total a month** | | | **$22.00** | **$10.05** |

Today: Heroku's list prices for its smallest paid sizes; the bill answer replaces them.

As the visitors grow, these lines grow with them: web, worker, Active Storage and egress. Egress adds about 0.33 MB a visitor and the stored files about 2 MB a visitor a day. These stay as they are: Heroku Scheduler, Postgres, Redis, builds and variables.

How each Cloud Run number is computed:

- web, Cloud Run CPU, request-based: 36,000 vCPU-s x $0.000024 = $0.86, less $0.86 free = $0.00 (8 requests a visitor, 0.3 s of 1 vCPU and 0.5 GiB each)
- web, Cloud Run memory, request-based: 18,000 GiB-s x $0.0000025 = $0.05, less $0.05 free = $0.00 (8 requests a visitor, 0.3 s of 1 vCPU and 0.5 GiB each)
- web, Cloud Run requests: 120,000 requests x $0.0000004 = $0.05, less $0.05 free = $0.00 (8 requests a visitor)
- worker, Cloud Run CPU, request-based: 3,750 vCPU-s x $0.000024 = $0.09, less $0.09 free = $0.00 (a job for every second visitor, 0.5 s of 1 vCPU each, on the service)
- worker, Cloud Run memory, request-based: 1,875 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (the same jobs at 0.5 GiB)
- worker, Cloud Run requests: 7,500 requests x $0.0000004 = $0.00, less $0.00 free = $0.00 (one request a job)
- worker, Cloud Tasks operations: 7,500 operations x $0.0000004 = $0.00, less $0.00 free = $0.00 (one task a job)
- Heroku Scheduler, Cloud Scheduler jobs: 1 jobs x $0.10 = $0.10, less $0.10 free = $0.00 (the times are not in the repository; taken as one run a day)
- Heroku Scheduler, Cloud Run jobs CPU: 300 vCPU-s x $0.000018 = $0.01, less $0.01 free = $0.00 (30 job runs a month, each taken as 10 s at 1 vCPU)
- Heroku Scheduler, Cloud Run jobs memory: 150 GiB-s x $0.000002 = $0.00, less $0.00 free = $0.00 (the same runs at 0.5 GiB)
- Postgres, Cloud SQL db-f1-micro: 730 hours x $0.0105 = $7.67 (always on, 730 hours)
- Postgres, Cloud SQL SSD storage: 10 GB-months x $0.17 = $1.70 (10 GB, Cloud SQL's smallest disk)
- Active Storage, Cloud Storage, Standard: 1 GB-months x $0.02 = $0.02 (1 GB of files, taken as the size)
- Active Storage, Cloud Storage writes, class A: 2,000 writes x $0.000005 = $0.01, less $0.01 free = $0.00 (about 2,000 files written a month, taken as the count)
- builds, Artifact Registry storage: 1.05 GB-months x $0.10 = $0.11, less $0.05 free = $0.06 (3 images of 350 MB)
- variables, Secret Manager active versions: 3 versions x $0.06 = $0.18, less $0.18 free = $0.00 (3 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Now and after

- web: Rails web process, always on -> Cloud Run service, request-based, scales to zero
- worker: Sidekiq worker, always on -> Cloud Tasks queue calling the service's /tasks handler
- Heroku Scheduler: Heroku Scheduler, jobs set in the dashboard -> Cloud Scheduler, 1 job, starting a Cloud Run job
- Postgres: PostgreSQL -> Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD
- ~~Redis: Redis: the Sidekiq queue~~ -> removed: the queue goes to Cloud Tasks
- Active Storage: Files on Heroku's disk -> Cloud Storage bucket, Standard
- builds: Heroku builds the image -> Artifact Registry, the last 3 images
- variables: 6 variables, 3 of them secrets -> Secret Manager, 3 secrets; the rest stay plain

Read from the repository: Ruby 3.3, Rails 7, Puma 6, Sidekiq 7, PostgreSQL, Redis, app.json. The container: Ruby 3.3 container running Rails; Buildpacks or your Dockerfile, on ruby:3.3-slim.

## What needs a change

### Would not work on Cloud Run (2)

**1. Files written to local disk**

`config/environments/production.rb:3`: `config.active_storage.service = :local`

Why: An instance's disk lives in its memory: a file counts against the memory limit, is gone when the instance stops, and other instances never see it, so an upload saved on one is a 404 on the next.

Instead: Active Storage on a Cloud Storage bucket (the GCS service in storage.yml), with signed URLs; other writes go to the bucket too.

Read: https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage

Task: 2. Files to a Cloud Storage bucket

**2. A worker waits on a Redis queue**

`config/environments/production.rb:4`: `config.active_job.queue_adapter = :sidekiq`

Why: Cloud Run gives an instance CPU only while it answers a request, so a worker waiting on a queue gets no CPU between requests and its jobs stall. With CPU always allocated it runs, and bills every second whether there is work or not.

Instead: Cloud Tasks: each job becomes a POST to /tasks/<name> on the service, through an Active Job adapter for Cloud Tasks or a task created where perform_later was called. Retries move to the queue.

Read: https://exodus.elim.io/blog/background-workers-with-cloud-tasks

Task: 3. Worker: Active Job to a Cloud Tasks handler

### Worth knowing (2)

**3. Migrations run in a release step**

`Procfile:3`: `release: bin/rails db:migrate`

Why: Cloud Run has no release phase, and a migration in the container's start command runs once per instance, several at the same time.

Instead: A Cloud Run job runs the migrations with the new image, and the deploy waits for it before it sends traffic to the new revision.

Read: https://exodus.elim.io/blog/database-migrations-as-cloud-run-jobs

Task: 4. Migrations as a Cloud Run job before each deploy

**4. Heroku Scheduler runs jobs the repository does not list**

`app.json:7`: `"scheduler:standard"`

Why: Heroku Scheduler is an add-on that stays behind, and its jobs and their times live only in the Heroku dashboard.

Instead: Copy each job from the dashboard (heroku addons:open scheduler) into Cloud Scheduler, which starts it as a Cloud Run job.

Read: https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler

Task: 5. Heroku Scheduler jobs to Cloud Scheduler

Looked for and not found: work after the response, in-process timers, sessions in memory, sign-in state in memory, sessions kept in Redis, long-lived connections, background threads, SQLite or files used as a database.

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Dockerfile and a health check route: A Ruby 3.3 image that listens on $PORT; GET /healthz answers 200 once Postgres answers. | - | S | To do |
| 2 | Files to a Cloud Storage bucket: Write to a bucket instead of the disk; for uploads, sign a PUT URL so the browser sends the file straight to the bucket and the service stores only the key. Copy the existing files over before the switch. | Finding 1 | M | To do |
| 3 | Worker: Active Job to a Cloud Tasks handler: Point Active Job at Cloud Tasks (or create a task where perform_later was called), add the /tasks route that runs the job, check the OIDC token, then drop the worker process and Redis. | Finding 2 | L | To do |
| 4 | Migrations as a Cloud Run job before each deploy: Create a Cloud Run job with the same image and the migrate command, and run it in the deploy before the new revision takes traffic. | Finding 3 | S | To do |
| 5 | Heroku Scheduler jobs to Cloud Scheduler: List the jobs and their times from the dashboard, create a Cloud Run job per command and a Cloud Scheduler job per time. | Finding 4 | S | To do |
| 6 | Infrastructure: service, Cloud SQL, bucket, queue, scheduler: Max 3 instances and a database pool of 5 a copy, because db-f1-micro takes 25 connections. | - | M | To do |
| 7 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 8 | Data: copy Postgres and Active Storage and compare: pg_dump into Cloud SQL, then row counts and checksums per table; the files with gcloud storage rsync. | - | M | To do |
| 9 | Switch the domain and stop Heroku: Point the domains at Cloud Run in a quiet hour; Heroku stays stopped, not deleted, for a week. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Prepare: Dockerfile and a health check route.
2. Change the code: Files to a Cloud Storage bucket; Worker: Active Job to a Cloud Tasks handler.
3. Stand up on Google Cloud: Migrations as a Cloud Run job before each deploy; Heroku Scheduler jobs to Cloud Scheduler; Infrastructure: service, Cloud SQL, bucket, queue, scheduler; Keyless deploys from CI.
4. Cut over: Data: copy Postgres and Active Storage and compare; Switch the domain and stop Heroku.

## Questions

15 questions: 0 found by your agent, 0 answered by you, 15 open.

1. How many people use the app a day? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the web line.
2. What does Heroku bill you a month, per service? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the Today column.
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

29 findings: 4 processes, 3 data, 3 build, settings and traffic, 7 stack, 4 patterns found, 8 looked for, not found.

### Processes (4)

- web: Rails web process, always on
- worker: Sidekiq worker, always on
- Heroku Scheduler: Heroku Scheduler, jobs set in the dashboard
- release: bin/rails db:migrate

### Data (3)

- Postgres: PostgreSQL
- Redis: Redis: the Sidekiq queue
- Active Storage: Files on Heroku's disk

### Build, settings and traffic (3)

- builds: Heroku builds the image
- variables: 6 variables, 3 of them secrets
- egress: Traffic out

### Stack (7)

- Ruby 3.3
- Rails 7
- Puma 6
- Sidekiq 7
- PostgreSQL
- Redis
- app.json

### Patterns found (4)

- Files written to local disk (config/environments/production.rb:3)
- A worker waits on a Redis queue (config/environments/production.rb:4)
- Migrations run in a release step (Procfile:3)
- Heroku Scheduler runs jobs the repository does not list (app.json:7)

### Looked for, not found (8)

- No work after the response
- No in-process timers
- No sessions in memory
- No sign-in state in memory
- No sessions kept in Redis
- No long-lived connections
- No background threads
- No SQLite or files used as a database

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/heroku-to-cloud-run (cited in Now and after)
- https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage (cited in Finding 1)
- https://exodus.elim.io/blog/background-workers-with-cloud-tasks (cited in Finding 2)
- https://exodus.elim.io/blog/database-migrations-as-cloud-run-jobs (cited in Finding 3)
- https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler (cited in Finding 4)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Cloud Tasks pricing: https://cloud.google.com/tasks/pricing
- Cloud Scheduler pricing: https://cloud.google.com/scheduler/pricing
- Cloud SQL pricing: https://cloud.google.com/sql/pricing
- Cloud Storage pricing: https://cloud.google.com/storage/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Secret Manager pricing: https://cloud.google.com/secret-manager/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this report was made: exodus 0.1.0 read 13 files of rails-heroku on your machine and found 29 findings (6.7 KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.
