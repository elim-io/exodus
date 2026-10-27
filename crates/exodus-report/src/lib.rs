//! Findings, answers and the prices into a report (`report.json`, architecture.md §6.3) and its Markdown. No
//! network: the CLI builds a report offline with this crate, and the platform builds the same bytes from the same
//! findings, answers and previous report.
// Note: one function per block of the report document (verdict, tasks, questions, inventory, since); each is
// what the cabinet's mock.ts and report.ts computed in the browser

mod markdown;
mod parts;

use std::collections::{BTreeMap, HashSet};

use exodus_audit::rules::RuleFile;
use exodus_audit::split_clean;
use exodus_schema::{
    Answer, Assumptions, AuditSent, Cost, Day, Findings, GoogleSource, Group, KbSource, Line, Part, PlatformPrices,
    Prices, Report, ReportFinding, ReportPhase, ReportProject, ReportQuestion, ReportTask, Since, Sources, Verdict,
};

pub use markdown::{markdown, usd};
use parts::{Today, count, join, num, tally};

/// The site the knowledge base is published on; a report links its articles there.
pub const SITE: &str = "https://exodus.elim.io";
/// The cabinet, where a shared report opens at `/r/<token>`.
pub const APP: &str = "https://app.exodus.elim.io";

/// The defaults of A14 while a question has no value: visitors a day, GB of egress a month, the region.
pub const VISITS: u64 = 500;
pub const EGRESS_GB: f64 = 5.0;
pub const REGION: &str = "europe-west1";
/// The answers that set the forecast's growth of egress (MB a visit) and of the stored files (MB a visitor a day).
pub const EGRESS_RATE: &str = "egressmb";
pub const FILES_RATE: &str = "filesmb";

const PHASES: [&str; 4] = ["Prepare", "Change the code", "Stand up on Google Cloud", "Cut over"];
const PHASES_NEW: [&str; 4] = ["Scaffold", "Build the core", "Stand up on Google Cloud", "Launch"];
const SERVERLESS: [&str; 5] = ["Vercel", "Netlify", "Cloudflare", "AWS Lambda", "Deno Deploy"];

/// What a report is built from.
pub struct Input<'a> {
    pub findings: &'a Findings,
    pub answers: &'a [Answer],
    /// The report before this one, when there is one: this one is the next version and says what changed.
    pub prev: Option<&'a Report>,
    /// When it is built, ISO 8601 UTC.
    pub at: &'a str,
    /// The coding agent that ran the audit, when known.
    pub agent: Option<&'a str>,
}

