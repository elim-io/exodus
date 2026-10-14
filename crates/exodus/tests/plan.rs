//! `exodus plan`, `task done`, `need`, `key`, `check` and `bill` as the binary: offline against the files, and with a
//! key against a fake platform on a local port. `bill` runs fake `gcloud` and `bq` that log every call, so the test
//! sees that only reads were asked for. No output ever holds the key.
// Note: one file for one fake platform and the commands of the plan and the move's end; a split would copy it
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

const PID: &str = "P1abcdefghijklmnopqr";
const KEY: &str = "exk_P1abcdefghijklmnopqr_0123456789abcdef0123456789abcdef";
const MINTED: &str = "exk_P1abcdefghijklmnopqr_c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";

/// What the fake answers, by "METHOD /path", and every request it saw: (method, path, authorization, body).
#[derive(Default)]
struct Fake {
    routes: HashMap<String, (u16, String)>,
    seen: Vec<(String, String, String, String)>,
}

type Shared = Arc<Mutex<Fake>>;

fn platform() -> (String, Shared) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let state: Shared = Arc::default();
    let st = state.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut parts = first.split(' ');
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").split('?').next().unwrap_or("").to_string();
            let (mut auth, mut length, mut json) = (String::new(), 0usize, false);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let (name, value) = line.split_once(':').unwrap_or(("", ""));
                match name.to_ascii_lowercase().as_str() {
                    "authorization" => auth = value.trim().to_string(),
                    "content-length" => length = value.trim().parse().unwrap(),
                    "content-type" => json = value.trim() == "application/json",
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut s = st.lock().unwrap();
            s.seen.push((
                method.clone(),
                path.clone(),
                auth.clone(),
                String::from_utf8(body).unwrap(),
            ));
            // as the API: a write is taken only as JSON (its CSRF rule), a body-less DELETE too
            let (status, out) = if method != "GET" && !json {
                (400, err("Send JSON with content-type: application/json."))
            } else if auth != format!("Bearer {KEY}") {
                (
                    401,
                    err("This key is not known; run the one line from the cabinet again to reconnect."),
                )
            } else {
                s.routes
                    .get(&format!("{method} {path}"))
                    .cloned()
                    .unwrap_or((404, err("Nothing here.")))
            };
            drop(s);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    (url, state)
}

fn err(message: &str) -> String {
    json!({ "error": "invalid", "message": message }).to_string()
}

fn route(state: &Shared, at: &str, status: u16, body: &str) {
    state.lock().unwrap().routes.insert(at.into(), (status, body.into()));
}

fn seen(state: &Shared) -> Vec<(String, String, String, String)> {
    std::mem::take(&mut state.lock().unwrap().seen)
}

/// `dir/shop`: the node-railway example as a git repository with one commit, and `dir/home` as HOME.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exodus-test-plan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/node-railway"),
        &dir.join("shop"),
    );
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(
        &dir,
        &[
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "start",
        ],
    );
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to.join(e.file_name()));
        } else if e.file_name() != "expected.json" {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir.join("shop"))
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Runs the binary in `dir/shop` with HOME at `dir/home` and `env`: (exit code, stdout, stderr).
fn exodus(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exodus"));
    cmd.args(args)
        .current_dir(dir.join("shop"))
        .env("HOME", dir.join("home"))
        .env_remove("XDG_CONFIG_HOME")
        // on Windows the credentials go under %APPDATA%: point it at the test's home so the paths match
        .env("APPDATA", dir.join("home/.config"))
        .env_remove("EXODUS_KEY")
        .env_remove("GITHUB_SHA")
        .env_remove("CI_COMMIT_SHA");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (o, e) = (text(&out.stdout), text(&out.stderr));
    assert!(
        !o.contains(&KEY[25..]) && !e.contains(&KEY[25..]),
        "{args:?} printed the key:\n{o}\n{e}"
    );
    (out.status.code().unwrap(), o, e)
}

