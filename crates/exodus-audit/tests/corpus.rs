//! The audit on the corpus of real repositories (tests/corpus/), against each hand-written expected.json
//! (`corpus/1`, tests/corpus/README.md). Ignored by default: it needs the clones `tests/corpus/fetch` makes.
//!
//!     tests/corpus/fetch
//!     cargo test -p exodus-audit -- --ignored corpus --nocapture
//!
//! It prints a table and the differences. A difference is explained when expected.json's `known` has its line
//! (`"MISSED break: in-process-timer a/b.py": "why"`); the test fails on a difference that is not explained, on a
//! `known` line that no longer occurs, and when a clone is missing. EXODUS_CORPUS_STRICT=1 fails on any difference,
//! explained or not. EXODUS_CORPUS is the clones' folder (default ~/.cache/exodus-corpus).
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn clones() -> PathBuf {
    std::env::var_os("EXODUS_CORPUS").map_or_else(
        || Path::new(&std::env::var_os("HOME").unwrap()).join(".cache/exodus-corpus"),
        PathBuf::from,
    )
}

/// The cases: (name, the folder to audit, expected.json).
fn cases() -> Vec<(String, PathBuf, PathBuf)> {
    let mut names: Vec<String> = std::fs::read_to_string(corpus().join("repos.txt"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next().map(String::from))
        .collect();
    names.sort();
    let mut cases: Vec<_> = names
        .into_iter()
        .map(|n| (n.clone(), clones().join(&n), corpus().join(&n).join("expected.json")))
        .collect();
    let s2 = corpus().join("synthetic/s2-shape");
    cases.push(("s2-shape".into(), s2.clone(), s2.join("expected.json")));
    cases
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_str).collect())
}

/// `rule file` pairs of the findings of one kind, the line number dropped.
fn found_pairs(patterns: &Value, kind: &str) -> BTreeSet<(String, String)> {
    patterns
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["kind"] == kind)
        .map(|p| {
            let at = p["at"].as_str().unwrap();
            (
                p["rule"].as_str().unwrap().into(),
                at.rsplit_once(':').map_or(at, |x| x.0).into(),
            )
        })
        .collect()
}

fn expected_pairs(list: &Value) -> BTreeSet<(String, String)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|b| (b["rule"].as_str().unwrap().into(), b["file"].as_str().unwrap().into()))
        .collect()
}

/// A kind and the engine's first letters, so "PostgreSQL 16" and "Postgres" are the same store.
fn store(kind: &str, engine: &str) -> (String, String) {
    let e: String = engine
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(5)
        .collect();
    // Valkey is a Redis; files and SQLite are one store each however they are named
    let e = if e == "valke" { "redis".into() } else { e };
    let e = if matches!(kind, "files" | "sqlite") {
        String::new()
    } else {
        e
    };
    (kind.into(), e)
}

/// What one case found against what it expects.
#[derive(Default)]
struct Diff {
    lines: Vec<String>,
    platform: String,
    ask: String,
    processes: (usize, usize),
    data: (usize, usize),
    missed: usize,
    false_breaks: (usize, usize),
    idle: (usize, usize),
    not_on_server: (usize, usize),
    stays: (usize, usize),
    /// Differences expected.json's `known` does not explain, and `known` lines that did not occur.
    unexplained: Vec<String>,
    stale: Vec<String>,
}

/// The `lines` that are differences from expected.json; the others are information.
const DIFFERENCES: [&str; 10] = [
    "platform",
    "process missing",
    "process extra",
    "data missing",
    "data extra",
    "stays missing",
    "MISSED",
    "FALSE",
    "questions",
    "WIDE",
];

/// A difference as `known` writes it: without the trap note and trailing space.
fn key(line: &str) -> &str {
    line.strip_suffix(" (trap / not on the server)")
        .unwrap_or(line)
        .trim_end()
}

impl Diff {
    fn clean(&self) -> bool {
        self.platform == "ok"
            && self.ask == "ok"
            && self.processes == (0, 0)
            && self.data == (0, 0)
            && self.missed == 0
            && self.false_breaks == (0, 0)
            && self.idle == (0, 0)
            && self.stays.0 == self.stays.1
    }
}

