//! The findings' processes and data as parts: what each is today, what it becomes on Google Cloud, and what it would
//! use there in a month (the profiles), plus what it costs today from the bill or the platform's list prices.
// Note: one branch per kind of part, as the cabinet draws them; the profiles are data, the rest is one table each

use std::collections::BTreeMap;

use exodus_schema::{Findings, Firestore, Part, PlatformList, Use};

/// Seconds a scheduled run is taken to last when nothing says otherwise: a job that sends, cleans or syncs.
const RUN_SECONDS: f64 = 10.0;
/// A schedule that fires more often than hourly is a short pass (a poll, a queue scan): it is a request to the service,
/// priced like one of the app's requests, not a job that boots each time.
const PASSES_A_MONTH: f64 = 720.0;
/// Files written a month when nothing says otherwise.
const WRITES: f64 = 2000.0;

/// The language of the stack: the first word of its first badge ("Node 20" -> "Node").
pub fn language(stack: &[String]) -> &str {
    stack.first().and_then(|s| s.split(' ').next()).unwrap_or("")
}

/// A unit of traffic in the singular: "visitors" -> "visitor", "webhook calls" -> "webhook call".
pub fn one(traffic: &str) -> &str {
    traffic.strip_suffix('s').unwrap_or(traffic)
}

/// Requests a unit of traffic makes and seconds of 1 vCPU each request holds an instance, by language. These are
/// planning figures for a typical app of that stack; the visits answer and the sliders move the total, not these.
fn web_profile(lang: &str, traffic: &str) -> (f64, f64) {
    if traffic != "visitors" {
        return (1.0, 0.3); // a webhook call or an API call is one request
    }
    match lang {
        "Node" => (10.0, 0.35),
        "Ruby" => (8.0, 0.3),
        "Python" | "PHP" => (8.0, 0.25),
        "Java" | "Kotlin" => (8.0, 0.15),
        "Elixir" => (8.0, 0.08),
        "Go" => (8.0, 0.06),
        "Rust" => (6.0, 0.05),
        _ => (8.0, 0.25),
    }
}

/// The size of a container image in MB by language, for Artifact Registry.
fn image_mb(lang: &str) -> f64 {
    match lang {
        "Node" => 270.0,
        "Ruby" => 350.0,
        "Python" | "PHP" => 200.0,
        "Java" | "Kotlin" => 250.0,
        "Elixir" => 100.0,
        "Go" | "Rust" => 30.0,
        _ => 250.0,
    }
}

fn use_(sku: &str, qty: f64, per: Option<&str>, how: impl Into<String>) -> Use {
    Use {
        sku: sku.into(),
        qty,
        per: per.map(Into::into),
        how: how.into(),
    }
}

fn part(id: &str, kind: &str, name: &str, now: String, after: String, product: Option<&str>, uses: Vec<Use>) -> Part {
    Part {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        now,
        after,
        removed: None,
        today: None,
        since: None,
        product: product.map(Into::into),
        options: None,
        size_mb: None,
        uses,
    }
}

/// "1,300", "0.8", "2.5": a count with thousands separators and at most two decimals.
pub fn num(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    let whole = r.trunc() as i64;
    let digits = whole.abs().to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let frac = format!("{:.2}", (r - r.trunc()).abs());
    let frac = frac.trim_start_matches('0').trim_end_matches('0').trim_end_matches('.');
    format!("{}{grouped}{frac}", if r < 0.0 { "-" } else { "" })
}