fn plan_md(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("shop/.exodus/plan.md")).unwrap()
}

/// `.exodus/project.json` pointed at the fake, and a credentials entry for it.
fn link(dir: &Path, api: &str) {
    let path = dir.join("shop/.exodus/project.json");
    let mut p: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    p["project"] = PID.into();
    p["api"] = api.into();
    std::fs::write(&path, p.to_string()).unwrap();
    let creds = dir.join("home/.config/exodus");
    std::fs::create_dir_all(&creds).unwrap();
    std::fs::write(
        creds.join("credentials"),
        format!("[[projects]]\nid = \"{PID}\"\nkey = \"{KEY}\"\napi = \"{api}\"\nname = \"shop\"\ndevice = \"t\"\n"),
    )
    .unwrap();
}

fn audited(dir: &Path) {
    for args in [&["init"][..], &["audit", "--quiet"], &["report", "--offline"]] {
        let (code, o, e) = exodus(dir, &[], args);
        assert_eq!(code, 0, "{args:?}: {o}{e}");
    }
}

#[test]
fn plan_task_done_and_need_work_offline_on_plan_md() {
    let dir = folder("offline");
    let (code, _, e) = exodus(&dir, &[], &["plan"]);
    assert_eq!(code, 2, "no report and no account: {e}");
    audited(&dir);

    let (code, o, _) = exodus(&dir, &[], &["plan"]);
    assert_eq!(code, 0);
    assert!(o.contains("10 tasks from report 1") && o.contains("Not linked"), "{o}");
    let md = plan_md(&dir);
    assert!(
        md.starts_with("# Plan for shop, from report 1 (exodus plan, 20"),
        "{md}"
    );
    assert!(
        md.contains("- [ ] 1. Dockerfile and the health check (S, Prepare)\n"),
        "{md}"
    );
    assert!(
        md.contains("- [ ] 2. Timers to Cloud Scheduler (M, Change the code) - fixes finding 1\n"),
        "{md}"
    );
    let (code, _, e) = exodus(&dir, &[], &["plan", "--report", "2"]);
    assert_eq!(code, 2, "report 2 is not here and there is no account: {e}");

    // a request under task 3, with a line to copy; a second one replaces it
    let (code, ..) = exodus(&dir, &[], &["need", "Old request", "--task", "5"]);
    assert_eq!(code, 0);
    let (code, o, e) = exodus(
        &dir,
        &[],
        &[
            "need",
            "A Google Cloud project with billing",
            "--task",
            "3",
            "--step",
            "Create it|gcloud projects create shop-prod | tee id",
            "--step",
            "Link billing in the console",
        ],
    );
    assert_eq!(code, 0, "{o}{e}");
    let md = plan_md(&dir);
    assert!(
        md.contains("  > Needs you: A Google Cloud project with billing\n  > 1. Create it: `gcloud projects create shop-prod | tee id`\n  > 2. Link billing in the console\n- [ ] 4."),
        "{md}"
    );
    assert!(!md.contains("Old request"), "one request at a time: {md}");
    for (args, says) in [
        (vec!["need", "x", "--task", "99"], "task 99 is not in"),
        (vec!["need", "two\nlines", "--task", "2"], "one line"),
        (vec!["need", "--task", "2"], "required"),
    ] {
        let (code, _, e) = exodus(&dir, &[], &args);
        assert!(code != 0 && e.contains(says), "{args:?}: {e}");
    }
    let many: Vec<String> = (0..13)
        .flat_map(|k| ["--step".to_string(), format!("step {k}")])
        .collect();
    let mut args = vec!["need", "x", "--task", "2"];
    args.extend(many.iter().map(String::as_str));
    let (code, _, e) = exodus(&dir, &[], &args);
    assert!(code == 1 && e.contains("at most 12"), "{e}");

    // task done: the commit must be hex and in this repository; done closes the request on it
    let head = git(&dir, &["rev-parse", "HEAD"]);
    for (args, says) in [
        (vec!["task", "done", "3", "--commit", "zzzzzzz"], "7 to 40 hex"),
        (
            vec!["task", "done", "3", "--commit", "abcdef1234"],
            "not a commit of this repository",
        ),
        (vec!["task", "done", "42", "--commit", &head], "task 42 is not in"),
    ] {
        let (code, _, e) = exodus(&dir, &[], &args);
        assert!(code == 1 && e.contains(says), "{args:?}: {e}");
    }
    let (code, o, _) = exodus(&dir, &[], &["task", "done", "3", "--commit", &head.to_uppercase()]);
    assert!(code == 0 && o.contains("the cabinet's plan will not tick"), "{o}");
    let md = plan_md(&dir);
    let short = &head[..7];
    assert!(
        md.contains("- [x] 3. Sessions to signed cookies (S, Change the code) - fixes finding 2 - done 20"),
        "{md}"
    );
    assert!(
        md.contains(&format!(", {short}\n")) && !md.contains("Needs you"),
        "{md}"
    );
    let (code, _, e) = exodus(&dir, &[], &["need", "x", "--task", "3"]);
    assert!(code == 1 && e.contains("done already"), "{e}");
    // done again: one line still, the same date
    let (code, ..) = exodus(&dir, &[], &["task", "done", "3", "--commit", &head]);
    assert_eq!(code, 0);
    assert_eq!(plan_md(&dir).matches("- [x]").count(), 1);
    assert_eq!(plan_md(&dir).matches(" - done ").count(), 1);

    let (code, _, e) = exodus(&dir, &[], &["plan"]);
    assert!(
        code == 1 && e.contains("1 of 10 tasks are done") && e.contains("--force"),
        "{e}"
    );
    let (code, ..) = exodus(&dir, &[], &["plan", "--force"]);
    assert_eq!(code, 0);
    assert_eq!(plan_md(&dir).matches("- [ ]").count(), 10);
    let (_, o, _) = exodus(&dir, &[], &["status"]);
    assert!(o.contains("plan.md: 0 of 10 tasks done"), "{o}");
}