fn compare(found: &Value, expected: &Value) -> Diff {
    let mut d = Diff::default();

    let (fp, ep) = (
        found["platform"]["name"].as_str().unwrap(),
        expected["platform"]["name"].as_str().unwrap(),
    );
    d.platform = if fp.eq_ignore_ascii_case(ep) {
        "ok".into()
    } else {
        format!("{fp}, expected {ep}")
    };
    let asks = expected["platform"]["ask"] == true || strings(&expected["questions"]).contains(&"platform");
    let asked = found["questions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|q| q["id"] == "platform");
    d.ask = match (asks, asked) {
        (true, false) => "not asked".into(),
        (false, true) => "asked, not expected".into(),
        _ => "ok".into(),
    };
    if d.platform != "ok" {
        d.lines.push(format!("platform: {}", d.platform));
    }
    if d.ask != "ok" {
        d.lines.push(format!("questions: platform {}", d.ask));
    }

    // processes by kind
    if std::env::var_os("EXODUS_CORPUS_VERBOSE").is_some() {
        for p in found["processes"].as_array().unwrap() {
            d.lines
                .push(format!("found process: {} {} [{}]", p["kind"], p["command"], p["from"]));
        }
    }
    for kind in ["web", "worker", "cron", "release"] {
        let count = |v: &Value| v.as_array().unwrap().iter().filter(|p| p["kind"] == kind).count();
        let (f, e) = (count(&found["processes"]), count(&expected["processes"]));
        if e > f {
            d.processes.0 += e - f;
            d.lines.push(format!("process missing: {} x {kind}", e - f));
        }
        if f > e {
            d.processes.1 += f - e;
            d.lines.push(format!("process extra: {} x {kind}", f - e));
        }
    }

    // data by store
    let stores = |v: &Value| -> Vec<(String, String)> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| store(x["kind"].as_str().unwrap(), x["engine"].as_str().unwrap()))
            .collect()
    };
    let (mut fd, ed) = (stores(&found["data"]), stores(&expected["data"]));
    // Firebase's stores are on Google already: the audit names them as staying, not as data to move.
    let stays: Vec<&str> = strings(&found["clean"])
        .into_iter()
        .filter_map(|c| c.strip_prefix("stays as it is: "))
        .collect();
    for (prefix, kind, engine) in [
        ("Firestore (", "db", "Firestore"),
        ("Firebase Realtime Database (", "db", "Firebase"),
        ("Cloud Storage for Firebase (", "files", ""),
    ] {
        if stays.iter().any(|s| s.starts_with(prefix)) {
            fd.push(store(kind, engine));
        }
    }
    for what in expected["stays"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["what"].as_str())
    {
        if stays.iter().any(|s| s.contains(what)) {
            d.stays.0 += 1;
        } else {
            d.lines.push(format!("stays missing: {what}"));
        }
        d.stays.1 += 1;
    }
    for s in ed.iter().collect::<BTreeSet<_>>() {
        if !fd.contains(s) {
            d.data.0 += 1;
            d.lines.push(format!("data missing: {} {}", s.0, s.1));
        }
    }
    for s in fd.iter().collect::<BTreeSet<_>>() {
        if !ed.contains(s) {
            d.data.1 += 1;
            d.lines.push(format!("data extra: {} {}", s.0, s.1));
        }
    }

    // breaks: missed, false (a trap when expected.json names it, or its file is under a notOnServer path)
    let (eb, ei) = (expected_pairs(&expected["breaks"]), expected_pairs(&expected["idle"]));
    let (fb, fi) = (
        found_pairs(&found["patterns"], "breaks"),
        found_pairs(&found["patterns"], "idle"),
    );
    let off: Vec<&str> = strings(&expected["notOnServer"]);
    let traps = expected_pairs(&expected["notBreaks"]);
    for (r, f) in eb.difference(&fb) {
        d.missed += 1;
        d.lines.push(format!("MISSED break: {r} {f}"));
    }
    for (r, f) in fb.difference(&eb) {
        let trap = traps.contains(&(r.clone(), f.clone())) || off.iter().any(|p| f.starts_with(p));
        if trap {
            d.false_breaks.0 += 1;
        } else {
            d.false_breaks.1 += 1;
        }
        d.lines.push(format!(
            "FALSE break: {r} {f}{}",
            if trap { " (trap / not on the server)" } else { "" }
        ));
    }
    for (r, f) in ei.difference(&fi) {
        d.idle.0 += 1;
        d.lines.push(format!("MISSED idle: {r} {f}"));
    }
    for (r, f) in fi.difference(&ei) {
        d.idle.1 += 1;
        let trap = traps.contains(&(r.clone(), f.clone())) || off.iter().any(|p| f.starts_with(p));
        d.lines.push(format!(
            "FALSE idle: {r} {f}{}",
            if trap { " (trap / not on the server)" } else { "" }
        ));
    }

    // not on the server: each trap under a notOnServer path is named in `clean`, by itself or a folder above it
    let named: Vec<&str> = strings(&found["clean"])
        .into_iter()
        .filter_map(|c| c.strip_prefix("not on the server: "))
        .filter_map(|c| c.rsplit_once(" (").map(|x| x.0))
        .collect();
    if !named.is_empty() {
        d.lines.push(format!("named not on the server: {}", named.join(", ")));
    }
    // a named folder above an expected place takes in what runs on the server beside it (a site's Dockerfile)
    for n in &named {
        if let Some(p) = off.iter().find(|p| p.len() > n.len() && p.starts_with(n)) {
            d.lines.push(format!("WIDE not on the server: {n} holds {p}"));
        }
    }
    for (_, f) in traps.iter().filter(|(_, f)| off.iter().any(|p| f.starts_with(p))) {
        d.not_on_server.1 += 1;
        if named.iter().any(|n| f.starts_with(n)) {
            d.not_on_server.0 += 1;
        }
    }

    let known: BTreeMap<&str, &str> = expected["known"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.as_str(), v.as_str()?))).collect())
        .unwrap_or_default();
    let found_diffs: Vec<&str> = d
        .lines
        .iter()
        .filter(|l| DIFFERENCES.iter().any(|p| l.starts_with(p)))
        .map(|l| key(l))
        .collect();
    d.unexplained = found_diffs
        .iter()
        .filter(|l| known.get(*l).is_none_or(|why| why.trim().is_empty()))
        .map(|l| l.to_string())
        .collect();
    d.stale = known
        .keys()
        .filter(|k| !found_diffs.contains(k))
        .map(|k| k.to_string())
        .collect();
    d
}

