//! Reports of the four sample repositories in examples/, built offline, against the snapshots in tests/snapshots/.
//! More cases: node-railway built again after its answers (report 2, with what changed since report 1), a new
//! product planned from scratch, and the corpus's synthetic s2-shape (a static site in a folder, Firebase Functions,
//! an iOS app) without answers and with a bill given as one total. A change to the report shows up here as a diff of the Markdown and the JSON;
//! when the change is meant, `UPDATE_SNAPSHOTS=1 cargo test -p exodus-report` rewrites the files, and the diff is
//! read before the commit.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use exodus_schema::{Answers, Findings, Report};

const AT: &str = "2026-10-11T12:00:00Z";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn audit(example: &str) -> Findings {
    let mut f = exodus_audit::audit(&root().join("examples").join(example)).unwrap();
    f.cli = "0.1.0".into();
    f.repo.owner = Some("example".into());
    f.repo.name = example.into();
    f
}

fn read<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/inputs").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn build(f: &Findings, answers: &Answers, prev: Option<&Report>, agent: Option<&str>) -> Report {
    let input = exodus_report::Input {
        findings: f,
        answers: &answers.answers,
        prev,
        at: AT,
        agent,
    };
    exodus_report::build(&input, &exodus_cost::bundled(), &exodus_cost::platforms()).unwrap()
}

/// Compares the report's JSON and Markdown with the snapshot pair `name.json`, `name.md`; returns the mismatches.
fn check(name: &str, r: &Report) -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let json = serde_json::to_string_pretty(r).unwrap() + "\n";
    let md = exodus_report::markdown(r, exodus_report::SITE, exodus_report::APP);
    let mut bad = Vec::new();
    for (ext, text) in [("json", json), ("md", md)] {
        let path = dir.join(format!("{name}.{ext}"));
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &text).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        if want != text {
            let line = want
                .lines()
                .zip(text.lines())
                .position(|(a, b)| a != b)
                .unwrap_or(want.lines().count().min(text.lines().count()));
            bad.push(format!(
                "{name}.{ext} differs at line {}:\n  snapshot: {}\n  built:    {}",
                line + 1,
                want.lines().nth(line).unwrap_or("<end>"),
                text.lines().nth(line).unwrap_or("<end>")
            ));
        }
    }
    bad
}

#[test]
fn the_four_examples_match_their_snapshots() {
    let none = Answers::default();
    let mut bad = Vec::new();
    for example in ["node-railway", "rails-heroku", "django-render", "go-fly"] {
        bad.extend(check(example, &build(&audit(example), &none, None, None)));
    }
    assert!(
        bad.is_empty(),
        "{}\nUPDATE_SNAPSHOTS=1 rewrites them when the change is meant",
        bad.join("\n")
    );
}