#[test]
fn plan_task_done_and_need_tell_the_platform_with_a_key() {
    let dir = folder("linked");
    audited(&dir);
    let (url, state) = platform();
    link(&dir, &url);
    // the platform's report 1 is the one the plan follows: the copy built here is replaced by it
    let mut report: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("shop/.exodus/report.json")).unwrap()).unwrap();
    report["token"] = "tok".into();
    route(&state, "GET /api/cli/reports/1", 200, &report.to_string());
    route(&state, "GET /api/cli/reports/1/report.md", 200, "# report 1\n");
    route(
        &state,
        "POST /api/cli/plan",
        400,
        &err("report: no report yet; exodus send makes report 1"),
    );
    let (code, _, e) = exodus(&dir, &[], &["plan"]);
    assert!(code == 3 && e.contains("exodus send makes report 1"), "{e}");
    assert!(
        !dir.join("shop/.exodus/plan.md").exists(),
        "a refused plan writes no plan.md"
    );
    let calls: Vec<String> = seen(&state).iter().map(|s| format!("{} {}", s.0, s.1)).collect();
    assert_eq!(
        calls,
        [
            "GET /api/cli/reports/1",
            "GET /api/cli/reports/1/report.md",
            "POST /api/cli/plan"
        ]
    );

    route(&state, "POST /api/cli/plan", 200, r#"{"tasks":[]}"#);
    let (code, o, _) = exodus(&dir, &[], &["plan"]);
    assert!(code == 0 && o.contains("The cabinet's plan follows report 1"), "{o}");
    let s = seen(&state);
    assert_eq!(
        s.len(),
        1,
        "report.json is the platform's now, so nothing is pulled: {s:?}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&s[0].3).unwrap(),
        json!({ "report": 1, "force": false })
    );
    assert_eq!(s[0].2, format!("Bearer {KEY}"));

    route(&state, "POST /api/cli/needs", 204, "");
    let (code, ..) = exodus(
        &dir,
        &[],
        &[
            "need",
            "A billing account",
            "--task",
            "2",
            "--step",
            "Open billing|https://console.cloud.google.com/billing",
        ],
    );
    assert_eq!(code, 0);
    let s = seen(&state);
    assert_eq!(
        serde_json::from_str::<Value>(&s[0].3).unwrap(),
        json!({ "task": 2, "title": "A billing account",
        "steps": [{ "text": "Open billing", "line": "https://console.cloud.google.com/billing" }] })
    );

    let head = git(&dir, &["rev-parse", "HEAD"]);
    route(&state, "POST /api/cli/tasks/2/done", 200, r#"{"done":1,"total":9}"#);
    let (code, o, _) = exodus(
        &dir,
        &[],
        &["task", "done", "2", "--commit", &head[..10].to_uppercase()],
    );
    assert!(
        code == 0 && o.contains("Task 2 done: 1 of 9 on the cabinet's plan"),
        "{o}"
    );
    let s = seen(&state);
    assert_eq!((s[0].0.as_str(), s[0].1.as_str()), ("POST", "/api/cli/tasks/2/done"));
    assert_eq!(
        serde_json::from_str::<Value>(&s[0].3).unwrap(),
        json!({ "commit": head[..10] })
    );

    // the file is ticked even when the platform does not answer, and the same command again tells it
    route(
        &state,
        "POST /api/cli/tasks/4/done",
        400,
        &err("task: 1 to 3 on this plan"),
    );
    let (code, _, e) = exodus(&dir, &[], &["task", "done", "4", "--commit", &head]);
    assert!(
        code == 3 && e.contains("1 to 3 on this plan") && e.contains("Run the same command again"),
        "{e}"
    );
    assert!(plan_md(&dir).contains("- [x] 4."));
    let mut p: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("shop/.exodus/project.json")).unwrap()).unwrap();
    p["api"] = "http://127.0.0.1:9".into();
    std::fs::write(dir.join("shop/.exodus/project.json"), p.to_string()).unwrap();
    let creds = dir.join("home/.config/exodus/credentials");
    std::fs::write(
        &creds,
        std::fs::read_to_string(&creds)
            .unwrap()
            .replace(&url, "http://127.0.0.1:9"),
    )
    .unwrap();
    let (code, _, e) = exodus(&dir, &[], &["task", "done", "5", "--commit", &head]);
    assert!(code == 2 && e.contains("http://127.0.0.1:9"), "{e}");
    assert!(plan_md(&dir).contains("- [x] 5."));

    let creds_text = std::fs::read_to_string(&creds)
        .unwrap()
        .replace("http://127.0.0.1:9", &url);
    std::fs::write(&creds, creds_text).unwrap();
    route(&state, "DELETE /api/cli/needs", 204, "");
    let (code, o, _) = exodus(&dir, &[], &["need", "--clear"]);
    assert!(code == 0 && o.contains("withdrawn"), "{o}");
    assert_eq!(seen(&state).last().map(|s| s.0.clone()), Some("DELETE".into()));

    // key --ci: a pipe gets the minted key (the test's stdout is one), nothing else does
    route(&state, "POST /api/cli/keys", 200, &json!({ "key": MINTED }).to_string());
    let (code, o, e) = exodus(&dir, &[], &["key", "--ci", "--name", "github"]);
    assert!(
        code == 0 && o == format!("{MINTED}\n") && e.contains("a key for github"),
        "{o}{e}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&seen(&state)[0].3).unwrap(),
        json!({ "device": "github", "os": "ci" })
    );
    std::fs::remove_file(&creds).unwrap();
    let (code, ..) = exodus(&dir, &[], &["key", "--ci"]);
    assert_eq!(code, 2);
}

