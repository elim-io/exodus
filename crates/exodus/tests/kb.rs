//! `exodus kb` and `exodus ask` as the binary, against a fake platform on a local port: the key from EXODUS_KEY or
//! the credentials file, the message without one, passages with links, an article kept in .exodus/cache/kb, and
//! refusals that never print the key.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const KEY: &str = "exk_P1abcdefghijklmnopqr_0123456789abcdef0123456789abcdef";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exodus-test-kb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// The requests it saw, as `<request target> <authorization>`.
type Seen = Arc<Mutex<Vec<String>>>;

/// A platform that answers the knowledge base's routes for `KEY` and 401 for anything else.
fn platform() -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let target = first.split(' ').nth(1).unwrap_or("").to_string();
            let mut auth = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("authorization:") {
                    auth = line[line.len() - v.len()..].trim().to_string();
                }
            }
            log.lock().unwrap().push(format!("{target} {auth}"));
            let (status, body) = if auth != format!("Bearer {KEY}") {
                (
                    401,
                    r#"{"error":"unauthorized","message":"This key is not one Exodus knows."}"#.to_string(),
                )
            } else if target.starts_with("/api/kb/search?") {
                (200, serde_json::json!({ "hits": [
                    { "slug": "patterns", "title": "Patterns that break", "heading": "Workers", "url": "https://exodus.elim.io/blog/patterns#workers", "passage": "A worker that polls never sleeps.", "score": 2.5 },
                    { "slug": "costs", "title": "What it costs", "url": "https://exodus.elim.io/blog/costs", "passage": "Billed by the request.", "score": 1.0 }
                ] }).to_string())
            } else if target == "/api/kb/what-cloud-run-is" {
                (200, serde_json::json!({ "slug": "what-cloud-run-is", "title": "What Cloud Run is", "summary": "The short version.",
                    "checked": "2026-10-10", "minutes": 4, "body": "## Containers\n\nIt runs a container." }).to_string())
            } else if target.starts_with("/api/kb?") || target == "/api/kb" {
                (200, serde_json::json!({ "topics": [{ "id": "pricing", "name": "Pricing", "short": "Pricing", "count": 1 }],
                    "articles": [{ "slug": "what-cloud-run-costs", "title": "What Cloud Run costs", "topic": "pricing", "summary": "s", "checked": "2026-10-10", "minutes": 7 }] }).to_string())
            } else {
                (404, r#"{"error":"not_found","message":"Nothing here."}"#.to_string())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (url, seen)
}

/// The repository's project file, pointing at `api`.
fn project(dir: &Path, api: &str) {
    std::fs::create_dir_all(dir.join(".exodus")).unwrap();
    let p = serde_json::json!({ "project": "P1abcdefghijklmnopqr", "api": api, "name": "app", "kind": "move", "cli": "0.0.0" });
    std::fs::write(dir.join(".exodus/project.json"), p.to_string()).unwrap();
}

/// Runs the binary in `dir` with its own HOME and, if given, EXODUS_KEY: (exit code, stdout, stderr).
fn exodus(dir: &Path, key: Option<&str>, args: &[&str]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exodus"));
    cmd.args(args)
        .current_dir(dir)
        .env("HOME", dir.join("home"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("EXODUS_KEY");
    if let Some(k) = key {
        cmd.env("EXODUS_KEY", k);
    }
    let out = cmd.output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    (out.status.code().unwrap(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn without_a_key_both_say_where_the_articles_are_and_call_nothing() {
    let dir = folder("nokey");
    let (url, seen) = platform();
    project(&dir, &url);
    for args in [
        &["kb", "what-cloud-run-is"][..],
        &["kb", "--list"],
        &["ask", "how do workers run"],
    ] {
        let (code, out, err) = exodus(&dir, None, args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty());
        assert!(
            err.contains("needs a linked account") && err.contains("https://exodus.elim.io/blog"),
            "{err}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn ask_prints_the_passages_with_their_links_using_the_credentials_key() {
    let dir = folder("ask");
    let (url, seen) = platform();
    project(&dir, "http://127.0.0.1:9");
    // the credentials entry of this project names the API, and wins over project.json's
    let creds = dir.join("home/.config/exodus");
    std::fs::create_dir_all(&creds).unwrap();
    let entry =
        format!("[[projects]]\nid = \"P1abcdefghijklmnopqr\"\nkey = \"{KEY}\"\napi = \"{url}\"\nname = \"app\"\n");
    std::fs::write(creds.join("credentials"), entry).unwrap();

    let (code, out, err) = exodus(&dir, None, &["ask", "what does a worker cost?", "-n", "2"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("1. Patterns that break - Workers\n   A worker that polls never sleeps.\n   https://exodus.elim.io/blog/patterns#workers\n"), "{out}");
    assert!(
        out.contains("2. What it costs\n   Billed by the request.\n   https://exodus.elim.io/blog/costs"),
        "{out}"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("/api/kb/search?q=what") && seen[0].contains("worker") && seen[0].contains("n=2"),
        "{}",
        seen[0]
    );
    assert!(seen[0].ends_with(&format!("Bearer {KEY}")));
    assert!(!out.contains(KEY) && !err.contains(KEY));
}

#[test]
fn kb_prints_an_article_keeps_a_copy_and_lists_a_topic() {
    let dir = folder("article");
    let (url, seen) = platform();
    project(&dir, &url);

    let (code, out, err) = exodus(&dir, Some(KEY), &["kb", "what-cloud-run-is"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("# What Cloud Run is\n\nThe short version.\n"), "{out}");
    assert!(
        out.contains("## Containers\n\nIt runs a container.")
            && out.contains("https://exodus.elim.io/blog/what-cloud-run-is")
    );
    let kept = std::fs::read_to_string(dir.join(".exodus/cache/kb/what-cloud-run-is.md")).unwrap();
    assert_eq!(kept, out);

    let (code, out, _) = exodus(&dir, Some(KEY), &["kb", "pricing"]);
    assert_eq!(code, 0);
    assert!(out.contains("what-cloud-run-costs") && out.contains("7 min"), "{out}");
    assert!(
        seen.lock()
            .unwrap()
            .last()
            .unwrap()
            .starts_with("/api/kb?topic=pricing ")
    );
    let (_, out, _) = exodus(&dir, Some(KEY), &["kb", "--list"]);
    assert!(out.starts_with("Topics:\n") && out.contains("Pricing (1)"), "{out}");

    let (code, _, err) = exodus(&dir, Some(KEY), &["kb", "nope"]);
    assert_eq!(code, 3);
    assert!(err.contains("no article nope"), "{err}");
    // a slug cannot leave the API's path
    let (code, _, _) = exodus(&dir, Some(KEY), &["kb", "../../me"]);
    assert_eq!(code, 3);
    assert!(seen.lock().unwrap().last().unwrap().starts_with("/api/kb/------me "));

    // the platform down: the copy kept is printed
    project(&dir, "http://127.0.0.1:9");
    let (code, out, err) = exodus(&dir, Some(KEY), &["kb", "what-cloud-run-is"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, kept);
    assert!(
        err.contains("GET http://127.0.0.1:9/api/kb/what-cloud-run-is") && err.contains("copy kept"),
        "{err}"
    );
    let (code, _, err) = exodus(&dir, Some(KEY), &["ask", "anything"]);
    assert_eq!(code, 2);
    assert!(err.contains("GET http://127.0.0.1:9/api/kb/search"), "{err}");
}

#[test]
fn a_refused_key_is_the_platforms_sentence_without_the_key() {
    let dir = folder("refused");
    let (url, _) = platform();
    project(&dir, &url);
    let wrong = "exk_P1abcdefghijklmnopqr_ffffffffffffffffffffffffffffffff";
    for args in [&["ask", "cron"][..], &["kb", "what-cloud-run-is"]] {
        let (code, out, err) = exodus(&dir, Some(wrong), args);
        assert_eq!(code, 3, "{args:?}");
        assert!(
            err.contains("the platform refused (401): This key is not one Exodus knows."),
            "{err}"
        );
        assert!(!out.contains(wrong) && !err.contains(wrong) && !err.contains("ffffffff"));
    }
}