/// How often a five-field cron line runs in a 30-day month; `None` when it cannot be read.
pub fn runs_a_month(cron: &str) -> Option<f64> {
    let f: Vec<&str> = cron.split_whitespace().collect();
    let [min, hour, dom, month, dow] = f.as_slice() else {
        return None;
    };
    let count = |field: &str, lo: u32, hi: u32| -> Option<f64> {
        let mut n = 0;
        for item in field.split(',') {
            let (range, step) = match item.split_once('/') {
                Some((r, s)) => (r, s.parse::<u32>().ok().filter(|s| *s > 0)?),
                None => (item, 1),
            };
            let (a, b) = match range {
                "*" => (lo, hi),
                r => match r.split_once('-') {
                    Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
                    None if step > 1 => (r.parse().ok()?, hi),
                    None => {
                        let v = r.parse().ok()?;
                        (v, v)
                    }
                },
            };
            n += (a..=b.min(hi)).step_by(step as usize).count();
        }
        Some(n as f64)
    };
    let days = match (*dom, *dow) {
        ("*", "*") => 30.0,
        ("*", d) => count(d, 0, 6)?.min(7.0) * 30.0 / 7.0,
        (d, "*") => count(d, 1, 31)?,
        (d, w) => (count(d, 1, 31)? + count(w, 0, 6)? * 30.0 / 7.0).min(30.0),
    };
    let months = if *month == "*" {
        1.0
    } else {
        count(month, 1, 12)? / 12.0
    };
    Some((count(min, 0, 59)? * count(hour, 0, 23)? * days * months).round())
}