#[test]
fn check_fails_on_what_breaks_and_tells_the_cabinet_with_exodus_key_only() {
    let dir = folder("check");
    audited(&dir);
    let (code, o, _) = exodus(&dir, &[], &["check"]);
    assert_eq!(code, 1, "{o}");
    assert!(
        o.contains("src/clock.js:9: breaks: Timers inside the process (in-process-timer)"),
        "{o}"
    );
    assert!(o.contains("4 found that fail the run (breaks, idle)"), "{o}");
    let (code, o, _) = exodus(&dir, &[], &["check", "--format", "github"]);
    assert_eq!(code, 1);
    assert!(
        o.lines()
            .any(|l| l.starts_with("::error file=src/clock.js,line=9,title=exodus%3A Timers inside the process::")),
        "{o}"
    );
    let (code, o, _) = exodus(&dir, &[], &["check", "--fail-on", "idle", "--format", "github"]);
    assert!(
        code == 0 && o.starts_with("::warning ") && o.contains("none of a kind that fails"),
        "{o}"
    );
    let (code, ..) = exodus(&dir, &[], &["check", "--fail-on", "nope"]);
    assert_eq!(code, 2, "clap refuses an unknown kind");

    // own rules: a folder of them, one replacing a built-in id; a broken one is named
    let rules = dir.join("rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(
        rules.join("console.toml"),
        "id = \"in-process-timer\"\nkind = \"idle\"\ntitle = \"Listening on a port\"\nkb = \"cloud-run-basics\"\npart = \"web\"\nwhy = \"w\"\nfix = \"f\"\ninclude = [\"**/*.js\"]\nregex = ['app\\.listen\\(']\n",
    )
    .unwrap();
    let (code, o, e) = exodus(&dir, &[], &["check", "--rules", rules.to_str().unwrap()]);
    assert_eq!(code, 1, "{e}");
    assert!(
        o.contains("idle: Listening on a port (in-process-timer)") && !o.contains("Timers inside the process"),
        "{o}"
    );
    std::fs::write(rules.join("broken.toml"), "id = \"x\"\nkind = \"maybe\"\n").unwrap();
    let (code, _, e) = exodus(&dir, &[], &["check", "--rules", rules.to_str().unwrap()]);
    assert!(code == 1 && e.contains("broken.toml"), "{e}");

    // the machine's credentials never report a run; EXODUS_KEY does, with the commit and the CI file
    let (url, state) = platform();
    link(&dir, &url);
    exodus(&dir, &[], &["check"]);
    assert!(seen(&state).is_empty(), "a local run with credentials is not a CI run");
    std::fs::create_dir_all(dir.join("shop/.github/workflows")).unwrap();
    std::fs::write(dir.join("shop/.github/workflows/a.yml"), "on: push\n").unwrap();
    std::fs::write(
        dir.join("shop/.github/workflows/ci.yml"),
        "steps:\n  - run: exodus check --format github\n",
    )
    .unwrap();
    route(&state, "POST /api/cli/check-runs", 204, "");
    let sha = "ABCDEF0123456789abcdef0123456789abcdef01";
    let (code, _, e) = exodus(&dir, &[("EXODUS_KEY", KEY), ("GITHUB_SHA", sha)], &["check"]);
    assert!(code == 1 && e.contains("the cabinet has this run (abcdef0)"), "{e}");
    let s = seen(&state);
    assert_eq!((s.len(), s[0].1.as_str()), (1, "/api/cli/check-runs"));
    let body: Value = serde_json::from_str(&s[0].3).unwrap();
    assert_eq!(body["commit"], sha.to_lowercase());
    assert_eq!(body["file"], ".github/workflows/ci.yml");
    assert_eq!(body["found"].as_array().unwrap().len(), 4);
    assert_eq!(
        body["found"][0],
        json!({ "rule": "in-process-timer", "at": "src/clock.js:9", "text": "Timers inside the process" })
    );
    // without GITHUB_SHA the commit is git's; a refusal is told on stderr and the exit code stays the check's
    route(
        &state,
        "POST /api/cli/check-runs",
        401,
        &err("This key was revoked; run the one line again to reconnect."),
    );
    let (code, _, e) = exodus(&dir, &[("EXODUS_KEY", KEY)], &["check", "--fail-on", "idle"]);
    assert!(code == 0 && e.contains("was revoked"), "{e}");
    let body: Value = serde_json::from_str(&seen(&state)[0].3).unwrap();
    assert_eq!(body["commit"], git(&dir, &["rev-parse", "HEAD"]));
}