/// Builds the report. An error names an unknown sku or region in the prices.
pub fn build(input: &Input, prices: &Prices, platforms: &PlatformPrices) -> Result<Report, String> {
    // a host no file names is the one the `platform` answer gives
    let named;
    let f = match input
        .answers
        .iter()
        .find(|a| a.id == "platform")
        .and_then(|a| a.value.as_ref()?.as_str())
    {
        Some(name) if input.findings.platform.name == "Unknown" => {
            named = Findings {
                platform: exodus_schema::Platform {
                    name: name.into(),
                    ..input.findings.platform.clone()
                },
                ..input.findings.clone()
            };
            &named
        }
        _ => input.findings,
    };
    let new = f.kind == "new";
    let n = input.prev.map_or(1, |p| p.n + 1);
    let traffic = f.traffic.clone().unwrap_or_else(|| "visitors".into());
    let value = |id: &str| input.answers.iter().find(|a| a.id == id).and_then(|a| a.value.as_ref());
    let rate = |id: &str| value(id).and_then(serde_json::Value::as_f64).filter(|v| *v >= 0.0);

    // A14: the numbers arrive with the answers' values; without one, the default
    let assumptions = Assumptions {
        visits: rate("visits").map_or(VISITS, |v| v.round() as u64),
        // where the users are: `users` when moving an app, `who` when planning a new product (templates/new.md)
        region: ["users", "who"]
            .into_iter()
            .find_map(|id| {
                value(id)
                    .and_then(serde_json::Value::as_str)
                    .filter(|r| prices.regions.contains_key(*r))
            })
            .unwrap_or(REGION)
            .into(),
        egress_gb: rate("egress").unwrap_or(EGRESS_GB),
        egress_mb_per_visit: rate(EGRESS_RATE),
        files_mb_per_visitor: rate(FILES_RATE),
    };
    let growth = value("growth").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let bill: Option<BTreeMap<String, i64>> = value("bill").and_then(|v| {
        let m = v.as_object()?;
        m.iter().map(|(k, c)| Some((k.clone(), c.as_i64()?))).collect()
    });
    let list = platforms.platforms.get(&f.platform.name);
    let (today, basis) = match (&bill, list) {
        _ if new => (Today::Unknown, "unknown"),
        (Some(b), _) => (Today::Bill(b), "bill"),
        (None, Some(l)) => (Today::List(l), "list"),
        (None, None) => (Today::Unknown, "unknown"),
    };

    let open = parts::public_connections(f, value("connections").and_then(serde_json::Value::as_str));
    let mut parts = parts::parts(f, &traffic, &prices.firestore, open.as_deref());
    if let Today::Bill(b) = &today {
        parts.extend(parts::bill_part(f, b));
    }
    let today_total = parts::price_today(&mut parts, f, &today, assumptions.egress_gb);
    let lines = price(prices, parts, &assumptions)?;
    let cloud_run = lines.iter().map(|l| l.cents).sum();
    let mut cost = Cost {
        today: today_total,
        cloud_run,
        lines,
    };

    let rules = exodus_audit::rules::builtin();
    let rule = |p: &exodus_schema::Pattern| -> Option<&RuleFile> {
        let same = |r: &&exodus_audit::rules::Rule| r.file.id == p.rule;
        rules
            .iter()
            .filter(same)
            // one id has a file per language; the text the finding carries names which one
            .find(|r| {
                r.file.title == p.title
                    && r.file.why == p.why
                    && (r.file.fix == p.fix || r.file.fixes.values().any(|f| *f == p.fix))
            })
            .or_else(|| rules.iter().find(same))
            .map(|r| &r.file)
    };

    let mut findings: Vec<ReportFinding> = f
        .patterns
        .iter()
        .enumerate()
        .map(|(i, p)| ReportFinding {
            i: i as u32 + 1,
            id: p.id.clone(),
            kind: p.kind.clone(),
            title: p.title.clone(),
            at: p.at.clone(),
            code: p.code.clone(),
            why: p.why.clone(),
            fix: p.fix.clone(),
            part: p.part.clone(),
            kb: p.kb.clone(),
            subject: p.subject.clone(),
            since: None,
            task: None,
        })
        .collect();
    let name = f.repo.name.clone();
    let runtime = exodus_audit::runtime_of(&f.stack);
    let mut tasks = tasks(f, &cost.lines, &runtime, &name, &mut findings, &rule);
    // numbered in the order the report groups them (breaks, idle, notes), so the verdict's list reads 1, 2, ...
    findings.sort_by_key(|x| ["breaks", "idle"].iter().position(|k| *k == x.kind).unwrap_or(2));
    for (i, x) in findings.iter_mut().enumerate() {
        x.i = i as u32 + 1;
    }

    let verdict = verdict(f, &name, &cost, &findings, &rule);
    let mut questions = questions(f, input.answers, n);
    let open = questions.iter().filter(|q| !q.is_answered).map(|q| q.i).collect();

    // what an earlier report already knew keeps its `since`; what is new is found by this one
    if let Some(prev) = input.prev {
        for l in &mut cost.lines {
            l.part.since = prev
                .cost
                .lines
                .iter()
                .find(|x| x.part.id == l.part.id)
                .map_or(Some(n), |x| x.part.since);
        }
        for x in &mut findings {
            x.since = prev.findings.iter().find(|y| same(y, x)).map_or(Some(n), |y| y.since);
        }
        for t in &mut tasks {
            t.since = prev
                .tasks
                .iter()
                .find(|y| y.title == t.title)
                .map_or(Some(n), |y| y.since);
        }
        for q in &mut questions {
            if let Some(p) = prev.questions.iter().find(|p| p.id == q.id) {
                q.asked = p.asked;
                if q.is_answered && p.is_answered && p.answer == q.answer {
                    q.answered = p.answered;
                }
            }
        }
    }

    let phases = (1..=4u8)
        .map(|phase| ReportPhase {
            phase,
            name: (if new { PHASES_NEW } else { PHASES })[phase as usize - 1].into(),
            tasks: tasks.iter().filter(|t| t.phase == phase).map(|t| t.i).collect(),
            done: 0,
            state: "todo".into(),
        })
        .collect();

    let inventory = inventory(f, &cost.lines, &findings, &questions, new);
    let findings_count = inventory.iter().map(|g| g.items.len() as u32).sum();
    let sources = sources(prices, &cost.lines, &findings, &f.platform.name, new);
    let day = day(f, &cost.lines, list, new);
    let sent = serde_json::to_vec(f).map_or(0, |v| v.len()) + serde_json::to_vec(input.answers).map_or(0, |v| v.len());

    let project = ReportProject {
        id: name.clone(),
        owner: f.repo.owner.clone(),
        name: name.clone(),
        from: if new {
            "New product".into()
        } else {
            f.platform.name.clone()
        },
        kind: f.kind.clone(),
        what: f.what.clone().unwrap_or_else(|| what(f)),
        agent: input.agent.map(Into::into),
        cli: f.cli.clone(),
        traffic,
        stack: f.stack.clone(),
    };
    let clean = split_clean(&f.clean);
    let mut report = Report {
        exodus: exodus_schema::REPORT.into(),
        project,
        n,
        reports: n,
        at: input.at.into(),
        change: String::new(),
        understanding: f.understanding.clone(),
        token: None,
        shared: None,
        archived_at: None,
        latest: n,
        assumptions,
        prices: prices.clone(),
        today_basis: basis.into(),
        growth,
        cost,
        verdict,
        findings,
        clean: clean.looked_for.iter().map(|c| c.to_string()).collect(),
        not_on_server: clean.not_on_server.iter().map(|c| c.to_string()).collect(),
        stays: clean.stays.iter().map(|c| c.to_string()).collect(),
        tasks,
        planned: false,
        done: 0,
        phases,
        questions,
        open,
        inventory,
        findings_count,
        since: None,
        sources,
        runtime,
        day,
        audit: AuditSent {
            files: f.stats.files,
            findings: findings_count,
            kb: (sent as f64 / 1024.0 * 10.0).round() / 10.0,
        },
        plan_from: None,
    };
    report.since = input.prev.map(|p| since(p, &report));
    report.change = match &report.since {
        None => if new { "First plan" } else { "First audit" }.into(),
        Some(s) => s
            .lines
            .first()
            .cloned()
            .unwrap_or_else(|| "Built again; nothing changed".into()),
    };
    Ok(report)
}