#[test]
fn answers_make_report_two_and_say_what_changed() {
    let f = audit("node-railway");
    let first = build(&f, &Answers::default(), None, Some("Claude Code"));
    let answers: Answers = read("node-railway.answers.json");
    // after the answers, the agent describes the project (`exodus describe`): report 2 opens with its words
    let mut f = f;
    f.understanding = Some(read("node-railway.understanding.json"));
    let second = build(&f, &answers, Some(&first), Some("Claude Code"));
    assert_eq!((second.n, second.today_basis.as_str()), (2, "bill"));
    assert_eq!(second.assumptions.visits, 2000);
    assert_eq!(
        second.cost.today,
        Some(4800),
        "the bill's parts and other add up to the answer's total"
    );
    let since = second.since.as_ref().unwrap();
    assert!(
        since.lines.iter().any(|l| l == "Visitors a day 500 -> 2,000"),
        "{:?}",
        since.lines
    );
    // a question first asked by report 2 and answered in it is not something report 2 learned since report 1
    let mut more = answers.clone();
    more.answers.push(exodus_schema::Answer {
        id: "dbgrowth".into(),
        q: "How fast does the database grow?".into(),
        answer: "About 1 GB a year.".into(),
        found: Some("the platform's metrics".into()),
        value: None,
        at: "2026-10-11T10:00:00Z".into(),
    });
    let third = build(&f, &more, Some(&first), Some("Claude Code"));
    let learned = |r: &Report| {
        r.since
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .find(|l| l.contains("answer"))
            .cloned()
    };
    assert_eq!(learned(&third), learned(&second));
    assert!(third.questions.iter().any(|q| q.id == "dbgrowth" && q.asked == 2));
    // the share line: none offline, private until shared, then the cabinet's /r/<token>
    let md = |r: &Report| exodus_report::markdown(r, exodus_report::SITE, exodus_report::APP);
    assert!(md(&first).contains("it has no share link"));
    let mut kept = first.clone();
    kept.token = Some("Tok123abcd".into());
    kept.shared = Some(false);
    assert!(md(&kept).contains("Private: only you can open it") && !md(&kept).contains("Tok123abcd"));
    kept.shared = Some(true);
    assert!(md(&kept).contains("Share link: https://app.exodus.elim.io/r/Tok123abcd"));
    assert_eq!(first.open.len(), 15);
    assert_eq!(second.open.len(), 15 - answers.answers.len());
    let bad = check("node-railway.2", &second);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// SPEC exodus-fix F9: the report opens with "Our understanding", the agent's three paragraphs over the parts the audit
/// found; without them it says the agent has not written them and shows only the parts. A new description is a new
/// version, the same one is not.
#[test]
fn the_report_opens_with_our_understanding() {
    let md = |r: &Report| exodus_report::markdown(r, exodus_report::SITE, exodus_report::APP);
    let section = |r: &Report| {
        let m = md(r);
        let at = m.find("## Our understanding").expect("the section");
        assert!(
            at < m.find("## Verdict").or(m.find("## Summary")).unwrap(),
            "it comes before the verdict"
        );
        m[at..].split("\n## ").next().unwrap().to_string()
    };
    let mut f = audit("node-railway");
    let bare = build(&f, &Answers::default(), None, None);
    assert!(bare.understanding.is_none());
    let s = section(&bare);
    assert!(s.contains("Your agent has not described the project yet"), "{s}");
    assert!(
        s.contains("- Runs on: Railway") && s.contains("- Processes: web (Express web process, always on)"),
        "{s}"
    );
    assert!(s.contains("- Data: Postgres (PostgreSQL); Redis"), "{s}");

    f.understanding = Some(read("node-railway.understanding.json"));
    let described = build(&f, &Answers::default(), Some(&bare), None);
    let s = section(&described);
    assert!(
        s.contains("**What it is.** node-railway is the back end") && s.contains("**How it runs today.**"),
        "{s}"
    );
    assert!(
        s.contains("**What we will do.**") && !s.contains("has not described"),
        "{s}"
    );
    assert_eq!(described.change, "Your agent described the project");

    // the same words again change nothing; other words are the next version
    let same = build(&f, &Answers::default(), Some(&described), None);
    assert!(same.since.as_ref().unwrap().lines.is_empty());
    f.understanding
        .as_mut()
        .unwrap()
        .plan
        .push_str(" It keeps one instance warm.");
    let again = build(&f, &Answers::default(), Some(&described), None);
    assert_eq!(again.change, "Your agent described the project again");

    // a new product's labels and its planned parts
    let new = build(
        &read("new-product.findings.json"),
        &read("new-product.answers.json"),
        None,
        None,
    );
    let s = section(&new);
    assert!(
        s.contains("**The idea.**") && s.contains("**What we will build.**"),
        "{s}"
    );
    assert!(s.contains("The parts, as planned:") && !s.contains("Runs on"), "{s}");
}

#[test]
fn a_new_product_gets_a_plan() {
    let f: Findings = read("new-product.findings.json");
    let answers: Answers = read("new-product.answers.json");
    let r = build(&f, &answers, None, Some("Codex"));
    assert_eq!((r.verdict.kind.as_str(), r.today_basis.as_str()), ("new", "unknown"));
    let names: Vec<&str> = r.phases.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["Scaffold", "Build the core", "Stand up on Google Cloud", "Launch"]
    );
    assert!(
        r.findings.iter().all(|x| x.task.is_some()),
        "every decision has its task"
    );
    let bad = check("new-product", &r);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert_eq!(r.sources.kb[1].slug, "planning-a-new-product-for-cloud-run");

    // an agent may leave a decision's `kb` empty: it cites nothing, and no source has an empty slug (a map key)
    let mut f = f;
    f.patterns[0].kb.clear();
    let r = build(&f, &answers, None, Some("Codex"));
    assert!(r.sources.kb.iter().all(|k| !k.slug.is_empty()), "{:?}", r.sources.kb);
    assert!(!exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP).contains("/blog/\n"));
}