/// Fake `gcloud` and `bq` in `dir/bin` that answer like the real ones for one account and log every call.
#[cfg(unix)] // fake gcloud and bq are shell scripts
fn fake_cloud(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = dir.join("calls.log");
    let gcloud = format!(
        r#"#!/bin/sh
echo "gcloud $*" | tr '\n' ' ' >> "{log}"; echo >> "{log}"
case "$*" in
  "config get-value project") echo shop-prod ;;
  "billing projects describe shop-prod --format=value(billingAccountName,billingEnabled)")
    printf 'billingAccounts/0AAAAA-BBBBBB-CCCCCC\tTrue\n' ;;
  "billing projects list --billing-account=0AAAAA-BBBBBB-CCCCCC --format=value(projectId)")
    printf 'shop-prod\nbilling-home\n' ;;
  *) echo "ERROR: unexpected" >&2; exit 1 ;;
esac
"#,
        log = log.display()
    );
    let bq = format!(
        r#"#!/bin/sh
echo "bq $*" | tr '\n' ' ' >> "{log}"; echo >> "{log}"
case "$1 $2" in
  "ls --project_id=shop-prod") echo "BigQuery error: the API is not enabled" >&2; exit 1 ;;
  "ls --project_id=billing-home") echo '[{{"datasetReference":{{"datasetId":"billing_export","projectId":"billing-home"}}}}]' ;;
  "ls --format=json")
    if [ -n "$NO_EXPORT" ]; then echo '[]'; else
    echo '[{{"tableReference":{{"tableId":"other"}}}},{{"tableReference":{{"tableId":"gcp_billing_export_v1_0AAAAA_BBBBBB_CCCCCC"}}}}]'; fi ;;
  query*)
    echo '[{{"service":"Cloud Run","sku_id":"4856-B847-F1EB","sku":"Services CPU (Request-based billing)","region":"europe-west1","unit":"second","currency":"'"${{CURRENCY:-USD}}"'","usage":"22703.5","cost":"0.544728","credits":"-0.544728"}},{{"service":"Cloud SQL","sku_id":"4C57-F0D3-6029","sku":"PostgreSQL: Zonal - Micro instance in EMEA","region":"europe-west1","unit":"hour","currency":"USD","usage":"720","cost":"7.56","credits":"0"}}]' ;;
  *) echo "unexpected" >&2; exit 1 ;;