/// Prices every part at the assumptions.
fn price(prices: &Prices, parts: Vec<Part>, a: &Assumptions) -> Result<Vec<Line>, String> {
    let uses: Vec<&[exodus_schema::Use]> = parts.iter().map(|p| p.uses.as_slice()).collect();
    let priced = exodus_cost::price(prices, &uses, a)?;
    Ok(parts
        .into_iter()
        .zip(priced)
        .map(|(part, p)| Line {
            part,
            cents: p.cents,
            uses: p.uses,
        })
        .collect())
}

/// The GB of files the report stores in a month, the sum of its file parts.
fn stored_gb(r: &Report) -> f64 {
    r.cost
        .lines
        .iter()
        .filter(|l| l.part.kind == "files")
        .flat_map(|l| &l.part.uses)
        .filter(|u| u.sku == "gcs")
        .map(|u| u.qty)
        .sum()
}

/// The rates the forecast grows egress and the stored files by: MB a visit, and MB a visitor a day. An answer sets
/// one; otherwise it is the report's own, its egress over its visits and its files over its visitors.
pub fn rates(r: &Report) -> (f64, f64) {
    let a = &r.assumptions;
    let v = a.visits.max(1) as f64;
    (
        a.egress_mb_per_visit
            .unwrap_or(a.egress_gb * 1000.0 / (v * exodus_cost::DAYS)),
        a.files_mb_per_visitor.unwrap_or(stored_gb(r) * 1000.0 / v),
    )
}

/// Cloud Run's monthly cost for the next twelve months if the traffic grows by `growth` percent a month; month 0
/// is the report's own. The requests and jobs grow with the visits, egress and the stored files by `rates` for each
/// visit more than the report's; the rest stays. Returns (month, visits a day, cents).
pub fn forecast(r: &Report, growth: f64) -> Vec<(u32, u64, i64)> {
    let a0 = &r.assumptions;
    let (egress, files) = rates(r);
    let gb = stored_gb(r);
    (0..=12)
        .map(|m| {
            let visits = (a0.visits as f64 * (1.0 + growth / 100.0).powi(m as i32)).round() as u64;
            let more = visits as f64 - a0.visits as f64;
            let a = Assumptions {
                visits,
                egress_gb: (a0.egress_gb + egress * more * exodus_cost::DAYS / 1000.0).max(0.0),
                ..a0.clone()
            };
            // every use of a file part (its storage and its writes) grows as its storage does
            let k = if gb > 0.0 {
                (gb + files * more / 1000.0).max(0.0) / gb
            } else {
                1.0
            };
            let grown: Vec<Vec<exodus_schema::Use>> = r
                .cost
                .lines
                .iter()
                .map(|l| {
                    let mut uses = l.part.uses.clone();
                    if l.part.kind == "files" {
                        uses.iter_mut().for_each(|u| u.qty *= k);
                    }
                    uses
                })
                .collect();
            let uses: Vec<&[exodus_schema::Use]> = grown.iter().map(Vec::as_slice).collect();
            let cents = exodus_cost::price(&r.prices, &uses, &a).map_or(0, |p| p.iter().map(|x| x.cents).sum());
            (m, visits, cents)
        })
        .collect()
}

/// The parts whose cost grows with the traffic: those priced per visit, the stored files and egress.
pub fn growing(r: &Report) -> Vec<String> {
    r.cost
        .lines
        .iter()
        .filter(|l| l.part.kind == "files" || l.part.uses.iter().any(|u| u.per.is_some()))
        .map(|l| l.part.name.clone())
        .collect()
}

/// The verdict: a new product's plan; stay when moving costs more and a rule that explains it fired; move as it is
/// when nothing breaks or bills idle; else move after the changes.
fn verdict<'r>(
    f: &Findings,
    name: &str,
    cost: &Cost,
    findings: &[ReportFinding],
    rule: &impl Fn(&exodus_schema::Pattern) -> Option<&'r RuleFile>,
) -> Verdict {
    let v = |kind: &str, label: String, title: String, reason: String, flip: Option<String>| Verdict {
        kind: kind.into(),
        label,
        title,
        reason,
        flip,
    };
    if f.kind == "new" {
        let title = format!(
            "{name} fits Cloud Run at about {} a month at launch",
            usd(cost.cloud_run)
        );
        return v("new", "Plan ready".into(), title, String::new(), None);
    }
    let platform = &f.platform.name;
    if let Some(today) = cost.today.filter(|t| cost.cloud_run > *t)
        && let Some(r) = f.patterns.iter().filter_map(rule).find(|r| r.stay)
    {
        let fill = |s: &str| {
            s.replace("{today}", &usd(today))
                .replace("{cloudRun}", &usd(cost.cloud_run))
        };
        return v(
            "stay",
            format!("Better on {platform} for now"),
            format!("We recommend keeping {name} on {platform} for now"),
            fill(&r.why),
            Some(fill(&r.fix)),
        );
    }
    let changes: Vec<&ReportFinding> = findings.iter().filter(|x| x.kind != "note").collect();
    if changes.is_empty() {
        return v(
            "move",
            "Ready to move".into(),
            format!("You can move {name} to Cloud Run as it is"),
            "The audit found nothing that breaks or bills while idle on Cloud Run.".into(),
            None,
        );
    }
    let subjects = tally(
        &changes
            .iter()
            .map(|x| x.subject.clone().unwrap_or_else(|| x.title.clone()))
            .collect::<Vec<_>>(),
    );
    let many = subjects.len() > 1;
    let what = join(&subjects);
    let n = count(changes.len(), "change");
    v(
        "move-after",
        format!("Ready after {n}"),
        format!("You can move {name} to Cloud Run after {n}"),
        format!(
            "{}{} need{} a change{} before Cloud Run; the rest moves as it is.",
            what[..1].to_uppercase(),
            &what[1..],
            if many { "" } else { "s" },
            if many { " each" } else { "" }
        ),
        None,
    )
}