#[test]
#[ignore = "needs the clones of tests/corpus/fetch; run with --ignored corpus"]
fn corpus_against_expected() {
    let mut table = String::from(
        "| case | platform | platform asked | processes missing / extra | data missing / extra | breaks missed | false breaks (trap / other) | idle missed / false | stays named | not on the server named |\n|---|---|---|---|---|---|---|---|---|---|\n",
    );
    let (mut details, mut absent, mut differing) = (String::new(), Vec::new(), 0);
    let (mut unexplained, mut stale): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut total = Diff::default();
    for (name, root, exp) in cases() {
        if !root.is_dir() {
            absent.push(name);
            continue;
        }
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(&exp).unwrap()).unwrap();
        // every place a rule fires on, not the five a report lists
        let found =
            serde_json::to_value(exodus_audit::audit_with(&root, &exodus_audit::rules::builtin(), usize::MAX).unwrap())
                .unwrap();
        let d = compare(&found, &expected);
        let _ = writeln!(
            table,
            "| {name} | {} | {} | {} / {} | {} / {} | {} | {} ({} / {}) | {} / {} | {} of {} | {} of {} |",
            d.platform,
            d.ask,
            d.processes.0,
            d.processes.1,
            d.data.0,
            d.data.1,
            d.missed,
            d.false_breaks.0 + d.false_breaks.1,
            d.false_breaks.0,
            d.false_breaks.1,
            d.idle.0,
            d.idle.1,
            d.stays.0,
            d.stays.1,
            d.not_on_server.0,
            d.not_on_server.1,
        );
        if !d.lines.is_empty() {
            let _ = writeln!(details, "\n{name}\n  {}", d.lines.join("\n  "));
        }
        differing += usize::from(!d.clean());
        unexplained.extend(d.unexplained.iter().map(|l| format!("{name}: {l}")));
        stale.extend(d.stale.iter().map(|l| format!("{name}: {l}")));
        total.processes = (total.processes.0 + d.processes.0, total.processes.1 + d.processes.1);
        total.data = (total.data.0 + d.data.0, total.data.1 + d.data.1);
        total.missed += d.missed;
        total.false_breaks = (
            total.false_breaks.0 + d.false_breaks.0,
            total.false_breaks.1 + d.false_breaks.1,
        );
        total.idle = (total.idle.0 + d.idle.0, total.idle.1 + d.idle.1);
        total.stays = (total.stays.0 + d.stays.0, total.stays.1 + d.stays.1);
        total.not_on_server = (
            total.not_on_server.0 + d.not_on_server.0,
            total.not_on_server.1 + d.not_on_server.1,
        );
    }
    println!("\n{table}{details}");
    println!(
        "\ntotal: processes missing {} / extra {}, data missing {} / extra {}, breaks missed {}, false breaks {} (trap {} / other {}), idle missed {} / false {}, stays named {} of {}, not on the server named {} of {}; {differing} case(s) differ",
        total.processes.0,
        total.processes.1,
        total.data.0,
        total.data.1,
        total.missed,
        total.false_breaks.0 + total.false_breaks.1,
        total.false_breaks.0,
        total.false_breaks.1,
        total.idle.0,
        total.idle.1,
        total.stays.0,
        total.stays.1,
        total.not_on_server.0,
        total.not_on_server.1,
    );
    assert!(absent.is_empty(), "not fetched, run tests/corpus/fetch: {absent:?}");
    assert!(
        unexplained.is_empty(),
        "differences from expected.json that its `known` does not explain:\n{}",
        unexplained.join("\n")
    );
    assert!(
        stale.is_empty(),
        "`known` lines that no longer occur (remove them):\n{}",
        stale.join("\n")
    );
    if std::env::var_os("EXODUS_CORPUS_STRICT").is_some() {
        assert_eq!(differing, 0, "{differing} corpus case(s) differ from expected.json");
    }
}