esac
"#,
        log = log.display()
    );
    for (name, text) in [("gcloud", gcloud), ("bq", bq)] {
        std::fs::write(bin.join(name), text).unwrap();
        std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

#[test]
#[cfg(unix)]
fn bill_reads_the_export_read_only_and_sends_the_month() {
    let dir = folder("bill");
    audited(&dir);
    let bin = fake_cloud(&dir);
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let env = [("PATH", path.as_str())];

    let (code, o, e) = exodus(&dir, &env, &["bill", "--month", "2026-09"]);
    assert_eq!(code, 0, "{o}{e}");
    assert!(
        e.contains("2026-09 of shop-prod from billing-home.billing_export.gcp_billing_export_v1_0AAAAA_BBBBBB_CCCCCC"),
        "{e}"
    );
    assert!(
        o.contains("Services CPU (Request-based billing)") && o.contains("2 rows, net $7.56"),
        "{o}"
    );
    assert!(o.contains("Not linked"), "{o}");
    let log = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    // read-only: gcloud describes and lists, bq lists and runs one SELECT; nothing makes, copies or writes
    for call in log.lines() {
        let words: Vec<&str> = call.split(' ').collect();
        let read = match words[0] {
            "gcloud" => {
                call.starts_with("gcloud config get-value") || words.contains(&"describe") || words.contains(&"list")
            }
            _ => words[1] == "ls" || (words[1] == "query" && call.contains(" SELECT")),
        };
        assert!(
            read && !call.contains("destination_table") && !call.contains(" mk ") && !call.contains(" cp "),
            "{call}"
        );
    }
    let query = log.lines().find(|l| l.starts_with("bq query")).unwrap();
    for part in [
        "--project_id=billing-home",
        "--parameter=month:STRING:202609",
        "--parameter=project:STRING:shop-prod",
        "FROM `billing-home.billing_export.gcp_billing_export_v1_0AAAAA_BBBBBB_CCCCCC`",
    ] {
        assert!(query.contains(part), "{part} in {query}");
    }
    assert_eq!(log.lines().filter(|l| l.starts_with("bq query")).count(), 1);

    // with a key the month goes to the cabinet: the rows as read, no account id
    let (url, state) = platform();
    link(&dir, &url);
    route(
        &state,
        "POST /api/cli/bill",
        200,
        r#"{"actual":{"report":1,"parts":{"web":0,"db":756},"other":[{"cents":0}]}}"#,
    );
    let (code, o, e) = exodus(
        &dir,
        &env,
        &[
            "bill",
            "--month",
            "2026-09",
            "--table",
            "billing-home:billing_export.gcp_billing_export_v1_0AAAAA_BBBBBB_CCCCCC",
        ],
    );
    assert_eq!(code, 0, "{o}{e}");
    assert!(o.contains("The cabinet has 2026-09: $7.56 against report 1"), "{o}");
    let s = seen(&state);
    assert_eq!(s.len(), 1);
    assert!(!s[0].3.contains("0AAAAA"), "the billing account stays here: {}", s[0].3);
    let body: Value = serde_json::from_str(&s[0].3).unwrap();
    assert_eq!(body["month"], "2026-09");
    assert_eq!(
        body["rows"][0],
        json!({ "service": "Cloud Run", "skuId": "4856-B847-F1EB", "sku": "Services CPU (Request-based billing)",
        "region": "europe-west1", "usage": 22703.5, "unit": "second", "costCents": 54.4728, "creditsCents": -54.4728 })
    );
    let (code, o, _) = exodus(&dir, &env, &["bill", "--month", "2026-09", "--dry-run"]);
    assert!(code == 0 && o.contains("Dry run: nothing sent"), "{o}");
    assert!(seen(&state).is_empty());

    for (extra, args, says) in [
        (
            ("CURRENCY", "EUR"),
            vec!["bill", "--month", "2026-09"],
            "the bill is in EUR",
        ),
        (
            ("NO_EXPORT", "1"),
            vec!["bill", "--month", "2026-09"],
            "Billing export (console.cloud.google.com/billing/export)",
        ),
        (("X", "1"), vec!["bill", "--month", "2026-13"], "YYYY-MM"),
        (
            ("X", "1"),
            vec!["bill", "--month", "2026-09", "--table", "p`; drop.x.y"],
            "--table takes",
        ),
    ] {
        let (code, _, e) = exodus(&dir, &[env[0], extra], &args);
        assert!(code == 1 && e.contains(says), "{args:?}: {e}");
    }
    assert!(seen(&state).is_empty(), "nothing is sent when the month is not read");
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let (code, _, e) = exodus(&dir, &[("PATH", empty.to_str().unwrap())], &["bill"]);
    assert!(
        code == 1 && e.contains("cannot run gcloud") && e.contains("gcloud auth login"),
        "{e}"
    );
}
