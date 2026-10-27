# django-render: report 1

From Render to Google Cloud Run. Report 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 from 28 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found.

Today, as the audit read it:

- Runs on: Render
- Processes: books-web (Django web process, always on); books-worker (Celery worker, always on); books-worker timers (1 schedule inside the books-worker process)
- Data: media (Files on Render's disk); books-redis (Redis: the Celery queue); books-db (PostgreSQL 16)

## Verdict

**You can move django-render to Cloud Run after 3 changes.** The worker, the timers and the files on disk need a change each before Cloud Run; the rest moves as it is.

Today $30.55 a month on Render (estimated from its list prices), $10.00 on Cloud Run: saves $20.55 a month, $246.60 a year.

The 3 changes before the move:

1. A worker waits on a Redis queue (`books/celery.py:8`)
2. Timers inside the process (`books/celery.py:14`)
3. Files written to local disk (`exports/tasks.py:13`)

Confidence: lower. 15 of 15 questions open (see Questions).

## What django-render gains on Cloud Run

Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make django-render faster to scale and simpler to run.

A weekday of django-render:

- On Render: 2 processes billed every hour: 48 process-hours a day.
- On Cloud Run: CPU only while requests and jobs run, about 0.3 CPU-hours a day.

Render spins down only free instances; this app runs on a paid instance, which runs every hour. Cloud Run's hours are computed from this report's numbers.

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
| books-web | Django web process, always on | Cloud Run service, request-based, scales to zero | $7.00 | $0.00 |
| books-worker | Celery worker, always on | Cloud Tasks queue calling the service's /tasks handler | $7.00 | $0.00 |
| books-worker timers | 1 schedule inside the books-worker process | Cloud Scheduler, 1 job, starting a Cloud Run job | $0.00 | $0.00 |
| media | Files on Render's disk | Cloud Storage bucket, Standard | $0.25 | $0.02 |
| books-redis | Redis: the Celery queue | Removed: the queue goes to Cloud Tasks | $10.00 | $0.00 |
| books-db | PostgreSQL 16 | Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD | $6.30 | $9.37 |
| builds | Render builds the image | Artifact Registry, the last 3 images | $0.00 | $0.01 |
| variables | 4 variables, 3 of them secrets | Secret Manager, 3 secrets; the rest stay plain | $0.00 | $0.00 |
| egress | Traffic out | Traffic out | $0.00 | $0.60 |
| **Total a month** | | | **$30.55** | **$10.00** |

Today: Render's list prices for its smallest paid sizes; the bill answer replaces them.

As the visitors grow, these lines grow with them: books-web, books-worker, media and egress. Egress adds about 0.33 MB a visitor and the stored files about 2 MB a visitor a day. These stay as they are: books-worker timers, books-redis, books-db, builds and variables.

How each Cloud Run number is computed:

- books-web, Cloud Run CPU, request-based: 30,000 vCPU-s x $0.000024 = $0.72, less $0.72 free = $0.00 (8 requests a visitor, 0.25 s of 1 vCPU and 0.5 GiB each)
- books-web, Cloud Run memory, request-based: 15,000 GiB-s x $0.0000025 = $0.04, less $0.04 free = $0.00 (8 requests a visitor, 0.25 s of 1 vCPU and 0.5 GiB each)
- books-web, Cloud Run requests: 120,000 requests x $0.0000004 = $0.05, less $0.05 free = $0.00 (8 requests a visitor)
- books-worker, Cloud Run CPU, request-based: 3,750 vCPU-s x $0.000024 = $0.09, less $0.09 free = $0.00 (a job for every second visitor, 0.5 s of 1 vCPU each, on the service)
- books-worker, Cloud Run memory, request-based: 1,875 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (the same jobs at 0.5 GiB)
- books-worker, Cloud Run requests: 7,500 requests x $0.0000004 = $0.00, less $0.00 free = $0.00 (one request a job)
- books-worker, Cloud Tasks operations: 7,500 operations x $0.0000004 = $0.00, less $0.00 free = $0.00 (one task a job)
- books-worker timers, Cloud Scheduler jobs: 1 jobs x $0.10 = $0.10, less $0.10 free = $0.00 (0 2 * * *)
- books-worker timers, Cloud Run jobs CPU: 300 vCPU-s x $0.000018 = $0.01, less $0.01 free = $0.00 (30 job runs a month, each taken as 10 s at 1 vCPU)
- books-worker timers, Cloud Run jobs memory: 150 GiB-s x $0.000002 = $0.00, less $0.00 free = $0.00 (the same runs at 0.5 GiB)
- media, Cloud Storage, Standard: 1 GB-months x $0.02 = $0.02 (1 GB of files, taken as the size)
- media, Cloud Storage writes, class A: 2,000 writes x $0.000005 = $0.01, less $0.01 free = $0.00 (about 2,000 files written a month, taken as the count)
- books-db, Cloud SQL db-f1-micro: 730 hours x $0.0105 = $7.67 (always on, 730 hours)
- books-db, Cloud SQL SSD storage: 10 GB-months x $0.17 = $1.70 (10 GB, Cloud SQL's smallest disk)
- builds, Artifact Registry storage: 0.6 GB-months x $0.10 = $0.06, less $0.05 free = $0.01 (3 images of 200 MB)
- variables, Secret Manager active versions: 3 versions x $0.06 = $0.18, less $0.18 free = $0.00 (3 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Now and after

- books-web: Django web process, always on -> Cloud Run service, request-based, scales to zero
- books-worker: Celery worker, always on -> Cloud Tasks queue calling the service's /tasks handler
- books-worker timers: 1 schedule inside the books-worker process -> Cloud Scheduler, 1 job, starting a Cloud Run job
- media: Files on Render's disk -> Cloud Storage bucket, Standard
- ~~books-redis: Redis: the Celery queue~~ -> removed: the queue goes to Cloud Tasks
- books-db: PostgreSQL 16 -> Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD
- builds: Render builds the image -> Artifact Registry, the last 3 images
- variables: 4 variables, 3 of them secrets -> Secret Manager, 3 secrets; the rest stay plain

Read from the repository: Python 3.12, Django 5, Gunicorn 23, Celery 5, PostgreSQL 16, Redis, render.yaml. The container: Python 3.12 container running Django; Buildpacks or your Dockerfile, on python:3.12-slim.

## What needs a change

### Would not work on Cloud Run (3)

**1. A worker waits on a Redis queue**

`books/celery.py:8`: `app = Celery("books", broker=os.environ["CELERY_BROKER_URL"])`

Why: Cloud Run gives an instance CPU only while it answers a request, so a worker waiting on a queue gets no CPU between requests and its jobs stall. With CPU always allocated it runs, and bills every second whether there is work or not.

Instead: Cloud Tasks: each task becomes a POST to /tasks/<name> on the service, created where the code called .delay() or .enqueue(); a handler does the work and answers 2xx. Retries and rate limits move to the queue.

Read: https://exodus.elim.io/blog/background-workers-with-cloud-tasks

Task: 2. Worker: Celery tasks to a Cloud Tasks handler

**2. Timers inside the process**

`books/celery.py:14`: `"schedule": crontab(hour=2, minute=0),`

Why: A service at zero has no process to run a timer, a running one gets no CPU between requests, and with several instances the timer fires once per instance: late, never, or twice.

Instead: Cloud Scheduler on the same cron lines: short work calls POST /cron/<name> on the service with an OIDC token, long work starts a Cloud Run job. Set each job's time zone; the timer ran on the server's, usually UTC.

Read: https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler

Task: 3. Timers to Cloud Scheduler

**3. Files written to local disk**

`exports/tasks.py:13`: `with open(path, "w", newline="") as f:`

Why: An instance's disk lives in its memory: a file counts against the memory limit, is gone when the instance stops, and other instances never see it, so an upload saved on one is a 404 on the next.

Instead: Write to a Cloud Storage bucket (django-storages' Google Cloud backend for FileField) and hand out signed URLs.

Read: https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage

Task: 4. Files to a Cloud Storage bucket

### Worth knowing (1)

**4. Migrations run in a release step**

`render.yaml:8`: `preDeployCommand: python manage.py migrate`

Why: Cloud Run has no release phase, and a migration in the container's start command runs once per instance, several at the same time.

Instead: A Cloud Run job runs the migrations with the new image, and the deploy waits for it before it sends traffic to the new revision.

Read: https://exodus.elim.io/blog/database-migrations-as-cloud-run-jobs

Task: 5. Migrations as a Cloud Run job before each deploy

Looked for and not found: work after the response, sessions in memory, sign-in state in memory, sessions kept in Redis, long-lived connections, background threads, SQLite or files used as a database.

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Dockerfile and a health check route: A Python 3.12 image that listens on $PORT; GET /healthz answers 200 once books-db answers. | - | S | To do |
| 2 | Worker: Celery tasks to a Cloud Tasks handler: Replace each .delay() with a Cloud Tasks task that POSTs to /tasks/<name>, move the task body into that view, check the OIDC token, then drop the worker process and the broker. | Finding 1 | L | To do |
| 3 | Timers to Cloud Scheduler: Give each schedule an endpoint (POST /cron/<name>, OIDC token checked) or a Cloud Run job, create a Cloud Scheduler job per schedule with its time zone, then remove the timer and its process. | Finding 2 | M | To do |
| 4 | Files to a Cloud Storage bucket: Write to a bucket instead of the disk; for uploads, sign a PUT URL so the browser sends the file straight to the bucket and the service stores only the key. Copy the existing files over before the switch. | Finding 3 | M | To do |
| 5 | Migrations as a Cloud Run job before each deploy: Create a Cloud Run job with the same image and the migrate command, and run it in the deploy before the new revision takes traffic. | Finding 4 | S | To do |
| 6 | Infrastructure: service, Cloud SQL, bucket, queue, scheduler: Max 3 instances and a database pool of 5 a copy, because db-f1-micro takes 25 connections. This plan deploys from the repository with gcloud, so the project needs a billing account: the Starter Tier (docs.cloud.google.com/docs/starter-tier) takes only apps published from Google AI Studio, and Cloud Tasks, Cloud Scheduler, Cloud Storage are outside its four products. A person who never had a billing account starts on Google's Free Trial: $300 of credit for 90 days, nothing billed until they upgrade to a paid account (docs.cloud.google.com/free/docs/free-cloud-features). Creating or linking the account is the person's step: ask with `exodus need`. | - | M | To do |
| 7 | Budget and alerts: A Cloud Billing budget of $10 a month on this project (the Cloud Run estimate rounded up, at least $5), with email alerts to the billing admins at 50%, 90% and 100% of actual spend and at 100% of the forecast: `gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID --display-name="django-render on Cloud Run" --budget-amount=10USD --calendar-period=month --filter-projects=projects/PROJECT_ID --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0 --threshold-rule=percent=1.0,basis=forecasted-spend` (the currency is the billing account's own; `--notifications-rule-pubsub-topic` adds a Pub/Sub topic for automated reactions). It needs Billing Account Administrator or Billing Account Costs Manager, or the project's owner for a budget on this project only. It changes billing settings: the agent shows the command, the person approves, and it runs with the person's own gcloud. How budgets work: https://exodus.elim.io/blog/budget-alerts-and-the-billing-export. Then a spend cap of $20 a month, a second budget that stops Cloud Run instead of only warning: in the console, Billing, Budgets & alerts, Create new budget, Spend cap enforcement, this project, the service Cloud Run, the amount $20 (or the Cloud Billing Budget API: `spendCap.inputState` CONFIGURED, the service `services/152E-C115-5142`, `creditTypesTreatment` EXCLUDE_ALL_CREDITS, thresholds at 50%, 80% and 100%, the full request on Google's page below). Past the cap, Cloud Run in this project pauses until the 1st of the next month or until the person lifts the cap: services answer 5xx, jobs and worker pools stop, nothing is deleted. It counts gross list-price costs before savings and credits and fires before the bill shows them, so it sits at twice the budget, at least $10, not at the estimate. It is not instant: what runs before it fires is billed as usual. Spend caps are monthly, for one project and one service; alerts go to the billing admins and project owners at 50%, 80% and 100%. It needs `billing.budgets.configureSpendCap` on the billing account, or `billing.resourcebudgets.configureSpendCap` on the project. Turning the app off at a ceiling is the person's call: the agent explains it, shows the steps and waits for a yes or no. How spend caps work: https://docs.cloud.google.com/billing/docs/how-to/budgets-spend-caps. | - | S | To do |
| 8 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 9 | Data: copy media and books-db and compare: the files with gcloud storage rsync; pg_dump into Cloud SQL, then row counts and checksums per table. | - | M | To do |
| 10 | Switch the domain and stop Render: Point the domains at Cloud Run in a quiet hour; Render stays stopped, not deleted, for a week. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Prepare: Dockerfile and a health check route.
2. Change the code: Worker: Celery tasks to a Cloud Tasks handler; Timers to Cloud Scheduler; Files to a Cloud Storage bucket.
3. Stand up on Google Cloud: Migrations as a Cloud Run job before each deploy; Infrastructure: service, Cloud SQL, bucket, queue, scheduler; Budget and alerts; Keyless deploys from CI.
4. Cut over: Data: copy media and books-db and compare; Switch the domain and stop Render.

## Questions

15 questions: 0 found by your agent, 0 answered by you, 15 open.

1. How many people use the app a day? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the web line.
2. What does Render bill you a month, per service? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: the Today column.
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

28 findings: 4 processes, 3 data, 3 build, settings and traffic, 7 stack, 4 patterns found, 7 looked for, not found.

### Processes (4)

- books-web: Django web process, always on
- books-worker: Celery worker, always on
- books-worker timers: 1 schedule inside the books-worker process
- books-web pre-deploy: python manage.py migrate

### Data (3)

- media: Files on Render's disk
- books-redis: Redis: the Celery queue
- books-db: PostgreSQL 16

### Build, settings and traffic (3)

- builds: Render builds the image
- variables: 4 variables, 3 of them secrets
- egress: Traffic out

### Stack (7)

- Python 3.12
- Django 5
- Gunicorn 23
- Celery 5
- PostgreSQL 16
- Redis
- render.yaml

### Patterns found (4)

- A worker waits on a Redis queue (books/celery.py:8)
- Timers inside the process (books/celery.py:14)
- Files written to local disk (exports/tasks.py:13)
- Migrations run in a release step (render.yaml:8)

### Looked for, not found (7)

- No work after the response
- No sessions in memory
- No sign-in state in memory
- No sessions kept in Redis
- No long-lived connections
- No background threads
- No SQLite or files used as a database

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/railway-or-render-to-cloud-run (cited in Now and after)
- https://exodus.elim.io/blog/background-workers-with-cloud-tasks (cited in Finding 1)
- https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler (cited in Finding 2)
- https://exodus.elim.io/blog/files-and-uploads-with-cloud-storage (cited in Finding 3)
- https://exodus.elim.io/blog/database-migrations-as-cloud-run-jobs (cited in Finding 4)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Cloud Tasks pricing: https://cloud.google.com/tasks/pricing
- Cloud Scheduler pricing: https://cloud.google.com/scheduler/pricing
- Cloud Storage pricing: https://cloud.google.com/storage/pricing
- Cloud SQL pricing: https://cloud.google.com/sql/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Secret Manager pricing: https://cloud.google.com/secret-manager/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this report was made: exodus 0.1.0 read 12 files of django-render on your machine and found 28 findings (6.9 KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.