/// The parts of the app in the cabinet's order: processes, scheduled work, data, then builds, variables and traffic.
/// `fs` gives the document reads and writes a request makes, for an app whose SQLite moves to Firestore.
pub fn parts(f: &Findings, traffic: &str, fs: &Firestore) -> Vec<Part> {
    let new = f.kind == "new";
    let lang = language(&f.stack);
    let planned = |s: String| if new { "Planned".to_string() } else { s };
    let mut out = Vec::new();
    let mut timers = Vec::new();

    for p in &f.processes {
        let fw = p.framework.clone().unwrap_or_default();
        let on = if p.always_on { "always on" } else { "sleeps when idle" };
        match p.kind.as_str() {
            "web" => {
                let (r, s) = web_profile(lang, traffic);
                let n = format!(
                    "{} request{} a {}",
                    num(r),
                    if r == 1.0 { "" } else { "s" },
                    one(traffic)
                );
                let how = format!("{n}, {} s of 1 vCPU and 0.5 GiB each", num(s));
                let now = if fw.is_empty() {
                    format!("web process, {on}")
                } else {
                    format!("{fw} web process, {on}")
                };
                out.push(part(
                    &p.id,
                    "web",
                    &p.name,
                    planned(now),
                    "Cloud Run service, request-based, scales to zero".into(),
                    Some("cloud-run"),
                    vec![
                        use_("run-cpu", r * s, Some("visit"), &how),
                        use_("run-mem", r * s * 0.5, Some("visit"), &how),
                        use_("run-req", r, Some("visit"), n),
                    ],
                ));
            }
            "worker" => {
                let jobs = format!("a job for every second {}", one(traffic));
                let now = if fw.is_empty() {
                    format!("worker process, {on}")
                } else {
                    format!("{fw} worker, {on}")
                };
                out.push(part(
                    &p.id,
                    "worker",
                    &p.name,
                    planned(now),
                    "Cloud Tasks queue calling the service's /tasks handler".into(),
                    Some("cloud-tasks"),
                    vec![
                        use_(
                            "run-cpu",
                            0.25,
                            Some("visit"),
                            format!("{jobs}, 0.5 s of 1 vCPU each, on the service"),
                        ),
                        use_("run-mem", 0.125, Some("visit"), "the same jobs at 0.5 GiB"),
                        use_("run-req", 0.5, Some("visit"), "one request a job"),
                        use_("tasks", 0.5, Some("visit"), "one task a job"),
                    ],
                ));
            }
            "cron" => {
                let schedules = p.schedules.as_deref().unwrap_or_default();
                let now = if p.command.is_empty() && f.platform.name == "Heroku" {
                    "Heroku Scheduler, jobs set in the dashboard".to_string()
                } else {
                    let what = if fw.is_empty() { "clock" } else { &fw };
                    match schedules.len() {
                        0 => format!("{what} process, schedules not in the repository"),
                        n => format!("{what} process, {}", count(n, "schedule")),
                    }
                };
                out.push(scheduled(
                    &p.id,
                    &p.name,
                    planned(now),
                    schedules.iter().map(|s| s.cron.as_str()),
                    lang,
                ));
            }
            _ => {} // a release step becomes a job of the deploy (its task), not a part of the month
        }
        if p.kind != "cron"
            && let Some(s) = p.schedules.as_deref().filter(|s| !s.is_empty())
        {
            let now = format!("{} inside the {} process", count(s.len(), "schedule"), p.name);
            timers.push(scheduled(
                &format!("{}-timers", p.id),
                &format!("{} timers", p.name),
                planned(now),
                s.iter().map(|s| s.cron.as_str()),
                lang,
            ));
        }
    }
    out.extend(timers);

    for d in &f.data {
        let size = d.size_mb.map(|mb| format!(", {} MB", num(mb as f64)));
        let version = d.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
        match d.kind.as_str() {
            // an app on SQLite moves to Firestore and is priced by its reads, writes and storage, with the free
            // quota; Cloud SQL's smallest instance would cost more than such an app (decision of 2026-10-13)
            "sqlite" => {
                let (r, _) = web_profile(lang, traffic);
                let n = format!(
                    "{} request{} a {}",
                    num(r),
                    if r == 1.0 { "" } else { "s" },
                    one(traffic)
                );
                let gib = d.size_mb.map(|mb| mb as f64 / 1024.0);
                let mut p = part(
                    &d.id,
                    "db",
                    &d.name,
                    planned("SQLite file on a volume".into()),
                    "Firestore, Standard edition, billed per read, write and GiB stored".into(),
                    Some("firestore"),
                    vec![
                        use_(
                            "firestore-read",
                            r * fs.reads_per_request,
                            Some("visit"),
                            format!("{n}, {} document reads each", num(fs.reads_per_request)),
                        ),
                        use_(
                            "firestore-write",
                            r * fs.writes_per_request,
                            Some("visit"),
                            format!("{n}, {} document writes each", num(fs.writes_per_request)),
                        ),
                        use_(
                            "firestore-storage",
                            gib.unwrap_or(1.0),
                            None,
                            gib.map_or_else(
                                || "the size is not in the repository; taken as 1 GiB".into(),
                                |g| format!("{} GiB, the SQLite file's size", num(g)),
                            ),
                        ),
                    ],
                );
                p.size_mb = d.size_mb;
                out.push(p);
            }
            "db" => {
                // Cloud SQL has no MariaDB: a MariaDB database moves to Cloud SQL for MySQL, which speaks its protocol
                let engine = if d.engine == "MySQL" || d.engine == "MariaDB" {
                    "MySQL"
                } else {
                    "PostgreSQL"
                };
                let now = format!("{}{version}{}", d.engine, size.clone().unwrap_or_default());
                let mut p = part(
                    &d.id,
                    "db",
                    &d.name,
                    planned(now),
                    if d.engine == "MariaDB" {
                        "Cloud SQL for MySQL (Cloud SQL has no MariaDB), db-f1-micro, 10 GB SSD".into()
                    } else {
                        format!("Cloud SQL for {engine}, db-f1-micro, 10 GB SSD")
                    },
                    Some("cloud-sql"),
                    vec![
                        use_("sql-f1", 730.0, None, "always on, 730 hours"),
                        use_("sql-ssd", 10.0, None, "10 GB, Cloud SQL's smallest disk"),
                    ],
                );
                p.options = Some(vec![
                    "cloud-sql".into(),
                    "firestore".into(),
                    "serverless-postgres".into(),
                ]);
                p.size_mb = d.size_mb;
                out.push(p);
            }
            "cache" => {
                let jobs = cache_jobs(f, &d.id);
                let now = if jobs.is_empty() {
                    format!("{}{version}", d.engine)
                } else {
                    format!("{}{version}: {}", d.engine, join(&jobs))
                };
                let removed = takes_over(f, &d.id);
                let mut p = part(&d.id, "cache", &d.name, planned(now), String::new(), None, vec![]);
                if removed.is_empty() {
                    p.after = "Memorystore for Redis, Basic, 1 GB; not priced in this report".into();
                } else {
                    p.after = "Nothing".into();
                    p.removed = Some(join(&removed));
                }
                out.push(p);
            }
            "files" => {
                let gb = d.size_mb.map_or(1.0, |mb| mb as f64 / 1000.0);
                let how = match d.size_mb {
                    Some(_) => format!("{} GB of files", num(gb)),
                    None => "1 GB of files, taken as the size".into(),
                };
                let now = format!("Files on {}'s disk{}", platform_or(f), size.unwrap_or_default());
                out.push(part(
                    &d.id,
                    "files",
                    &d.name,
                    planned(now),
                    "Cloud Storage bucket, Standard".into(),
                    Some("cloud-storage"),
                    vec![
                        use_("gcs", gb, None, how),
                        use_(
                            "gcs-write",
                            WRITES,
                            None,
                            "about 2,000 files written a month, taken as the count",
                        ),
                    ],
                ));
            }
            _ => {}
        }
    }

    let mb = image_mb(lang);
    out.push(part(
        "builds",
        "build",
        "builds",
        planned(format!("{} builds the image", platform_or(f))),
        "Artifact Registry, the last 3 images".into(),
        Some("artifact-registry"),
        vec![use_(
            "ar",
            3.0 * mb / 1000.0,
            None,
            format!("3 images of {} MB", num(mb)),
        )],
    ));
    let s = &f.settings;
    let secrets = s.secrets_guess;
    out.push(part(
        "variables",
        "secrets",
        "variables",
        planned(format!(
            "{}, {secrets} of them secrets",
            count(s.variables_total, "variable")
        )),
        if secrets == 0 {
            "No secrets; the variables stay plain".to_string()
        } else if secrets == s.variables_total {
            format!("Secret Manager, {}", count(secrets, "secret"))
        } else {
            format!("Secret Manager, {}; the rest stay plain", count(secrets, "secret"))
        },
        Some("secret-manager"),
        vec![use_(
            "secrets",
            secrets as f64,
            None,
            format!("{}, one version each", count(secrets, "secret")),
        )],
    ));
    out.push(part(
        "egress",
        "traffic",
        "egress",
        planned("Traffic out".into()),
        "Traffic out".into(),
        None,
        vec![use_("egress", 1.0, Some("egress"), "the egress assumption")],
    ));
    out
}