struct Draft {
    title: String,
    detail: String,
    effort: &'static str,
    phase: u8,
    fixes: Option<String>,
}

fn draft(title: impl Into<String>, detail: impl Into<String>, effort: &'static str, phase: u8) -> Draft {
    Draft {
        title: title.into(),
        detail: detail.into(),
        effort,
        phase,
        fixes: None,
    }
}

/// The checklist: the standard steps of a move (or of a new product) and one task per rule that fired, in phase
/// order. A finding points at the task that fixes it; one task covers a rule's findings in several files.
fn tasks<'r>(
    f: &Findings,
    lines: &[Line],
    runtime: &exodus_schema::Runtime,
    name: &str,
    findings: &mut [ReportFinding],
    rule: &impl Fn(&exodus_schema::Pattern) -> Option<&'r RuleFile>,
) -> Vec<ReportTask> {
    let new = f.kind == "new";
    let has = |kind: &str| lines.iter().any(|l| l.part.kind == kind && l.part.removed.is_none());
    let db = lines.iter().find(|l| l.part.kind == "db").map(|l| l.part.name.clone());
    let image = runtime.name.trim_end_matches(" container");
    // an app that already answers a health route keeps it as the startup probe; one without gets /healthz
    let health = match (f.settings.health.as_deref(), &db) {
        (Some(route), Some(db)) => {
            format!("its health route {route} is the startup probe and answers 200 once {db} answers")
        }
        (Some(route), None) => format!("its health route {route} is the startup probe"),
        (None, Some(db)) => format!("GET /healthz answers 200 once {db} answers"),
        (None, None) => "GET /healthz answers 200".into(),
    };
    let mut drafts = Vec::new();
    if new {
        drafts.push(draft(
            format!("Repository for {name}"),
            format!("One repository; {health}."),
            "S",
            1,
        ));
        drafts.push(draft("Dockerfile", format!("{}.", runtime.build), "S", 1));
        if db.is_some() {
            drafts.push(draft(
                "Schema and migrations as a Cloud Run job",
                "Migrations run once per deploy as a job, never from a serving copy.",
                "M",
                2,
            ));
        }
        drafts.push(draft(
            format!("The core of {name}"),
            "The main screens and the API behind them.",
            "L",
            2,
        ));
    } else {
        let detail = match &f.settings.dockerfile {
            Some(d) => format!("{d} exists: check that it listens on $PORT; {health}."),
            None if image == "Your" => format!("An image that listens on $PORT; {health}."),
            None => format!("A {image} image that listens on $PORT; {health}."),
        };
        let title = if f.settings.health.is_some() {
            "Dockerfile and the health check"
        } else {
            "Dockerfile and a health check route"
        };
        drafts.push(draft(title, detail, "S", 1));
    }

    // one task per rule, from its template; a new product's decisions are their own tasks
    let mut by_rule: Vec<(String, usize)> = Vec::new();
    for (k, p) in f.patterns.iter().enumerate() {
        // a rule's findings in several files share its task; each decision of a new product is its own
        let key = if p.at == "design" { &p.id } else { &p.rule };
        if let Some((_, d)) = by_rule.iter().find(|(r, _)| r == key) {
            findings[k].task = Some(*d as u32);
            continue;
        }
        let d = match rule(p).and_then(|r| r.task.as_ref()) {
            Some(t) if p.at != "design" => draft(&t.title, &t.detail, effort(&t.effort), t.phase.clamp(1, 4)),
            _ if new || p.at == "design" => draft(&p.title, &p.fix, "M", 2),
            _ => continue,
        };
        drafts.push(Draft {
            fixes: Some(p.id.clone()),
            ..d
        });
        by_rule.push((key.clone(), drafts.len() - 1));
        findings[k].task = Some((drafts.len() - 1) as u32);
    }

    let product = |p: &str| lines.iter().any(|l| l.part.product.as_deref() == Some(p));
    let (sql, firestore) = (product("cloud-sql"), product("firestore"));
    let mut infra = vec!["service".to_string()];
    for (on, what) in [
        (sql, "Cloud SQL"),
        (firestore, "Firestore"),
        (has("files"), "bucket"),
        (has("worker"), "queue"),
        (has("cron"), "scheduler"),
    ] {
        if on {
            infra.push(what.into());
        }
    }
    let mut detail = if sql {
        "Max 3 instances and a database pool of 5 a copy, because db-f1-micro takes 25 connections.".to_string()
    } else {
        "Max 3 instances to begin with; the busiest hour decides when to raise it.".to_string()
    };
    if firestore {
        detail.push_str(
            " Firestore's (default) database in Native mode in the service's region, the one database the free quota \
             covers; the service's account gets roles/datastore.user.",
        );
    }
    let products: Vec<&str> = lines.iter().filter_map(|l| l.part.product.as_deref()).collect();
    detail.push(' ');
    detail.push_str(&account(&products, new, name));
    drafts.push(draft(format!("Infrastructure: {}", infra.join(", ")), detail, "M", 3));
    drafts.push(draft("Budget and alerts", budget(lines, name), "S", 3));
    let ci = match f.settings.ci.as_deref() {
        Some(ci) if ci.starts_with(".github") => "GitHub Actions signs in".to_string(),
        Some(ci) => format!("The CI in {ci} signs in"),
        None => "The deploy signs in".into(),
    };
    drafts.push(draft(
        "Keyless deploys from CI",
        format!("{ci} with Workload Identity Federation; no key file anywhere."),
        "S",
        3,
    ));
    if new {
        drafts.push(draft(
            "Domain and launch",
            "The domain on Cloud Run, then the first users.",
            "S",
            4,
        ));
    } else {
        let mut copy = Vec::new();
        let mut what = Vec::new();
        for d in &f.data {
            let step = match (d.kind.as_str(), d.engine.as_str()) {
                ("db", "MySQL" | "MariaDB") => "mysqldump into Cloud SQL, then row counts and checksums per table",
                ("db", _) => "pg_dump into Cloud SQL, then row counts and checksums per table",
                ("sqlite", _) => {
                    "a one-off script copies each SQLite table into a Firestore collection, a row a document, then \
                     the documents of each collection are counted against the table's rows"
                }
                ("files", _) => "the files with gcloud storage rsync",
                _ => continue,
            };
            what.push(d.name.clone());
            copy.push(step);
        }
        if !copy.is_empty() {
            drafts.push(draft(
                format!("Data: copy {} and compare", join(&what)),
                format!("{}.", copy.join("; ")),
                "M",
                4,
            ));
        }
        let from = if f.platform.name == "Unknown" {
            "the old host"
        } else {
            &f.platform.name
        };
        drafts.push(draft(
            format!("Switch the domain and stop {from}"),
            format!("Point the domains at Cloud Run in a quiet hour; {from} stays stopped, not deleted, for a week."),
            "S",
            4,
        ));
    }

    // phase order, stable: the standard steps of a phase keep their place around the rules' tasks
    let mut order: Vec<usize> = (0..drafts.len()).collect();
    order.sort_by_key(|&k| drafts[k].phase);
    let place = |k: usize| order.iter().position(|&o| o == k).map_or(0, |p| p as u32 + 1);
    for x in findings.iter_mut() {
        x.task = x.task.map(|k| place(k as usize));
    }
    order
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            let d = &drafts[k];
            ReportTask {
                i: i as u32 + 1,
                title: d.title.clone(),
                detail: d.detail.clone(),
                effort: d.effort.into(),
                phase: d.phase,
                fixes: d.fixes.clone(),
                since: None,
                status: "todo".into(),
                done_at: None,
                commit: None,
            }
        })
        .collect()
}

