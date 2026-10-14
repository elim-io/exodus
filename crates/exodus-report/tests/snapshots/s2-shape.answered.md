# s2-shape: report 1

From Railway to Google Cloud Run. Report 1 of 1, Oct 11, 2026. Made by exodus 0.1.0 with Claude Code from 23 findings; no code was uploaded.

Built on this machine by `exodus report`; it has no share link.

## Our understanding

Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found.

Today, as the audit read it:

- Runs on: Railway
- Processes: static site (Caddy web process, always on)
- Outside the server, not moved: functions/ (Firebase Functions 2nd gen, already on Cloud Run functions); Firestore (functions/index.js); ios/ (mobile app); tools/ (build and tool code); website/build.py (build and tool code); website/start/ (browser code)

## Verdict

**You can move s2-shape to Cloud Run as it is.** The audit found nothing that breaks or bills while idle on Cloud Run.

Today $5.00 a month on Railway (from your bill), $0.63 on Cloud Run: saves $4.37 a month, $52.44 a year.

Confidence: lower. 10 of 14 questions open (see Questions).

## What s2-shape gains on Cloud Run

Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make s2-shape faster to scale and simpler to run.

A weekday of s2-shape:

- On Railway: 1 process billed every hour: 24 process-hours a day.
- On Cloud Run: CPU only while requests and jobs run, about 0.2 CPU-hours a day.

Railway may sleep idle services on some plans; this report counts the processes found as running every hour. Cloud Run's hours are computed from this report's numbers.

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

Assumptions: 300 visitors a day, europe-west1 (Belgium), 5 GB of egress a month.
Prices: Google list prices, europe-west1, read Oct 9, 2026.

| Part | Now | On Google Cloud | Today | Cloud Run |
|---|---|---|---:|---:|
| static site | Caddy web process, always on | Cloud Run service, request-based, scales to zero | - | $0.00 |
| builds | Railway builds the image | Artifact Registry, the last 3 images | - | $0.03 |
| variables | 0 variables, 0 of them secrets | No secrets; the variables stay plain | - | $0.00 |
| egress | Traffic out | Traffic out | - | $0.60 |
| bill | Railway's bill, one total | Priced part by part above | $5.00 | - |
| **Total a month** | | | **$5.00** | **$0.63** |

Today: your bill as one total, on a line of its own; it is not split by part.

As the visitors grow, these lines grow with them: static site and egress. Egress adds about 0.56 MB a visitor. These stay as they are: builds and variables. In a year at 10% a month: 942 visitors a day, $1.91 a month on Cloud Run.

How each Cloud Run number is computed:

- static site, Cloud Run CPU, request-based: 18,000 vCPU-s x $0.000024 = $0.43, less $0.43 free = $0.00 (8 requests a visitor, 0.25 s of 1 vCPU and 0.5 GiB each)
- static site, Cloud Run memory, request-based: 9,000 GiB-s x $0.0000025 = $0.02, less $0.02 free = $0.00 (8 requests a visitor, 0.25 s of 1 vCPU and 0.5 GiB each)
- static site, Cloud Run requests: 72,000 requests x $0.0000004 = $0.03, less $0.03 free = $0.00 (8 requests a visitor)
- builds, Artifact Registry storage: 0.75 GB-months x $0.10 = $0.08, less $0.05 free = $0.03 (3 images of 250 MB)
- variables, Secret Manager active versions: 0 versions x $0.06 = $0.00 (0 secrets, one version each)
- egress, Internet egress, Premium tier: 5 GB x $0.12 = $0.60 (the egress assumption)

## Now and after

- static site: Caddy web process, always on -> Cloud Run service, request-based, scales to zero
- builds: Railway builds the image -> Artifact Registry, the last 3 images
- variables: 0 variables, 0 of them secrets -> No secrets; the variables stay plain

The container: built from your Dockerfile.

## What needs a change

Nothing found that breaks or bills while idle on Cloud Run.

Looked for and not found: workers waiting on a queue, work after the response, in-process timers, files written to disk, sessions in memory, sign-in state in memory, sessions kept in Redis, long-lived connections, migrations in a release step, background threads, SQLite or files used as a database.

Not on the server, so not counted: ios/ (mobile app), tools/ (build and tool code), website/build.py (build and tool code), website/start/ (browser code).

Stays as it is, so not counted: functions/ (Firebase Functions 2nd gen, already on Cloud Run functions), Firestore (functions/index.js).

## Checklist

