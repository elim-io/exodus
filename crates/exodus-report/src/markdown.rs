//! The report as one Markdown document, in the cabinet's order: our understanding, verdict, what the app gains, cost,
//! now and after, the changes, checklist, roadmap, questions, what the audit sent, sources. A new product's plan names
//! some of the sections for what they hold. Formal voice: a sentence per fact, no exclamations.
// Note: one block per section of the cabinet's report screen, each a few lines of text

use exodus_schema::Report;

use crate::parts::{count, join, num, one};

/// "$48.00", "$1,234.56".
pub fn usd(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let c = cents.abs();
    format!("{sign}${}.{:02}", num((c / 100) as f64), c % 100)
}

/// "$0.000024", "$0.0105", "$0.10", "$0.00000033": a unit price with at least two decimals and at most nine.
fn price(usd: f64) -> String {
    let s = format!("{usd:.9}");
    let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
    let frac = frac.trim_end_matches('0');
    format!("${whole}.{frac:0<2}")
}

/// "Oct 11, 2026" from an ISO date or date-time.
fn date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut p = iso.get(..10).unwrap_or(iso).split('-');
    match (
        p.next(),
        p.next().and_then(|m| m.parse::<usize>().ok()),
        p.next().and_then(|d| d.parse::<u32>().ok()),
    ) {
        (Some(y), Some(m @ 1..=12), Some(d)) => format!("{} {d}, {y}", MONTHS[m - 1]),
        _ => iso.into(),
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

fn today(c: Option<i64>) -> String {
    c.map_or_else(|| "-".into(), usd)
}

/// The labels of the agent's three paragraphs (`Understanding`), a move's or a new product's.
const UNDERSTANDING: [[&str; 3]; 2] = [
    ["What it is", "How it runs today", "What we will do"],
    ["The idea", "Where it starts", "What we will build"],
];

/// "Our understanding": the agent's three paragraphs (or a line saying it has not written them), then the parts the
/// audit found, as they are today (a new product's as planned). The report is built without a model, so the meaning
/// comes from the agent that read the code and the structure from the findings.
fn understanding(r: &Report, from: &str) -> Vec<String> {
    let new = r.project.kind == "new";
    let mut o = vec!["## Our understanding".to_string()];
    match &r.understanding {
        Some(u) => {
            let labels = UNDERSTANDING[usize::from(new)];
            for (label, text) in labels.iter().zip([&u.project, &u.today, &u.plan]) {
                o.push(format!("**{label}.** {}", text.trim()));
            }
        }
        None => o.push(
            "Your agent has not described the project yet; `exodus describe` adds what it read in the code. Below is only what the audit found."
                .into(),
        ),
    }
    let names = |kinds: &[&str]| -> Vec<String> {
        r.cost
            .lines
            .iter()
            .filter(|l| kinds.contains(&l.part.kind.as_str()))
            .map(|l| match l.part.now.as_str() {
                _ if new => l.part.name.clone(),
                now => format!("{} ({})", l.part.name, now),
            })
            .collect()
    };
    let mut parts = Vec::new();
    if !new {
        parts.push(format!("- Runs on: {from}"));
    }
    let outside: Vec<String> = r.stays.iter().chain(&r.not_on_server).cloned().collect();
    for (label, items) in [
        ("Processes", names(&["web", "worker", "cron"])),
        ("Data", names(&["db", "cache", "files"])),
        ("Outside the server, not moved", outside),
    ] {
        if !items.is_empty() {
            parts.push(format!("- {label}: {}", items.join("; ")));
        }
    }
    if !parts.is_empty() {
        let head = if new {
            "The parts, as planned:"
        } else {
            "Today, as the audit read it:"
        };
        o.push(head.into());
        o.push(parts.join("\n"));
    }
    o
}

/// The report as Markdown; `site` is where the knowledge base is published, `app` the cabinet whose `/r/<token>`
/// opens a shared report.
pub fn markdown(r: &Report, site: &str, app: &str) -> String {
    let p = &r.project;
    let new = p.kind == "new";
    let stay = r.verdict.kind == "stay";
    let a = &r.assumptions;
    // a host no file and no answer names is the current one, never "Unknown"
    let from = if p.from == "Unknown" {
        "the current host"
    } else {
        p.from.as_str()
    };
    let mut o: Vec<String> = Vec::new();
    let push = |o: &mut Vec<String>, s: String| {
        o.push(s);
        o.push(String::new());
    };

    push(
        &mut o,
        format!("# {}: {} {}", p.name, if new { "plan" } else { "report" }, r.n),
    );
    let with = p.agent.as_deref().map(|a| format!(" with {a}")).unwrap_or_default();
    push(
        &mut o,
        format!(
            "{}. {} {} of {}, {}. Made by exodus {}{with} from {} findings; no code was uploaded.",
            if new {
                "A new product on Google Cloud Run".to_string()
            } else {
                format!("From {from} to Google Cloud Run")
            },
            if new { "Plan" } else { "Report" },
            r.n,
            r.reports,
            date(&r.at),
            p.cli,
            r.findings_count
        ),
    );
    push(
        &mut o,
        match (&r.token, r.shared) {
            (Some(t), Some(true)) => format!("Share link: {app}/r/{t}"),
            (Some(_), _) => "Private: only you can open it, signed in to Exodus.".into(),
            (None, _) => "Built on this machine by `exodus report`; it has no share link.".into(),
        },
    );

    for block in understanding(r, from) {
        push(&mut o, block);
    }

    // Verdict
    push(&mut o, format!("## {}", if new { "Summary" } else { "Verdict" }));
    let reason = if r.verdict.reason.is_empty() {
        String::new()
    } else {
        format!(" {}", r.verdict.reason)
    };
    push(&mut o, format!("**{}.**{reason}", r.verdict.title));
    if new {
        let year = crate::forecast(r, r.growth)
            .pop()
            .unwrap_or((12, a.visits, r.cost.cloud_run));
        let free: i64 = r
            .cost
            .lines
            .iter()
            .flat_map(|l| &l.uses)
            .map(|u| (u.free * 100.0).round() as i64)
            .sum();
        push(
            &mut o,
            format!(
                "At launch {} a month at {} {} a day. In a year {} a month at {} a day, at {}% growth a month. The free tier covers {} a month at launch.",
                usd(r.cost.cloud_run),
                num(a.visits as f64),
                p.traffic,
                usd(year.2),
                num(year.1 as f64),
                num(r.growth),
                usd(free)
            ),
        );
    } else {
        let diff = r.cost.today.map(|t| r.cost.cloud_run - t);
        let line = match (r.cost.today, diff) {
            (Some(t), Some(d)) => format!(
                "Today {} a month on {}{}, {} on Cloud Run: {}.",
                usd(t),
                p.from,
                match r.today_basis.as_str() {
                    "bill" => " (from your bill)",
                    _ => " (estimated from its list prices)",
                },
                usd(r.cost.cloud_run),
                if d <= 0 {
                    format!("saves {} a month, {} a year", usd(-d), usd(-d * 12))
                } else {
                    format!("costs {} more a month", usd(d))
                }
            ),
            _ => format!(
                "{} a month on Cloud Run. Today's cost on {from} is not known until your bill is read.",
                usd(r.cost.cloud_run),
            ),
        };
        push(&mut o, line);
    }
    if let Some(s) = r.since.as_ref().filter(|s| !s.lines.is_empty()) {
        push(&mut o, format!("Since report {}:", s.prev));
        push(
            &mut o,
            s.lines.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"),
        );
    }
    let changes: Vec<_> = r.findings.iter().filter(|f| f.kind != "note").collect();
    if !changes.is_empty() && !stay && !new {
        push(
            &mut o,
            format!("The {} before the move:", count(changes.len(), "change")),
        );
        push(
            &mut o,
            changes
                .iter()
                .map(|f| format!("{}. {} (`{}`)", f.i, f.title, f.at))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if let Some(flip) = &r.verdict.flip {
        push(&mut o, format!("What would change it: {flip}"));
    }
    let by_agent = r
        .questions
        .iter()
        .filter(|q| q.is_answered && q.found.is_some())
        .count();
    push(
        &mut o,
        if r.open.is_empty() {
            format!(
                "Confidence: high. All {} answered{}.",
                count(r.questions.len(), "question"),
                if by_agent > 0 {
                    format!(", {by_agent} of them by your agent")
                } else {
                    String::new()
                }
            )
        } else {
            format!(
                "Confidence: lower. {} of {} questions open (see Questions).",
                r.open.len(),
                r.questions.len()
            )
        },
    );

    // What the app gains
    push(&mut o, format!("## What {} gains on Cloud Run", p.name));
    push(
        &mut o,
        if stay {
            format!(
                "Cloud Run bills for the seconds the code works. {} keeps its CPU busy most of the day, so it would pay for nearly every hour; once the checklist's changes let it rest, it gains what is listed below.",
                p.name
            )
        } else if new {
            "Cloud Run bills for the seconds the code works, so a product built from parts that hold no state costs little while it is small and grows without a rebuild. The decisions in this plan are those parts.".into()
        } else {
            format!(
                "Cloud Run bills for the seconds the code works. It rewards an app built from parts that hold no state and charges for parts that wait, so the changes in this report are the same ones that make {} faster to scale and simpler to run.",
                p.name
            )
        },
    );
    let d = &r.day;
    let mut weekday = Vec::new();
    if new {
        weekday.push("- On an always-on server: billed every hour, 24 hours a day.".to_string());
    } else if d.processes_all_day == 0 && !d.serverless {
        weekday.push(format!("- On {from}: no process found that is billed every hour."));
    } else if !d.serverless {
        weekday.push(format!(
            "- On {from}: {} billed every hour: {} process-hours a day.",
            count(d.processes_all_day as usize, "process"),
            d.processes_all_day * 24
        ));
    }
    weekday.push(if d.always_on {
        "- On Cloud Run: billed every hour as well; the CPU is busy most of the day.".into()
    } else {
        let h = if d.cpu_hours_a_day < 10.0 {
            format!("{:.1}", d.cpu_hours_a_day)
        } else {
            num(d.cpu_hours_a_day.round())
        };
        format!("- On Cloud Run: CPU only while requests and jobs run, about {h} CPU-hours a day.")
    });
    push(&mut o, format!("A weekday of {}:", p.name));
    push(&mut o, weekday.join("\n"));
    let mut note = d.sleep_note.clone().unwrap_or_default();
    if !note.is_empty() {
        note.push(' ');
    }
    push(
        &mut o,
        format!("{note}Cloud Run's hours are computed from this report's numbers."),
    );
    let db = r
        .cost
        .lines
        .iter()
        .any(|l| l.part.kind == "db" && l.part.product.as_deref() == Some("cloud-sql"));
    let gains = [
        (
            "Scales with the traffic",
            "From no copies at night to as many as the traffic needs, each taking up to 1,000 requests at once.",
        ),
        (
            "Nothing to keep running",
            "No server, operating system or certificate to look after. Google patches the machines, renews HTTPS and restarts a copy that fails.",
        ),
        (
            "Releases without downtime",
            "Each deploy is a revision. Send 5% of the traffic to it first, move everyone when it holds, and go back in one click.",
        ),
        if db {
            (
                "A private line to the database",
                "Cloud Run reaches Cloud SQL over Google's network with IAM, without a public address; backups and point-in-time recovery are on.",
            )
        } else {
            (
                "Close to your users",
                "Requests enter Google's network near the visitor and travel on Google's own backbone to the region.",
            )
        },
        (
            "The rest of Google Cloud",
            "BigQuery, Pub/Sub, Cloud Storage and Gemini are one permission away, on the same bill.",
        ),
        (
            "99.95% uptime in writing",
            "Google's SLA covers every Cloud Run service, with credits when a month falls short.",
        ),
    ];
    push(
        &mut o,
        gains
            .iter()
            .map(|(t, x)| format!("- **{t}.** {x}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let files_part = r
        .cost
        .lines
        .iter()
        .find(|l| l.part.kind == "files")
        .map(|l| l.part.id.as_str());
    let files_task = r
        .findings
        .iter()
        .find(|f| Some(f.part.as_str()) == files_part)
        .and_then(|f| f.task);
    push(&mut o, "What works differently:".into());
    push(
        &mut o,
        [
            "- A copy that starts after a quiet spell answers in about 0.5 to 2 seconds. Startup CPU boost shortens that; one copy kept warm removes it, for a few dollars a month.".to_string(),
            "- A request can run for up to 60 minutes. Longer work runs as a Cloud Run job, for up to 24 hours.".into(),
            format!(
                "- The disk of a copy lives in its memory and goes with it, so files are kept in Cloud Storage{}.",
                files_task.map(|t| format!(" (task {t})")).unwrap_or_default()
            ),
        ]
        .join("\n"),
    );

    // Cost
    push(&mut o, "## Cost".into());
    let place = r.prices.regions.get(&a.region).map(|x| x.place.as_str()).unwrap_or("");
    push(
        &mut o,
        format!(
            "Assumptions: {} {} a day, {} ({place}), {} GB of egress a month.\nPrices: Google list prices, {}, read {}.",
            num(a.visits as f64),
            p.traffic,
            a.region,
            num(a.egress_gb),
            a.region,
            date(&r.prices.date)
        ),
    );
    let mut table = vec![
        "| Part | Now | On Google Cloud | Today | Cloud Run |".to_string(),
        "|---|---|---|---:|---:|".into(),
    ];
    for l in &r.cost.lines {
        let after = l
            .part
            .removed
            .as_ref()
            .map_or_else(|| l.part.after.clone(), |x| format!("Removed: {x}"));
        table.push(format!(
            "| {} | {} | {} | {} | {} |",
            cell(&l.part.name),
            cell(&l.part.now),
            cell(&after),
            today(l.part.today),
            if l.part.kind == "bill" {
                "-".into()
            } else {
                usd(l.cents)
            }
        ));
    }
    table.push(format!(
        "| **Total a month** | | | **{}** | **{}** |",
        today(r.cost.today),
        usd(r.cost.cloud_run)
    ));
    push(&mut o, table.join("\n"));
    push(
        &mut o,
        match r.today_basis.as_str() {
            "bill" if r.cost.lines.iter().any(|l| l.part.kind == "bill") => if r
                .cost
                .lines
                .iter()
                .any(|l| l.part.kind != "bill" && l.part.today.is_some())
            {
                "Today: your bill, part by part; what it does not split by part is on a line of its own."
            } else {
                "Today: your bill as one total, on a line of its own; it is not split by part."
            }
            .to_string(),
            "bill" => "Today: your bill, part by part.".to_string(),
            "list" => format!(
                "Today: {}'s list prices for its smallest paid sizes; the bill answer replaces them.",
                p.from
            ),
            _ if new => "Today: nothing, the product is not built yet.".into(),
            _ => "Today: not known until your bill is read.".into(),
        },
    );
    // F5: which lines the forecast grows with the traffic, and by how much
    let grows = crate::growing(r);
    let stays: Vec<String> = r
        .cost
        .lines
        .iter()
        .filter(|l| l.part.kind != "bill" && !grows.contains(&l.part.name))
        .map(|l| l.part.name.clone())
        .collect();
    let (egress, files) = crate::rates(r);
    let mut growth = format!(
        "As the {} grow, these lines grow with them: {}. Egress adds about {} MB a {}",
        p.traffic,
        join(&grows),
        num(egress),
        one(&p.traffic)
    );
    if r.cost.lines.iter().any(|l| l.part.kind == "files") {
        growth.push_str(&format!(
            " and the stored files about {} MB a {} a day",
            num(files),
            one(&p.traffic)
        ));
    }
    growth.push('.');
    if !stays.is_empty() {
        growth.push_str(&format!(" These stay as they are: {}.", join(&stays)));
    }
    if !new && r.growth != 0.0 {
        let (_, visits, cents) = crate::forecast(r, r.growth)
            .pop()
            .unwrap_or((12, a.visits, r.cost.cloud_run));
        growth.push_str(&format!(
            " In a year at {}% a month: {} {} a day, {} a month on Cloud Run.",
            num(r.growth),
            num(visits as f64),
            p.traffic,
            usd(cents)
        ));
    }
    push(&mut o, growth);
    push(&mut o, "How each Cloud Run number is computed:".into());
    let mut how = Vec::new();
    for l in &r.cost.lines {
        for u in &l.uses {
            let sku = r.prices.skus.get(&u.sku);
            let (what, unit) = sku.map_or((u.sku.as_str(), ""), |s| (s.what.as_str(), s.unit.as_str()));
            let free = if u.free > 0.0 {
                format!(
                    ", less {} free = {}",
                    usd((u.free * 100.0).round() as i64),
                    usd(u.cents)
                )
            } else {
                String::new()
            };
            how.push(format!(
                "- {}, {what}: {} {unit} x {} = {}{free} ({})",
                l.part.name,
                num(u.qty),
                price(u.price),
                usd((u.gross * 100.0).round() as i64),
                u.how
            ));
        }
    }
    push(&mut o, how.join("\n"));

    // Now and after
    push(
        &mut o,
        format!("## {}", if new { "Architecture" } else { "Now and after" }),
    );
    let mut parts = Vec::new();
    for l in r
        .cost
        .lines
        .iter()
        .filter(|l| !matches!(l.part.kind.as_str(), "traffic" | "bill"))
    {
        let x = &l.part;
        parts.push(match (&x.removed, new) {
            (Some(rm), _) => format!("- ~~{}: {}~~ -> removed: {rm}", x.name, x.now),
            (None, true) => format!("- {}: {}", x.name, x.after),
            (None, false) => format!("- {}: {} -> {}", x.name, x.now, x.after),
        });
    }
    push(&mut o, parts.join("\n"));
    push(
        &mut o,
        format!(
            "{}The container: {}.",
            match (p.stack.is_empty(), new) {
                (true, _) => String::new(),
                (false, true) => format!("Planned stack: {}. ", p.stack.join(", ")),
                (false, false) => format!("Read from the repository: {}. ", p.stack.join(", ")),
            },
            {
                let fw = r
                    .runtime
                    .framework
                    .as_deref()
                    .map(|f| format!(" running {f}"))
                    .unwrap_or_default();
                // a stack no file names: "Your container; Your Dockerfile" would say nothing
                if r.runtime.name == "Your container" {
                    format!("built from your Dockerfile{fw}")
                } else {
                    format!("{}{fw}; {}", r.runtime.name, r.runtime.build)
                }
            }
        ),
    );

    // Changes
    push(
        &mut o,
        format!("## {}", if new { "Design decisions" } else { "What needs a change" }),
    );
    for (kind, group) in [
        ("breaks", "Would not work on Cloud Run"),
        ("idle", "Would cost money while idle"),
        (
            "note",
            if new {
                "How it is built for Cloud Run"
            } else {
                "Worth knowing"
            },
        ),
    ] {
        let items: Vec<_> = r.findings.iter().filter(|f| f.kind == kind).collect();
        if items.is_empty() {
            continue;
        }
        push(&mut o, format!("### {group} ({})", items.len()));
        for f in items {
            let design = f.at == "design";
            let mut block = vec![format!("**{}. {}**", f.i, f.title)];
            block.push(if design {
                format!("Decision: `{}`", f.code)
            } else {
                format!("`{}`: `{}`", f.at, f.code)
            });
            block.push(format!("Why: {}", f.why));
            block.push(format!("{}: {}", if design { "How" } else { "Instead" }, f.fix));
            if !f.kb.is_empty() {
                block.push(format!("Read: {site}/blog/{}", f.kb));
            }
            if let Some(t) = f.task.and_then(|t| r.tasks.iter().find(|x| x.i == t)) {
                block.push(format!("Task: {}. {}", t.i, t.title));
            }
            push(&mut o, block.join("\n\n"));
        }
    }
    if r.findings.is_empty() {
        push(
            &mut o,
            "Nothing found that breaks or bills while idle on Cloud Run.".into(),
        );
    }
    if !r.clean.is_empty() {
        push(&mut o, format!("Looked for and not found: {}.", r.clean.join(", ")));
    }
    if !r.not_on_server.is_empty() {
        push(
            &mut o,
            format!("Not on the server, so not counted: {}.", r.not_on_server.join(", ")),
        );
    }
    if !r.stays.is_empty() {
        push(
            &mut o,
            format!("Stays as it is, so not counted: {}.", r.stays.join(", ")),
        );
    }

    // Checklist
    push(&mut o, "## Checklist".into());
    if stay {
        push(&mut o, "The changes that would make a move pay off.".into());
    }
    let status = |s: &str| match s {
        "done" => "Done",
        "next" => "Next",
        "blocked" => "Needs you",
        _ => "To do",
    };
    let mut rows = vec![
        "| # | Task | Answers | Effort | Status |".to_string(),
        "|---:|---|---|---|---|".into(),
    ];
    for t in &r.tasks {
        let finding = t.fixes.as_ref().and_then(|id| r.findings.iter().find(|f| f.id == *id));
        rows.push(format!(
            "| {} | {}: {} | {} | {} | {}{} |",
            t.i,
            cell(&t.title),
            cell(&t.detail),
            finding.map_or_else(|| "-".into(), |f| format!("Finding {}", f.i)),
            t.effort,
            status(&t.status),
            t.done_at
                .as_deref()
                .map(|d| format!(" ({})", date(d)))
                .unwrap_or_default()
        ));
    }
    push(&mut o, rows.join("\n"));
    push(
        &mut o,
        "Effort: S one file or a setting, M a few files and a test, L a part of the app rewritten.".into(),
    );

    // Roadmap
    push(&mut o, "## Roadmap".into());
    if stay {
        push(&mut o, "No roadmap while moving costs more. Once the checklist's changes land, a new report recomputes the cost and the verdict.".into());
    } else {
        let title = |i: &u32| r.tasks.iter().find(|t| t.i == *i).map_or("", |t| t.title.as_str());
        push(
            &mut o,
            r.phases
                .iter()
                .map(|ph| {
                    let tasks = if ph.tasks.is_empty() {
                        "nothing to change in the code".to_string()
                    } else {
                        ph.tasks.iter().map(title).collect::<Vec<_>>().join("; ")
                    };
                    format!("{}. {}: {tasks}.", ph.phase, ph.name)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    // Questions
    push(&mut o, "## Questions".into());
    let by_you = r
        .questions
        .iter()
        .filter(|q| q.is_answered && q.found.is_none())
        .count();
    push(
        &mut o,
        format!(
            "{}: {by_agent} found by your agent, {by_you} answered by you, {} open.",
            count(r.questions.len(), "question"),
            r.open.len()
        ),
    );
    let mut qs = Vec::new();
    for q in &r.questions {
        let affects = if q.affects.is_empty() {
            String::new()
        } else {
            format!(" Affects: {}.", q.affects)
        };
        qs.push(match (&q.answer, &q.found) {
            (Some(ans), Some(found)) => format!("{}. {} {ans} (Your agent: {found}.){affects}", q.i, q.q),
            (Some(ans), None) => format!("{}. {} {ans} (Your answer.){affects}", q.i, q.q),
            _ => format!(
                "{}. {} **Not answered.** It lowers confidence; your agent asks again on the next audit.{affects}",
                q.i, q.q
            ),
        });
    }
    push(&mut o, qs.join("\n"));

    // Audit
    push(
        &mut o,
        format!(
            "## {}",
            if new {
                "What the plan rests on"
            } else {
                "Everything the audit sent"
            }
        ),
    );
    push(
        &mut o,
        format!(
            "{} findings: {}.",
            r.findings_count,
            r.inventory
                .iter()
                .map(|g| format!("{} {}", g.items.len(), g.title.to_lowercase()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    for g in &r.inventory {
        push(&mut o, format!("### {} ({})", g.title, g.items.len()));
        push(
            &mut o,
            g.items.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n"),
        );
    }

    // Sources
    push(&mut o, "## Sources".into());
    push(&mut o, "Exodus knowledge base:".into());
    push(
        &mut o,
        r.sources
            .kb
            .iter()
            .map(|k| format!("- {site}/blog/{} (cited in {})", k.slug, k.cited.join(", ")))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    push(&mut o, format!("Google's price lists, read {}:", date(&r.prices.date)));
    push(
        &mut o,
        r.sources
            .google
            .iter()
            .map(|g| format!("- {} pricing: {}", g.what, g.doc))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let agent = p.agent.as_deref().map(|a| format!(" and {a}")).unwrap_or_default();
    push(
        &mut o,
        if new {
            format!(
                "How this plan was made: {} asked about the idea with exodus {} and sent {} facts ({} KB). The cost is computed by plain code from the price lists above; no model on our side wrote any of this plan.",
                p.agent.as_deref().unwrap_or("Your agent"),
                p.cli,
                r.findings_count,
                num(r.audit.kb)
            )
        } else {
            format!(
                "How this report was made: exodus {}{agent} read {} files of {} on your machine and found {} findings ({} KB); no code left it. The cost is computed by plain code from the price lists above; no model on our side wrote any of this report.",
                p.cli,
                num(r.audit.files as f64),
                p.name,
                r.findings_count,
                num(r.audit.kb)
            )
        },
    );
    o.join("\n").trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_dates_as_the_cabinet_prints_them() {
        assert_eq!(usd(4800), "$48.00");
        assert_eq!(usd(123_456), "$1,234.56");
        assert_eq!(usd(5), "$0.05");
        assert_eq!(price(0.000024), "$0.000024");
        assert_eq!(price(0.0105), "$0.0105");
        assert_eq!(price(0.1), "$0.10");
        assert_eq!(price(0.000024 * 1.4), "$0.0000336");
        assert_eq!(
            (price(3.3e-7), price(9.9e-7)),
            ("$0.00000033".into(), "$0.00000099".into())
        );
        assert_eq!(date("2026-10-11T08:40:00Z"), "Oct 11, 2026");
    }
}