/// The budget task: the monthly Cloud Run estimate rounded up to whole dollars, never under $5, with alerts at 50%,
/// 90% and 100% of it and on the forecast; then a Cloud Run spend cap at twice that, never under $10 ([`cap_usd`]).
/// Both change billing settings, so the person approves each first.
fn budget(lines: &[Line], name: &str) -> String {
    let cents: i64 = lines.iter().map(|l| l.cents).sum();
    let usd = budget_usd(cents);
    let cap = cap_usd(usd);
    format!(
        "A Cloud Billing budget of ${usd} a month on this project (the Cloud Run estimate rounded up, at least $5), with \
         email alerts to the billing admins at 50%, 90% and 100% of actual spend and at 100% of the forecast: \
         `gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID --display-name=\"{name} on Cloud Run\" \
         --budget-amount={usd}USD --calendar-period=month --filter-projects=projects/PROJECT_ID \
         --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0 \
         --threshold-rule=percent=1.0,basis=forecasted-spend` (the currency is the billing account's own; \
         `--notifications-rule-pubsub-topic` adds a Pub/Sub topic for automated reactions). It needs Billing Account \
         Administrator or Billing Account Costs Manager, or the project's owner for a budget on this project only. It \
         changes billing settings: the agent shows the command, the person approves, and it runs with the person's \
         own gcloud. How budgets work: {SITE}/blog/budget-alerts-and-the-billing-export. \
         Then a spend cap of ${cap} a month, a second budget that stops Cloud Run instead of only warning: in the \
         console, Billing, Budgets & alerts, Create new budget, Spend cap enforcement, this project, the service \
         Cloud Run, the amount ${cap} (or the Cloud Billing Budget API: `spendCap.inputState` CONFIGURED, the service \
         `services/152E-C115-5142`, `creditTypesTreatment` EXCLUDE_ALL_CREDITS, thresholds at 50%, 80% and 100%, the \
         full request on Google's page below). Past the cap, Cloud Run in this project pauses until the 1st of the \
         next month or until the person lifts the cap: services answer 5xx, jobs and worker pools stop, nothing is \
         deleted. It counts gross list-price costs before savings and credits and fires before the bill shows them, \
         so it sits at twice the budget, at least $10, not at the estimate. It is not instant: what runs before it \
         fires is billed as usual. Spend caps are monthly, for one project and one service; alerts go to the billing \
         admins and project owners at 50%, 80% and 100%. It needs `billing.budgets.configureSpendCap` on the billing \
         account, or `billing.resourcebudgets.configureSpendCap` on the project. Turning the app off at a ceiling is \
         the person's call: the agent explains it, shows the steps and waits for a yes or no. How spend caps work: \
         https://docs.cloud.google.com/billing/docs/how-to/budgets-spend-caps."
    )
}