| # | Task | Answers | Effort | Status |
|---:|---|---|---|---|
| 1 | Dockerfile and a health check route: An image that listens on $PORT; GET /healthz answers 200. | - | S | To do |
| 2 | Infrastructure: service: Max 3 instances to begin with; the busiest hour decides when to raise it. | - | M | To do |
| 3 | Budget and alerts: A Cloud Billing budget of $5 a month on this project (the Cloud Run estimate rounded up, at least $5), with email alerts to the billing admins at 50%, 90% and 100% of actual spend and at 100% of the forecast: `gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID --display-name="s2-shape on Cloud Run" --budget-amount=5USD --calendar-period=month --filter-projects=projects/PROJECT_ID --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0 --threshold-rule=percent=1.0,basis=forecasted-spend` (the currency is the billing account's own; `--notifications-rule-pubsub-topic` adds a Pub/Sub topic for automated reactions). It needs Billing Account Administrator or Billing Account Costs Manager, or the project's owner for a budget on this project only. It changes billing settings: the agent shows the command, the person approves, and it runs with the person's own gcloud. How budgets work: https://exodus.elim.io/blog/budget-alerts-and-the-billing-export. | - | S | To do |
| 4 | Keyless deploys from CI: The deploy signs in with Workload Identity Federation; no key file anywhere. | - | S | To do |
| 5 | Switch the domain and stop Railway: Point the domains at Cloud Run in a quiet hour; Railway stays stopped, not deleted, for a week. | - | S | To do |

Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.

## Roadmap

1. Prepare: Dockerfile and a health check route.
2. Change the code: nothing to change in the code.
3. Stand up on Google Cloud: Infrastructure: service; Budget and alerts; Keyless deploys from CI.
4. Cut over: Switch the domain and stop Railway.

## Questions

14 questions: 2 found by your agent, 2 answered by you, 10 open.

1. Where does the app run now? Railway, one service built from website/Dockerfile. (Your agent: railway status in the repository's folder.) Affects: Cost: the Today column; Cutover.
2. How many people use the app a day? About 300. (Your agent: Railway's HTTP metrics, the last 30 days.) Affects: Cost: the web line.
3. What does your host bill you a month, per service? $5.00 a month in all, the plan's minimum. (Your answer.) Affects: Cost: the Today column.
4. Where are most of your users? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Region.
5. How much data does the app send out a month? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cost: egress.
6. Do clients keep connections open: WebSockets, server-sent events, long polling? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Findings.
7. When is the busiest hour, and how busy is it? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the instance limit.
8. Which domains point at the app? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: the domains to switch.
9. Can the app be down for a few minutes during the switch? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: the cutover.
10. Where do the tests and the deploys run? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Checklist: keyless deploys.
11. Which outside services does the app call? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Cutover: webhooks and allow lists to update.
12. Which environments does the app run in? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Infrastructure: the services to create.
13. How fast is the traffic growing? About 10% a month. (Your answer.) Affects: Cost: the forecast.
14. Is there a Google Cloud billing account to use? **Not answered.** It lowers confidence; your agent asks again on the next audit. Affects: Roadmap: phase 3 needs it.

## Everything the audit sent

23 findings: 1 processes, 3 build, settings and traffic, 11 looked for, not found, 4 not on the server, not counted, 2 stays as it is, not counted, 2 read by your agent.

### Processes (1)

- static site: Caddy web process, always on

### Build, settings and traffic (3)

- builds: Railway builds the image
- variables: 0 variables, 0 of them secrets
- egress: Traffic out

### Looked for, not found (11)

- No workers waiting on a queue
- No work after the response
- No in-process timers
- No files written to disk
- No sessions in memory
- No sign-in state in memory
- No sessions kept in Redis
- No long-lived connections
- No migrations in a release step
- No background threads
- No SQLite or files used as a database

### Not on the server, not counted (4)

- ios/ (mobile app)
- tools/ (build and tool code)
- website/build.py (build and tool code)
- website/start/ (browser code)

### Stays as it is, not counted (2)

- functions/ (Firebase Functions 2nd gen, already on Cloud Run functions)
- Firestore (functions/index.js)

### Read by your agent (2)

- Where does the app run now? Railway, one service built from website/Dockerfile (from railway status in the repository's folder)
- How many people use the app a day? About 300 (from Railway's HTTP metrics, the last 30 days)

## Sources

Exodus knowledge base:

- https://exodus.elim.io/blog/what-cloud-run-costs (cited in Cost)
- https://exodus.elim.io/blog/railway-or-render-to-cloud-run (cited in Now and after)

Google's price lists, read Oct 9, 2026:

- Cloud Run pricing: https://cloud.google.com/run/pricing
- Artifact Registry pricing: https://cloud.google.com/artifact-registry/pricing
- Network egress pricing: https://cloud.google.com/vpc/network-pricing

How this report was made: exodus 0.1.0 and Claude Code read 19 files of s2-shape on your machine and found 23 findings (4.5 KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.