/// A part that runs on a schedule. Cloud Scheduler calls the service for a schedule that fires more often than hourly
/// (each run a short pass, priced as one of the app's requests) and starts a Cloud Run job for the others (each run
/// taken as `RUN_SECONDS`), so a poll every minute costs what its requests cost, not a minute of CPU each time.
fn scheduled<'a>(id: &str, name: &str, now: String, crons: impl Iterator<Item = &'a str>, lang: &str) -> Part {
    let crons: Vec<&str> = crons.collect();
    let (jobs, how) = if crons.is_empty() {
        (
            1,
            "the times are not in the repository; taken as one run a day".to_string(),
        )
    } else {
        (crons.len(), crons.join(", "))
    };
    let runs_of = |c: &&str| runs_a_month(c).unwrap_or(30.0);
    let (passes, runs): (Vec<&str>, Vec<&str>) = crons.iter().partition(|c| runs_of(c) > PASSES_A_MONTH);
    let pass_runs: f64 = passes.iter().map(runs_of).sum();
    let job_runs: f64 = if crons.is_empty() {
        30.0
    } else {
        runs.iter().map(runs_of).sum()
    };
    let (_, req_secs) = web_profile(lang, "visitors");
    let after = match (passes.is_empty(), job_runs > 0.0) {
        (true, _) => format!("Cloud Scheduler, {}, starting a Cloud Run job", count(jobs, "job")),
        (false, false) => format!("Cloud Scheduler, {}, calling the service", count(jobs, "job")),
        (false, true) => format!(
            "Cloud Scheduler, {}: the frequent ones call the service, the others start a Cloud Run job",
            count(jobs, "job")
        ),
    };
    let mut uses = vec![use_("scheduler", jobs as f64, None, how)];
    if pass_runs > 0.0 {
        let secs = pass_runs * req_secs;
        let pass = format!(
            "{} passes a month called on the service, {} s of 1 vCPU each, as one of its requests",
            num(pass_runs),
            num(req_secs)
        );
        uses.push(use_("run-cpu", secs, None, &pass));
        uses.push(use_("run-mem", secs * 0.5, None, "the same passes at 0.5 GiB"));
        uses.push(use_("run-req", pass_runs, None, "one request a pass"));
    }
    if job_runs > 0.0 {
        let secs = job_runs * RUN_SECONDS;
        uses.push(use_(
            "job-cpu",
            secs,
            None,
            format!(
                "{} job runs a month, each taken as {} s at 1 vCPU",
                num(job_runs),
                num(RUN_SECONDS)
            ),
        ));
        uses.push(use_("job-mem", secs * 0.5, None, "the same runs at 0.5 GiB"));
    }
    part(id, "cron", name, now, after, Some("scheduler"), uses)
}