/// The account the infrastructure task needs. Google's Starter Tier runs apps with no billing account, but only apps
/// published from Google AI Studio's Build mode, on four products; a repository deployed with gcloud needs a billing
/// account, and one that never had any starts on the Free Trial. A new product within the four may start in AI
/// Studio instead. Secret Manager does not count: it holds the repository's variables, and AI Studio keeps its own.
/// Sources (2026-10-27): docs.cloud.google.com/docs/starter-tier, ai.google.dev/gemini-api/docs/aistudio-deploying,
/// the Cloud blog's "The Starter Tier for Google AI Studio explained".
fn account(products: &[&str], new: bool, name: &str) -> String {
    let mut outside: Vec<&str> = Vec::new();
    for p in products {
        let what = match *p {
            "scheduler" => "Cloud Scheduler",
            "cloud-tasks" => "Cloud Tasks",
            "cloud-storage" => "Cloud Storage",
            _ => continue,
        };
        if !outside.contains(&what) {
            outside.push(what);
        }
    }
    let mut s = String::new();
    if new && outside.is_empty() {
        s.push_str(&format!(
            "If {name} starts as a prototype in Google AI Studio's Build mode, its Starter Tier publishes it with no \
             billing account (Cloud Run, Firestore, Cloud SQL for PostgreSQL and Firebase Authentication; up to two \
             apps, one instance each, one region, an eligible personal Google account: not Workspace, no paid billing \
             account now or before), and the project later upgrades to billing in place with the same URLs and data. "
        ));
    }
    s.push_str(
        "This plan deploys from the repository with gcloud, so the project needs a billing account: the Starter Tier \
         (docs.cloud.google.com/docs/starter-tier) takes only apps published from Google AI Studio",
    );
    if outside.is_empty() {
        s.push_str(". ");
    } else {
        s.push_str(&format!(
            ", and {} {} outside its four products. ",
            outside.join(", "),
            if outside.len() == 1 { "is" } else { "are" }
        ));
    }
    s.push_str(
        "A person who never had a billing account starts on Google's Free Trial: $300 of credit for 90 days, nothing \
         billed until they upgrade to a paid account (docs.cloud.google.com/free/docs/free-cloud-features). Creating \
         or linking the account is the person's step: ask with `exodus need`.",
    );
    s
}

/// Whole dollars, rounded up, at least 5.
fn budget_usd(cents: i64) -> i64 {
    ((cents.max(0) + 99) / 100).max(5)
}

/// The spend cap above the budget: twice it, at least $10. The cap stops the app, and Google counts it on gross
/// list-price costs before savings and credits, so a cap at the estimate would stop an app in an ordinary month.
fn cap_usd(budget_usd: i64) -> i64 {
    (budget_usd * 2).max(10)
}

fn effort(e: &str) -> &'static str {
    match e {
        "S" => "S",
        "L" => "L",
        _ => "M",
    }
}

/// The questions the audit asked, then any other the agent answered, with their answers.
fn questions(f: &Findings, answers: &[Answer], n: u32) -> Vec<ReportQuestion> {
    let mut asked: Vec<(String, String, String)> = f
        .questions
        .iter()
        .map(|q| (q.id.clone(), q.q.clone(), q.affects.clone()))
        .collect();
    for a in answers {
        if !asked.iter().any(|(id, ..)| *id == a.id) {
            asked.push((a.id.clone(), a.q.clone(), String::new()));
        }
    }
    asked
        .into_iter()
        .enumerate()
        .map(|(i, (id, q, affects))| {
            let a = answers.iter().find(|a| a.id == id);
            ReportQuestion {
                i: i as u32 + 1,
                id,
                q,
                answer: a.map(|a| a.answer.clone()),
                found: a.and_then(|a| a.found.clone()),
                affects,
                asked: n,
                answered: a.map(|_| n),
                is_answered: a.is_some(),
            }
        })
        .collect()
}

/// Everything the audit sent, in groups; its length is the report's count of findings.
fn inventory(f: &Findings, lines: &[Line], findings: &[ReportFinding], q: &[ReportQuestion], new: bool) -> Vec<Group> {
    let clean = split_clean(&f.clean);
    let kinds = |ks: &[&str]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| ks.contains(&l.part.kind.as_str()))
            .map(|l| format!("{}: {}", l.part.name, if new { &l.part.after } else { &l.part.now }))
            .collect()
    };
    let mut processes = kinds(&["web", "worker", "cron"]);
    processes.extend(
        f.processes
            .iter()
            .filter(|p| p.kind == "release")
            .map(|p| format!("{}: {}", p.name, p.command)),
    );
    let groups = [
        ("Processes", processes),
        ("Data", kinds(&["db", "cache", "files"])),
        ("Build, settings and traffic", kinds(&["build", "secrets", "traffic"])),
        ("Stack", f.stack.clone()),
        (
            if new { "Design decisions" } else { "Patterns found" },
            findings
                .iter()
                .map(|x| {
                    if x.at == "design" {
                        x.title.clone()
                    } else {
                        format!("{} ({})", x.title, x.at)
                    }
                })
                .collect(),
        ),
        (
            "Looked for, not found",
            clean.looked_for.iter().map(|c| format!("No {c}")).collect(),
        ),
        (
            "Not on the server, not counted",
            clean.not_on_server.iter().map(|c| c.to_string()).collect(),
        ),
        (
            "Stays as it is, not counted",
            clean.stays.iter().map(|c| c.to_string()).collect(),
        ),
        (
            "Read by your agent",
            q.iter()
                .filter(|q| q.is_answered)
                .filter_map(|q| {
                    Some(format!(
                        "{} {} (from {})",
                        q.q,
                        q.answer.as_deref().unwrap_or_default().trim_end_matches('.'),
                        q.found.as_ref()?
                    ))
                })
                .collect(),
        ),
    ];
    groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(t, items)| Group { title: t.into(), items })
        .collect()
}

