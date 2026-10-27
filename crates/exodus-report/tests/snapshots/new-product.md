# class-booking: plan 1

A new product on Google Cloud Run. Plan 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 with Codex from 14 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

**The idea.** class-booking is a product for small fitness studios: a studio publishes its classes, and its clients book a place and pay by card. Most users are in Spain and Portugal, about 300 people a day at launch.

**Where it starts.** Nothing is built yet. The person has chosen Rust with Axum for the API and React for the pages, and expects about 20% more visitors a month in the first year.

**What we will build.** We will build one Cloud Run service for the API and the pages, keep bookings in Cloud SQL for PostgreSQL and class photos in Cloud Storage, and send reminders from a Cloud Run job that Cloud Scheduler starts every 15 minutes. Payment webhooks are checked and answered at once. Nothing runs while nobody uses the product, so the first months cost little.

The parts, as planned:

- Processes: api and app; reminders
- Data: Postgres; photos

## Summary

**class-booking fits Cloud Run at about $9.99 a month at launch.**

At launch $9.99 a month at 300 visitors a day. In a year $14.96 a month at 2,675 a day, at 20% growth a month. The free tier covers $0.50 a month at launch.

Confidence: high. All 6 questions answered.

## What class-booking gains on Cloud Run

Cloud Run bills for the seconds the code works, so a product built from parts that hold no state costs little while it is small and grows without a rebuild. The decisions in this plan are those parts.

A weekday of class-booking:

- On an always-on server: billed every hour, 24 hours a day.
- On Cloud Run: CPU only while requests and jobs run, under 0.1 CPU-hours a day.

A server you rent bills every hour, busy or not. Cloud Run's hours are computed from this report's numbers.

- **Scales with the traffic.** From no copies at night to as many as the traffic needs, each taking up to 1,000 requests at once.
- **Nothing to keep running.** No server, operating system or certificate to look after. Google patches the machines, renews HTTPS and restarts a copy that fails.
- **Releases without downtime.** Each deploy is a revision. Send 5% of the traffic to it first, move everyone when it holds, and go back in one click.
- **A private line to the database.** Cloud Run reaches Cloud SQL over Google's network with IAM, without a public address; backups and point-in-time recovery are on.
- **The rest of Google Cloud.** BigQuery, Pub/Sub, Cloud Storage and Gemini are one permission away, on the same bill.
- **99.95% uptime in writing.** Google's SLA covers every Cloud Run service, with credits when a month falls short.

What works differently:

- A copy that starts after a quiet spell answers in about 0.5 to 2 seconds. Startup CPU boost shortens that; one copy kept warm removes it, for a few dollars a month.
- A request can run for up to 60 minutes. Longer work runs as a Cloud Run job, for up to 24 hours.
- The disk of a copy lives in its memory and goes with it, so files are kept in Cloud Storage.

## Cost

Assumptions: 300 visitors a day, europe-west1 (Belgium), 5 GB of egress a month.
Prices: Google list prices, europe-west1, read Oct 9, 2026.

| Part | Now | On Google Cloud | Today | Cloud Run |
|---|---|---|---:|---:|
| api and app | Planned | Cloud Run service, request-based, scales to zero | - | $0.00 |
| reminders | Planned | Cloud Scheduler, 1 job, calling the service | - | $0.00 |
| Postgres | Planned | Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD | - | $9.37 |
| photos | Planned | Cloud Storage bucket, Standard | - | $0.02 |
| builds | Planned | Artifact Registry, the last 3 images | - | $0.00 |
| variables | Planned | Secret Manager, 5 secrets | - | $0.00 |
| egress | Planned | Traffic out | - | $0.60 |
| **Total a month** | | | **-** | **$9.99** |

Today: nothing, the product is not built yet.

As the visitors grow, these lines grow with them: api and app, photos and egress. Egress adds about 0.56 MB a visitor and the stored files about 3.33 MB a visitor a day. These stay as they are: reminders, Postgres, builds and variables.

How each Cloud Run number is computed:

- api and app, Cloud Run CPU, request-based: 2,700 vCPU-s x $0.000024 = $0.06, less $0.06 free = $0.00 (6 requests a visitor, 0.05 s of 1 vCPU and 0.5 GiB each)
- api and app, Cloud Run memory, request-based: 1,350 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (6 requests a visitor, 0.05 s of 1 vCPU and 0.5 GiB each)
- api and app, Cloud Run requests: 54,000 requests x $0.0000004 = $0.02, less $0.02 free = $0.00 (6 requests a visitor)
- reminders, Cloud Scheduler jobs: 1 jobs x $0.10 = $0.10, less $0.10 free = $0.00 (*/15 * * * *)
- reminders, Cloud Run CPU, request-based: 144 vCPU-s x $0.000024 = $0.00, less $0.00 free = $0.00 (2,880 passes a month called on the service, 0.05 s of 1 vCPU each, as one of its requests)
- reminders, Cloud Run memory, request-based: 72 GiB-s x $0.0000025 = $0.00, less $0.00 free = $0.00 (the same passes at 0.5 GiB)
- reminders, Cloud Run requests: 2,880 requests x $0.0000004 = $0.00, less $0.00 free = $0.00 (one request a pass)
- Postgres, Cloud SQL db-f1-micro: 730 hours x $0.0105 = $7.67 (always on, 730 hours)
- Postgres, Cloud SQL SSD storage: 10 GB-months x $0.17 = $1.70 (10 GB, Cloud SQL's smallest disk)
- photos, Cloud Storage, Standard: 1 GB-months x $0.02 = $0.02 (1 GB of files)
- photos, Cloud Storage writes, class A: 2,000 writes x $0.000005 = $0.01, less $0.01 free = $0.00 (about 2,000 files written a month, taken as the count)
- builds, Artifact Registry storage: 0.09 GB-months x $0.10 = $0.01, less $0.01 free = $0.00 (3 images of 30 MB)
- variables, Secret Manager active versions: 5 versions x $0.06 = $0.30, less $0.30 free = $0.00 (5 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Architecture

- api and app: Cloud Run service, request-based, scales to zero
- reminders: Cloud Scheduler, 1 job, calling the service
- Postgres: Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD
- photos: Cloud Storage bucket, Standard
- builds: Artifact Registry, the last 3 images
- variables: Secret Manager, 5 secrets

Planned stack: Rust 1.90, Axum 0.8, React 19, PostgreSQL 16. The container: Rust 1.90 container running Axum; Your Dockerfile, on debian slim or distroless.

## Design decisions

### How it is built for Cloud Run (3)

**1. Reminders run as a job on a schedule**

Decision: `Cloud Scheduler: */15 * * * * -> Cloud Run job "reminders"`

Why: A timer inside the API would run on every copy at once, or on none when nobody calls.

How: Cloud Scheduler starts a Cloud Run job every 15 minutes; it sends what is due and exits.

Read: https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler

Task: 5. Reminders run as a job on a schedule

**2. Payment webhooks answer at once; the work goes to a task**

Decision: `POST /payments/webhook -> 200, then Cloud Tasks: POST /tasks/payment`

Why: A payment provider retries a webhook that answers late, and a copy may stop as soon as it has answered.

How: The handler checks the signature, queues a Cloud Task and answers 200; the task does the booking.

Read: https://exodus.elim.io/blog/background-workers-with-cloud-tasks

Task: 6. Payment webhooks answer at once; the work goes to a task

**3. Sessions in signed cookies**

Decision: `a signed cookie store, the key in Secret Manager`

Why: Copies come and go, so a session kept in one copy's memory is lost on the next request.

How: A signed, encrypted cookie holds the session; no Redis to pay for.

Read: https://exodus.elim.io/blog/sessions-without-redis

Task: 7. Sessions in signed cookies

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Repository for class-booking: One repository; GET /healthz answers 200 once Postgres answers. | - | S | To do |
| 2 | Dockerfile: Your Dockerfile, on debian slim or distroless. | - | S | To do |
| 3 | Schema and migrations as a Cloud Run job: Migrations run once per deploy as a job, never from a serving copy. | - | M | To do |
| 4 | The core of class-booking: The main screens and the API behind them. | - | L | To do |
| 5 | Reminders run as a job on a schedule: Cloud Scheduler starts a Cloud Run job every 15 minutes; it sends what is due and exits. | Finding 1 | M | To do |
| 6 | Payment webhooks answer at once; the work goes to a task: The handler checks the signature, queues a Cloud Task and answers 200; the task does the booking. | Finding 2 | M | To do |
| 7 | Sessions in signed cookies: A signed, encrypted cookie holds the session; no Redis to pay for. | Finding 3 | M | To do |
| 8 | Infrastructure: service, Cloud SQL, bucket, scheduler: Max 3 instances and a database pool of 5 a copy, because db-f1-micro takes 25 connections. This plan deploys from the repository with gcloud, so the project needs a billing account: the Starter Tier takes only apps published from Google AI Studio, and Cloud Scheduler, Cloud Storage are outside its four products. A person who never had a billing account starts on Google's Free Trial: $300 of credit for 90 days, nothing billed until they upgrade to a paid account (docs.cloud.google.com/free/docs/free-cloud-features). Creating or linking the account is the person's step: ask with `exodus need`. | - | M | To do |
| 9 | Budget and alerts: A Cloud Billing budget of $10 a month on this project (the Cloud Run estimate rounded up, at least $5), with email alerts to the billing admins at 50%, 90% and 100% of actual spend and at 100% of the forecast: `gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID --display-name="class-booking on Cloud Run" --budget-amount=10USD --calendar-period=month --filter-projects=projects/PROJECT_ID --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0 --threshold-rule=percent=1.0,basis=forecasted-spend` (the currency is the billing account's own; `--notifications-rule-pubsub-topic` adds a Pub/Sub topic for automated reactions). It needs Billing Account Administrator or Billing Account Costs Manager, or the project's owner for a budget on this project only. It changes billing settings: the agent shows the command, the person approves, and it runs with the person's own gcloud. How budgets work: https://exodus.elim.io/blog/budget-alerts-and-the-billing-export. Then a spend cap of $20 a month, a second budget that stops Cloud Run instead of only warning: in the console, Billing, Budgets & alerts, Create new budget, Spend cap enforcement, this project, the service Cloud Run, the amount $20 (or the Cloud Billing Budget API: `spendCap.inputState` CONFIGURED, the service `services/152E-C115-5142`, `creditTypesTreatment` EXCLUDE_ALL_CREDITS, thresholds at 50%, 80% and 100%, the full request on Google's page below). Past the cap, Cloud Run in this project pauses until the 1st of the next month or until the person lifts the cap: services answer 5xx, jobs and worker pools stop, nothing is deleted. It counts gross list-price costs before savings and credits and fires before the bill shows them, so it sits at twice the budget, at least $10, not at the estimate. It is not instant: what runs before it fires is billed as usual. Spend caps are monthly, for one project and one service; alerts go to the billing admins and project owners at 50%, 80% and 100%. It needs `billing.budgets.configureSpendCap` on the billing account, or `billing.resourcebudgets.configureSpendCap` on the project. Turning the app off at a ceiling is the person's call: the agent explains it, shows the steps and waits for a yes or no. How spend caps work: https://docs.cloud.google.com/billing/docs/how-to/budgets-spend-caps. | - | S | To do |
| 10 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 11 | Domain and launch: The domain on Cloud Run, then the first users. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Scaffold: Repository for class-booking; Dockerfile.
2. Build the core: Schema and migrations as a Cloud Run job; The core of class-booking; Reminders run as a job on a schedule; Payment webhooks answer at once; the work goes to a task; Sessions in signed cookies.
3. Stand up on Google Cloud: Infrastructure: service, Cloud SQL, bucket, scheduler; Budget and alerts; Keyless deploys from CI.
4. Launch: Domain and launch.

## Questions

6 questions: 0 found by your agent, 6 answered by you, 0 open.

1. What will the product do, in a sentence? Studios publish classes; their clients book a place and pay by card. (Your answer.) Affects: Architecture.
2. Who uses it, and from where? Studio owners and their clients, mostly in Spain and Portugal. (Your answer.) Affects: Region.
3. How many people a day at launch? About 300 a day. (Your answer.) Affects: Cost: the web line.
4. How fast should it grow in the first year? About 20% a month, if the first studios stay. (Your answer.) Affects: Cost: the forecast.
5. Does it take payments? Yes, by card. (Your answer.) Affects: Architecture: the webhook and a task.
6. How much data will it send out a month? 5 GB at launch: pages and photos. (Your answer.) Affects: Cost: egress.

## What the plan rests on

14 findings: 2 processes, 2 data, 3 build, settings and traffic, 4 stack, 3 design decisions.

### Processes (2)

- api and app: Cloud Run service, request-based, scales to zero
- reminders: Cloud Scheduler, 1 job, calling the service

### Data (2)

- Postgres: Cloud SQL for PostgreSQL, db-f1-micro, 10 GB SSD
- photos: Cloud Storage bucket, Standard

### Build, settings and traffic (3)

- builds: Artifact Registry, the last 3 images
- variables: Secret Manager, 5 secrets
- egress: Traffic out

### Stack (4)

- Rust 1.90
- Axum 0.8
- React 19
- PostgreSQL 16

### Design decisions (3)

- Reminders run as a job on a schedule
- Payment webhooks answer at once; the work goes to a task
- Sessions in signed cookies

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/planning-a-new-product-for-cloud-run (cited in Now and after)
- https://exodus.elim.io/blog/cron-and-timers-with-cloud-scheduler (cited in Finding 1)
- https://exodus.elim.io/blog/background-workers-with-cloud-tasks (cited in Finding 2)
- https://exodus.elim.io/blog/sessions-without-redis (cited in Finding 3)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Cloud Scheduler pricing: https://cloud.google.com/scheduler/pricing
- Cloud SQL pricing: https://cloud.google.com/sql/pricing
- Cloud Storage pricing: https://cloud.google.com/storage/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Secret Manager pricing: https://cloud.google.com/secret-manager/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this plan was made: Codex asked about the idea with exodus 0.1.0 and sent 14 facts (4.3 KB). The cost is computed by plain code from the price lists above; no model on our side wrote any of this plan.