/// What the cache holds, from the patterns that use it: the queue of a worker, the sessions.
fn cache_jobs(f: &Findings, cache: &str) -> Vec<String> {
    let mut jobs = Vec::new();
    for p in &f.patterns {
        let job = match p.rule.as_str() {
            "queue-worker" => {
                let fw = f
                    .processes
                    .iter()
                    .find(|x| x.id == p.part)
                    .and_then(|x| x.framework.clone());
                fw.map_or_else(|| "the job queue".to_string(), |fw| format!("the {fw} queue"))
            }
            "redis-sessions" if p.part == cache => "the sessions".into(),
            _ => continue,
        };
        if !jobs.contains(&job) {
            jobs.push(job);
        }
    }
    jobs
}

/// The clauses of the rules whose fix takes the cache's jobs elsewhere: a queue worker's, and the patterns on the cache.
fn takes_over(f: &Findings, cache: &str) -> Vec<String> {
    let rules = exodus_audit::rules::builtin();
    let mut out: Vec<String> = Vec::new();
    for p in &f.patterns {
        let on_cache = p.part == cache || p.rule == "queue-worker";
        let clause = rules
            .iter()
            .find(|r| r.file.id == p.rule)
            .and_then(|r| r.file.takes_over.clone());
        if let Some(c) = clause.filter(|_| on_cache)
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out
}

fn platform_or(f: &Findings) -> &str {
    if f.platform.name == "Unknown" {
        "the host"
    } else {
        &f.platform.name
    }
}

/// "3 schedules", "1 job".
pub fn count(n: usize, what: &str) -> String {
    let plural = if what.ends_with('s') { "es" } else { "s" };
    format!("{n} {what}{}", if n == 1 { "" } else { plural })
}

/// Each subject once, in the order first named, with how many places name it: "files on disk (2 places)".
pub fn tally(xs: &[String]) -> Vec<String> {
    let mut seen: Vec<(&String, usize)> = Vec::new();
    for x in xs {
        match seen.iter_mut().find(|(s, _)| *s == x) {
            Some((_, n)) => *n += 1,
            None => seen.push((x, 1)),
        }
    }
    seen.into_iter()
        .map(|(s, n)| if n == 1 { s.clone() } else { format!("{s} ({n} places)") })
        .collect()
}

/// "a, b and c".
pub fn join(xs: &[String]) -> String {
    match xs {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Where each part's Today comes from.
pub enum Today<'a> {
    /// The bill answer: cents per part id, and "other".
    Bill(&'a BTreeMap<String, i64>),
    /// The platform's list prices for the smallest paid sizes.
    List(&'a PlatformList),
    Unknown,
}

/// The line of a bill given as one total, or with a rest no part was named for: Today only, nothing on Cloud Run,
/// where the parts above price it.
pub fn bill_part(f: &Findings, bill: &BTreeMap<String, i64>) -> Option<Part> {
    let split = bill.keys().any(|k| k != "total" && k != "other");
    if !bill.contains_key("total") && !bill.contains_key("other") {
        return None;
    }
    let host = platform_or(f);
    let (name, now) = if split {
        ("rest of the bill", format!("What {host}'s bill does not split by part"))
    } else if host == "the host" {
        ("bill", "The host's bill, one total".into())
    } else {
        ("bill", format!("{host}'s bill, one total"))
    };
    Some(part(
        "bill",
        "bill",
        name,
        now,
        "Priced part by part above".into(),
        None,
        vec![],
    ))
}

/// Fills each part's `today` and returns the month's total, `None` while unknown. A bill names parts by id (egress
/// too); `total` is the whole bill, and what the named parts leave of it, or `other`, goes on the bill's own line.
pub fn price_today(parts: &mut [Part], f: &Findings, today: &Today, egress_gb: f64) -> Option<i64> {
    let cents = |x: f64| (x + 1e-6).round() as i64;
    if let Today::Bill(bill) = today {
        let named: i64 = parts.iter().filter_map(|p| bill.get(&p.id)).sum();
        let total = bill.get("total").copied();
        for p in parts.iter_mut() {
            p.today = match (p.kind.as_str(), bill.get(&p.id), total) {
                ("bill", ..) => {
                    Some(total.map_or_else(|| bill.get("other").copied().unwrap_or(0), |t| (t - named).max(0)))
                }
                (_, Some(c), _) => Some(*c),
                // a total that names no part leaves each part unknown, not free
                (_, None, Some(_)) => None,
                (_, None, None) => Some(0),
            };
        }
        return Some(parts.iter().filter_map(|p| p.today).sum());
    }
    for p in parts.iter_mut() {
        p.today = match today {
            Today::Unknown | Today::Bill(_) => None,
            Today::List(l) => Some(match p.kind.as_str() {
                "web" | "worker" => cents(l.process.cents),
                "cron" => {
                    let own = f.processes.iter().find(|x| x.id == p.id);
                    match own {
                        Some(x) if x.always_on && !x.command.is_empty() => cents(l.process.cents),
                        Some(_) => cents(l.scheduled.cents),
                        None => 0, // timers inside a process are paid with it
                    }
                }
                "db" if f.data.iter().any(|d| d.id == p.id && d.kind == "sqlite") => cents(l.disk_gb.cents),
                "db" => cents(l.db.cents),
                "cache" => cents(l.cache.cents),
                "files" => cents(l.disk_gb.cents * p.size_mb.map_or(1.0, |mb| mb as f64 / 1000.0)),
                "traffic" => cents(l.egress_gb.cents * egress_gb),
                _ => 0,
            }),
        };
    }
    parts.iter().map(|p| p.today).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poll_every_minute_is_priced_as_requests_and_a_daily_job_as_a_job() {
        let p = scheduled("t", "t", String::new(), ["* * * * *", "* * * * *"].into_iter(), "Rust");
        let cpu: f64 = p.uses.iter().filter(|u| u.sku == "run-cpu").map(|u| u.qty).sum();
        // 2 x 43,200 passes at 0.05 s: 4,320 vCPU-s, about $0.08 at list price, not two vCPU-minutes a run
        assert!((cpu - 4320.0).abs() < 1.0, "{cpu}");
        assert!(p.uses.iter().all(|u| u.sku != "job-cpu"));
        let d = scheduled("d", "d", String::new(), ["0 3 * * *"].into_iter(), "Rust");
        let job: f64 = d.uses.iter().filter(|u| u.sku == "job-cpu").map(|u| u.qty).sum();
        assert!((job - 300.0).abs() < 1.0, "{job}");
    }

    #[test]
    fn cron_lines_as_runs_a_month() {
        assert_eq!(runs_a_month("0 * * * *"), Some(720.0));
        assert_eq!(runs_a_month("0 2 * * *"), Some(30.0));
        assert_eq!(runs_a_month("0 8 * * 1"), Some(4.0));
        assert_eq!(runs_a_month("*/10 * * * *"), Some(4320.0));
        assert_eq!(runs_a_month("*/15 9-17 * * 1-5"), Some(771.0));
        assert_eq!(runs_a_month("0 0 1 * *"), Some(1.0));
        assert_eq!(runs_a_month("0 0 1 1 *"), Some(0.0));
        assert_eq!(runs_a_month("@daily"), None);
        assert_eq!(runs_a_month("0 * * *"), None);
    }

    #[test]
    fn numbers_as_the_cabinet_prints_them() {
        assert_eq!(num(19_500.0), "19,500");
        assert_eq!(num(0.8), "0.8");
        assert_eq!(num(0.35), "0.35");
        assert_eq!(num(1_234_567.891), "1,234,567.89");
        assert_eq!(num(0.0), "0");
    }
}