/// The knowledge base behind each section and finding, and Google's price list of each product the cost uses.
/// Every slug is a guide's own, as the site's blog serves it; a finding without one cites nothing.
fn sources(prices: &Prices, lines: &[Line], findings: &[ReportFinding], platform: &str, new: bool) -> Sources {
    let mut kb: Vec<KbSource> = Vec::new();
    let mut cite = |slug: &str, where_: String| match kb.iter_mut().find(|k| k.slug == slug) {
        _ if slug.is_empty() => {}
        Some(k) => k.cited.push(where_),
        None => kb.push(KbSource {
            slug: slug.into(),
            cited: vec![where_],
        }),
    };
    cite("what-cloud-run-costs", "Cost".into());
    let from = match platform {
        _ if new => "planning-a-new-product-for-cloud-run",
        "Heroku" => "heroku-to-cloud-run",
        "Railway" | "Render" => "railway-or-render-to-cloud-run",
        "Fly.io" => "fly-to-cloud-run",
        _ => "what-cloud-run-is",
    };
    cite(from, "Now and after".into());
    for x in findings {
        cite(&x.kb, format!("Finding {}", x.i));
    }
    let mut seen = HashSet::new();
    let google = lines
        .iter()
        .flat_map(|l| &l.uses)
        .filter(|u| u.qty > 0.0)
        .filter_map(|u| prices.skus.get(&u.sku).map(|s| &s.product))
        .filter(|p| seen.insert(p.as_str()))
        .filter_map(|p| prices.products.get(p))
        .map(|p| GoogleSource {
            what: p.name.clone(),
            doc: p.doc.clone(),
        })
        .collect();
    Sources { kb, google }
}

/// A weekday on both platforms: the processes billed every hour today, the CPU hours on Cloud Run.
fn day(f: &Findings, lines: &[Line], list: Option<&exodus_schema::PlatformList>, new: bool) -> Day {
    let cpu: f64 = lines
        .iter()
        .flat_map(|l| &l.uses)
        .filter(|u| matches!(u.sku.as_str(), "run-cpu" | "job-cpu" | "run-cpu-always"))
        .map(|u| u.qty)
        .sum();
    let platform = &f.platform.name;
    let billed = f
        .processes
        .iter()
        .filter(|p| matches!(p.kind.as_str(), "web" | "worker" | "cron") && p.always_on)
        .count() as u32;
    let sleep_note = if new {
        Some("A server you rent bills every hour, busy or not.".into())
    } else {
        match &f.platform.sleep {
            Some(s) if s.on => Some(format!(
                "{platform} sleeps this app when it is idle ({}), so it runs only while used.",
                s.from
            )),
            // the list's note speaks of processes running every hour; with none found it does not apply
            _ if billed == 0 => None,
            // the list's note speaks of a file the audit read; a host from an answer has none
            _ if f.platform.file.is_empty() && list.is_some() => Some(format!(
                "{platform} may sleep idle services on some plans; this report counts the processes found as running every hour."
            )),
            _ => list.map(|l| l.sleep.clone()),
        }
    };
    Day {
        processes_all_day: billed,
        // an empty sum of f64 is -0.0, which prints as "-0.0"; adding 0.0 makes it 0
        cpu_hours_a_day: (cpu / exodus_cost::DAYS / 3600.0 * 100.0).round() / 100.0 + 0.0,
        always_on: lines.iter().any(|l| l.uses.iter().any(|u| u.sku == "run-cpu-always")),
        sleep_note,
        serverless: SERVERLESS.contains(&platform.as_str()),
    }
}

/// One line on what the app is: "Node/Express app with a BullMQ worker, 3 cron schedules, Postgres and Redis".
fn what(f: &Findings) -> String {
    let lang = parts::language(&f.stack);
    let web = f
        .processes
        .iter()
        .find(|p| p.kind == "web")
        .and_then(|p| p.framework.as_deref());
    let head = match (lang, web) {
        ("", _) => "An app".to_string(),
        (l, Some(w)) => format!("{l}/{w} app"),
        (l, None) => format!("{l} app"),
    };
    let mut with: Vec<String> = f
        .processes
        .iter()
        .filter(|p| p.kind == "worker")
        .map(|p| format!("a {} worker", p.framework.as_deref().unwrap_or("background")))
        .collect();
    let schedules: usize = f
        .processes
        .iter()
        .map(|p| p.schedules.as_ref().map_or(0, Vec::len))
        .sum();
    if schedules > 0 {
        with.push(count(schedules, "cron schedule"));
    }
    with.extend(tally(
        &f.data
            .iter()
            .map(|d| match d.kind.as_str() {
                "files" => "files on disk".to_string(),
                _ => d.name.clone(),
            })
            .collect::<Vec<_>>(),
    ));
    if with.is_empty() {
        head
    } else {
        format!("{head} with {}", join(&with))
    }
}

/// Whether two findings are the same pattern in the same file. Ids are numbered by file order (`<rule>-2`), and a
/// rule that stops firing in one file renumbers the rest, so the id cannot say it; a line can move with an edit.
fn same(a: &ReportFinding, b: &ReportFinding) -> bool {
    let file = |at: &str| at.rsplit_once(':').map_or(at, |(f, _)| f).to_string();
    a.title == b.title && file(&a.at) == file(&b.at)
}