#[test]
fn every_finding_points_at_a_task_that_fixes_it_and_numbers_add_up() {
    let none = Answers::default();
    for example in ["node-railway", "rails-heroku", "django-render", "go-fly"] {
        let r = build(&audit(example), &none, None, None);
        for x in &r.findings {
            let t = x.task.and_then(|i| r.tasks.iter().find(|t| t.i == i));
            assert!(t.is_some(), "{example}: finding {} has no task", x.id);
        }
        // the Markdown and the screen group breaks, then idle, then notes: the numbers run 1, 2, ... down the page
        let order: Vec<(u32, &str)> = r.findings.iter().map(|x| (x.i, x.kind.as_str())).collect();
        let rank = |k: &str| ["breaks", "idle"].iter().position(|x| *x == k).unwrap_or(2);
        assert!(
            order.iter().enumerate().all(|(n, (i, _))| *i as usize == n + 1)
                && order.windows(2).all(|w| rank(w[0].1) <= rank(w[1].1)),
            "{example}: findings numbered out of the page's order {order:?}"
        );
        let phases: Vec<u8> = r.tasks.iter().map(|t| t.phase).collect();
        assert!(
            phases.windows(2).all(|w| w[0] <= w[1]),
            "{example}: tasks out of phase order {phases:?}"
        );
        assert_eq!(
            r.cost.cloud_run,
            r.cost.lines.iter().map(|l| l.cents).sum::<i64>(),
            "{example}"
        );
        for l in &r.cost.lines {
            assert_eq!(
                l.cents,
                l.uses.iter().map(|u| u.cents).sum::<i64>(),
                "{example}: {}",
                l.part.id
            );
        }
        assert_eq!(
            r.cost.today,
            r.cost.lines.iter().map(|l| l.part.today).sum::<Option<i64>>(),
            "{example}"
        );
        assert_eq!(
            r.findings_count as usize,
            r.inventory.iter().map(|g| g.items.len()).sum::<usize>()
        );
        let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
        assert!(!md.contains('\u{2014}'), "{example}: an em dash in the Markdown");
        assert!(
            md.contains(&format!("You can move {example} to Cloud Run after")),
            "{example}: {}",
            r.verdict.title
        );
    }
}

#[test]
fn a_new_product_is_priced_in_the_region_of_who_and_users_wins_over_who() {
    // templates/new.md records the region on `who`; a move records it on `users`
    let f: Findings = read("new-product.findings.json");
    let mut answers: Answers = read("new-product.answers.json");
    let who = answers.answers.iter_mut().find(|a| a.id == "who").unwrap();
    who.answer = "Studio owners and their clients, mostly in Germany.".into();
    who.value = Some("europe-west3".into());
    let r = build(&f, &answers, None, Some("Codex"));
    assert_eq!(r.assumptions.region, "europe-west3");
    let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
    assert!(md.contains("europe-west3 (Frankfurt)"), "{md}");
    let bad = check("new-product-who-region", &r);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    // both answered: `users` decides; a region the prices do not hold falls through to the next answer
    answers.answers.push(exodus_schema::Answer {
        id: "users".into(),
        q: "Where are most of your users?".into(),
        answer: "Mostly in Finland.".into(),
        found: None,
        value: Some("europe-north1".into()),
        at: "2026-10-11T10:06:00Z".into(),
    });
    assert_eq!(build(&f, &answers, None, None).assumptions.region, "europe-north1");
    answers.answers.last_mut().unwrap().value = Some("mars-north1".into());
    assert_eq!(build(&f, &answers, None, None).assumptions.region, "europe-west3");
}