/// What this report learned since `prev`: parts and patterns found or gone, answers, and how the numbers moved.
fn since(prev: &Report, r: &Report) -> Since {
    let n = r.n;
    let mut lines: Vec<String> = Vec::new();
    lines.extend(
        r.cost
            .lines
            .iter()
            .filter(|l| l.part.since == Some(n) && l.part.kind != "bill")
            .map(|l| format!("Found {}: {}", l.part.name, l.part.now)),
    );
    lines.extend(
        r.findings
            .iter()
            .filter(|x| x.since == Some(n))
            .map(|x| format!("New pattern: {} ({})", x.title, x.at)),
    );
    lines.extend(
        prev.findings
            .iter()
            .filter(|p| !r.findings.iter().any(|x| same(p, x)))
            .map(|p| format!("No longer found: {} ({})", p.title, p.at)),
    );
    // mock.ts reportDiff: an answer counts as learned when an earlier report asked the question
    let answered: Vec<&ReportQuestion> = r
        .questions
        .iter()
        .filter(|q| q.answered == Some(n) && q.asked < n)
        .collect();
    if !answered.is_empty() {
        let head = if answered.len() == 1 {
            "An answer".to_string()
        } else {
            format!("{} answers", answered.len())
        };
        lines.push(format!(
            "{head}: {}",
            answered.iter().map(|q| q.q.as_str()).collect::<Vec<_>>().join(" ")
        ));
    }
    let (a, b) = (&prev.assumptions, &r.assumptions);
    let traffic = &r.project.traffic;
    if a.visits != b.visits {
        lines.push(format!(
            "{}{} a day {} -> {}",
            traffic[..1].to_uppercase(),
            &traffic[1..],
            num(a.visits as f64),
            num(b.visits as f64)
        ));
    }
    if a.egress_gb != b.egress_gb {
        lines.push(format!(
            "Data sent out {} -> {} GB a month",
            num(a.egress_gb),
            num(b.egress_gb)
        ));
    }
    if a.region != b.region {
        lines.push(format!("Region {} -> {}", a.region, b.region));
    }
    if prev.cost.today != r.cost.today {
        let t = |c: Option<i64>| c.map_or_else(|| "unknown".into(), usd);
        lines.push(format!("Today {} -> {}", t(prev.cost.today), t(r.cost.today)));
    }
    if prev.cost.cloud_run != r.cost.cloud_run {
        lines.push(format!(
            "Cloud Run {} -> {}",
            usd(prev.cost.cloud_run),
            usd(r.cost.cloud_run)
        ));
    }
    if prev.verdict.label != r.verdict.label {
        lines.push(format!("Verdict: {} -> {}", prev.verdict.label, r.verdict.label));
    }
    // the agent's account is part of what a report is built from: a new one is a new version
    match (&prev.understanding, &r.understanding) {
        (None, Some(_)) => lines.push("Your agent described the project".into()),
        (Some(_), None) => lines.push("Your agent's description of the project was removed".into()),
        (Some(a), Some(b)) if a != b => lines.push("Your agent described the project again".into()),
        _ => {}
    }
    Since { prev: prev.n, lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, at: &str) -> ReportFinding {
        serde_json::from_value(serde_json::json!({
            "i": 1, "id": id, "kind": "breaks", "title": "Files written to local disk", "at": at,
            "code": "", "why": "", "fix": "", "part": "files", "kb": ""
        }))
        .unwrap()
    }

    #[test]
    fn a_finding_renumbered_or_moved_is_the_same() {
        // a rule that stopped firing in one file renumbers the others: local-disk-write-2 becomes local-disk-write
        assert!(same(
            &finding("local-disk-write-2", "a/fs.go:37"),
            &finding("local-disk-write", "a/fs.go:39")
        ));
        assert!(!same(
            &finding("local-disk-write", "cmd/install.go:301"),
            &finding("local-disk-write", "a/fs.go:37")
        ));
    }

    #[test]
    fn the_budget_is_the_estimate_rounded_up_and_at_least_5() {
        assert_eq!(budget_usd(0), 5);
        assert_eq!(budget_usd(-300), 5);
        assert_eq!(budget_usd(499), 5);
        assert_eq!(budget_usd(1083), 11);
        assert_eq!(budget_usd(1100), 11);
        assert_eq!(budget_usd(4401), 45);
    }

    #[test]
    fn the_starter_tier_is_offered_only_to_a_new_product_on_its_four_products() {
        let small = account(&["cloud-run", "firestore", "egress"], true, "notes");
        assert!(small.starts_with("If notes starts as a prototype in Google AI Studio"));
        assert!(small.contains("Free Trial") && !small.contains("outside its four products"));

        let worker = account(&["cloud-run", "cloud-tasks", "scheduler", "cloud-tasks"], true, "notes");
        assert!(!worker.contains("prototype in Google AI Studio"));
        assert!(worker.contains("and Cloud Tasks, Cloud Scheduler are outside its four products"));

        let moved = account(&["cloud-run", "cloud-sql"], false, "shop");
        assert!(moved.starts_with("This plan deploys from the repository with gcloud"));
        assert!(moved.contains("takes only apps published from Google AI Studio. A person who never had"));
    }

    #[test]
    fn the_spend_cap_is_twice_the_budget_and_at_least_10() {
        assert_eq!(cap_usd(5), 10);
        assert_eq!(cap_usd(11), 22);
        assert_eq!(cap_usd(45), 90);
        let text = budget(&[], "app");
        assert!(text.contains("budget of $5 a month") && text.contains("spend cap of $10 a month"));
        assert!(text.contains("services/152E-C115-5142") && text.contains("waits for a yes or no"));
    }
}