#[test]
fn a_site_in_a_folder_with_firebase_functions_and_an_app_reads_as_its_parts() {
    // the synthetic shape of the corpus: a static site in website/, Firebase Functions, an iOS app, no host named
    let mut f = exodus_audit::audit(&root().join("tests/corpus/synthetic/s2-shape")).unwrap();
    f.cli = "0.1.0".into();
    assert!(f.questions.iter().any(|q| q.id == "platform"));
    let r = build(&f, &Answers::default(), None, None);
    assert_eq!(r.today_basis, "unknown");
    assert_eq!(
        r.stays,
        [
            "functions/ (Firebase Functions 2nd gen, already on Cloud Run functions)",
            "Firestore (functions/index.js)"
        ]
    );
    assert!(r.not_on_server.iter().any(|p| p == "ios/ (mobile app)"));
    assert!(r.clean.iter().all(|c| !c.contains(" (")));
    let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
    assert!(md.contains("Stays as it is, so not counted: functions/"));

    // the agent answers `platform` from the host's CLI: the Today column prices that host's list
    let answers: Answers = serde_json::from_value(serde_json::json!({
        "exodus": "answers/1",
        "answers": [{ "id": "platform", "q": "Where does the app run now?", "answer": "Railway, one service",
                      "found": "railway status", "value": "Railway", "at": AT }]
    }))
    .unwrap();
    assert_eq!(build(&f, &answers, None, None).today_basis, "list");

    // SPEC F6, the texts S2 got: no "Unknown" as a host, no empty stack line, no "-0.0", no "0 processes billed"
    for bad in [
        "Unknown",
        "Read from the repository: .",
        "-0.0",
        "0 processes",
        "A Your image",
        // a Caddy static site is a process that runs all day
        "no process found that is billed every hour",
        "railway.json",
    ] {
        assert!(!md.contains(bad), "{bad:?} in the report");
    }
    assert!(md.contains("From the current host to Google Cloud Run"));
    let mut bad = check("s2-shape", &r);

    // the bill as one total is a Today line of its own, not egress's, and the parts it does not name are unknown
    let answers: Answers = read("s2-shape.answers.json");
    let r = build(&f, &answers, None, Some("Claude Code"));
    assert_eq!((r.today_basis.as_str(), r.cost.today), ("bill", Some(500)));
    let today = |kind: &str| {
        r.cost
            .lines
            .iter()
            .find(|l| l.part.kind == kind)
            .map(|l| (l.part.today, l.cents))
    };
    assert_eq!(today("bill"), Some((Some(500), 0)));
    assert_eq!(today("traffic").unwrap().0, None);
    let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
    assert!(
        md.contains("| bill | Railway's bill, one total | Priced part by part above | $5.00 | - |"),
        "{md}"
    );
    assert!(md.contains("Today: your bill as one total"));
    // "Read by your agent" names the question, not what it affects
    let read_by = &r
        .inventory
        .iter()
        .find(|g| g.title == "Read by your agent")
        .unwrap()
        .items;
    assert!(
        read_by[0].starts_with("Where does the app run now? Railway"),
        "{read_by:?}"
    );
    bad.extend(check("s2-shape.answered", &r));
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn a_bill_split_by_part_puts_what_it_does_not_name_on_its_own_line() {
    let f = audit("node-railway");
    let bill = |v: serde_json::Value| -> Answers {
        serde_json::from_value(serde_json::json!({
            "exodus": "answers/1",
            "answers": [{ "id": "bill", "q": "What does Railway bill you?", "answer": "as the value", "value": v, "at": AT }]
        }))
        .unwrap()
    };
    let line = |r: &Report, id: &str| r.cost.lines.iter().find(|l| l.part.id == id).and_then(|l| l.part.today);
    // `other` is the rest, not egress
    let r = build(&f, &bill(serde_json::json!({ "web": 1400, "other": 300 })), None, None);
    assert_eq!(
        (line(&r, "egress"), line(&r, "bill"), r.cost.today),
        (Some(0), Some(300), Some(1700))
    );
    // a total with some parts named: the rest of the total is the bill's line, the unnamed parts unknown
    let r = build(&f, &bill(serde_json::json!({ "web": 1400, "total": 4000 })), None, None);
    assert_eq!(
        (line(&r, "web"), line(&r, "bill"), r.cost.today),
        (Some(1400), Some(2600), Some(4000))
    );
    assert_eq!(line(&r, "egress"), None);
    // every part named: no bill line
    let r = build(&f, &bill(serde_json::json!({ "web": 1400 })), None, None);
    assert!(r.cost.lines.iter().all(|l| l.part.kind != "bill"));
}

#[test]
fn the_year_grows_egress_and_files_with_the_visitors() {
    // S1: visitors grew 48x in a year and the cost stayed the same, because egress and storage stood still
    let f = audit("node-railway");
    let r = build(&f, &Answers::default(), None, None);
    let year = |r: &Report, g: f64| exodus_report::forecast(r, g).pop().unwrap();
    let flat = year(&r, 0.0);
    assert_eq!(flat.2, r.cost.cloud_run, "no growth, no change");
    let (_, visits, cents) = year(&r, 38.0);
    assert!(visits > 20 * r.assumptions.visits, "{visits}");
    // egress alone at the launch rate: 5 GB for 500 a day, so about 0.33 MB a visit
    let (egress, files) = exodus_report::rates(&r);
    assert!(
        (egress - 1.0 / 3.0).abs() < 1e-9 && (files - 2.0).abs() < 1e-9,
        "{egress} {files}"
    );
    let egress_year = 5.0 * visits as f64 / 500.0 * 0.12 * 100.0;
    assert!(
        cents - r.cost.cloud_run > egress_year as i64,
        "{cents} vs {}",
        r.cost.cloud_run
    );
    // an answer sets a rate: no egress per visit keeps egress where it is
    let answers: Answers = serde_json::from_value(serde_json::json!({
        "exodus": "answers/1",
        "answers": [{ "id": "egressmb", "q": "q", "answer": "none", "value": 0, "at": AT }]
    }))
    .unwrap();
    let quiet = build(&f, &answers, None, None);
    assert_eq!(quiet.assumptions.egress_mb_per_visit, Some(0.0));
    assert!(year(&quiet, 38.0).2 < cents);
    let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
    assert!(
        md.contains("As the visitors grow, these lines grow with them: web, worker, uploads and egress."),
        "{md}"
    );
}

/// The decision of 2026-10-13: an app on SQLite moves to Firestore, priced by its reads, writes and storage less the
/// free quota, and Cloud SQL is neither priced nor named for it; Postgres and MySQL stay on Cloud SQL.
#[test]
fn an_app_on_sqlite_moves_to_firestore_and_no_cloud_sql_is_priced() {
    let f = audit("go-fly");
    let r = build(&f, &Answers::default(), None, None);
    let db = r.cost.lines.iter().find(|l| l.part.kind == "db").unwrap();
    assert_eq!(db.part.product.as_deref(), Some("firestore"));
    assert!(db.part.options.is_none());
    let skus: Vec<&str> = db.uses.iter().map(|u| u.sku.as_str()).collect();
    assert_eq!(skus, ["firestore-read", "firestore-write", "firestore-storage"]);
    // Go's 8 requests a visitor at 2 reads and 0.2 writes each: 8,000 reads and 800 writes a day for 500 visitors,
    // inside the free 50,000 and 20,000; the size is unknown, so 1 GiB, the free GiB
    let qty: Vec<f64> = db.uses.iter().map(|u| u.qty.round()).collect();
    assert_eq!(qty, [240_000.0, 24_000.0, 1.0]);
    assert_eq!(db.cents, 0);
    assert!(
        r.cost
            .lines
            .iter()
            .flat_map(|l| &l.uses)
            .all(|u| !u.sku.starts_with("sql-"))
    );
    let md = exodus_report::markdown(&r, exodus_report::SITE, exodus_report::APP);
    for gone in ["Cloud SQL", "pgloader", "db-f1-micro"] {
        assert!(!md.contains(gone), "{gone:?} in the report");
    }
    assert!(
        md.contains("| cards_data | SQLite file on a volume | Firestore, Standard edition"),
        "{md}"
    );
    assert!(md.contains("Infrastructure: service, Firestore, scheduler"), "{md}");
    assert!(
        md.contains("Firestore pricing: https://cloud.google.com/firestore/pricing"),
        "{md}"
    );
    let task = r.tasks.iter().find(|t| t.title == "SQLite to Firestore").unwrap();
    assert!(r.findings.iter().any(|x| x.task == Some(task.i)));

    // at 20,000 visitors a day the reads and writes pass the free quota: 320,000 reads and 32,000 writes a day
    let busy: Answers = serde_json::from_value(serde_json::json!({
        "exodus": "answers/1",
        "answers": [{ "id": "visits", "q": "Visitors a day?", "answer": "20,000", "value": 20000, "at": AT }]
    }))
    .unwrap();
    let r = build(&f, &busy, None, None);
    let db = r.cost.lines.iter().find(|l| l.part.kind == "db").unwrap();
    // (9,600,000 - 1,500,000) x $0.0000033 = $2.67; (960,000 - 600,000) x $0.0000099 = $0.36
    let cents: Vec<i64> = db.uses.iter().map(|u| u.cents).collect();
    assert_eq!(cents, [267, 36, 0]);
    assert!(exodus_report::growing(&r).contains(&"cards_data".to_string()));

    // a Postgres app keeps Cloud SQL and its other options
    let r = build(&audit("node-railway"), &Answers::default(), None, None);
    let db = r.cost.lines.iter().find(|l| l.part.kind == "db").unwrap();
    assert_eq!(db.part.product.as_deref(), Some("cloud-sql"));
    assert_eq!(db.part.options.as_ref().unwrap()[0], "cloud-sql");
}

#[test]
fn a_subject_found_in_two_places_is_named_once_with_its_count() {
    let mut f = audit("node-railway");
    let files = f.patterns.iter().find(|p| p.subject.is_some()).unwrap().clone();
    let mut again = files.clone();
    again.id = format!("{}-again", files.id);
    again.at = "src/other.js:3".into();
    f.patterns.push(again);
    let r = build(&f, &Answers::default(), None, None);
    let subject = files.subject.unwrap();
    assert!(
        r.verdict.reason.contains(&format!("{} (2 places)", &subject[1..]))
            && r.verdict.reason.matches(&subject[1..]).count() == 1,
        "{}",
        r.verdict.reason
    );
}

/// Open connections the plan keeps (WebSockets, server-sent events) need CPU between requests: the web service is
/// priced as one instance on instance-based billing all month. Without them it stays request-based, at zero when idle.
#[test]
fn kept_open_connections_price_the_service_as_one_instance_always_on() {
    let mut f = audit("node-railway");
    let web = |r: &Report| r.cost.lines.iter().find(|l| l.part.kind == "web").cloned().unwrap();
    let before = web(&build(&f, &Answers::default(), None, None));
    assert_eq!(before.part.after, "Cloud Run service, request-based, scales to zero");
    assert!(before.uses.iter().all(|u| !u.sku.ends_with("-always")));

    let mut open = f.patterns[0].clone();
    open.id = "long-lived-connection".into();
    open.rule = "long-lived-connection".into();
    open.part = "web".into();
    f.patterns.push(open);
    let r = build(&f, &Answers::default(), None, None);
    let after = web(&r);
    assert!(
        after.part.after.contains("one instance always on"),
        "{}",
        after.part.after
    );
    let skus: Vec<&str> = after.uses.iter().map(|u| u.sku.as_str()).collect();
    assert_eq!(skus, ["run-cpu-always", "run-mem-always"]);
    // 30 days of 1 vCPU less the free 240,000 vCPU-s at $0.000018, and 0.5 GiB less 450,000 GiB-s at $0.000002
    assert_eq!(after.cents, 4234 + 169);
    assert!(r.day.always_on);
}
