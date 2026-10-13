//! The platform commands as the binary, against a fake platform on a local port: `init --code` and `login --code`
//! (the exchange, the 0600 credentials, the kind from the code, the folders a code is not spent in), `status`, `audit`
//! and `answer` telling the waiting screen, `send`, `report pull` and `report` with a key, and install.sh with a code.
//! No output ever holds the key.
// Note: one file for one fake platform and the commands and the installer that share it; a split would copy the fake
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const PID: &str = "P1abcdefghijklmnopqr";
const KEY: &str = "exk_P1abcdefghijklmnopqr_0123456789abcdef0123456789abcdef";
const KEY2: &str = "exk_P1abcdefghijklmnopqr_fedcba9876543210fedcba9876543210";

/// One request the fake saw.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    target: String,
    auth: String,
    body: String,
}

#[derive(Default)]
struct State {
    seen: Vec<Seen>,
    /// What `GET cli/reports/{2,latest}` answers.
    report: String,
    /// The kind the next good code carries.
    kind: String,
    /// Which key the next good code mints.
    key: String,
}

type Shared = Arc<Mutex<State>>;

fn platform() -> (String, Shared) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let state: Shared = Arc::new(Mutex::new(State {
        kind: "move".into(),
        key: KEY.into(),
        ..State::default()
    }));
    let st = state.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut parts = first.split(' ');
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("").to_string();
            let (mut auth, mut length) = (String::new(), 0usize);
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
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let mut s = st.lock().unwrap();
            s.seen.push(Seen {
                method: method.clone(),
                target: target.clone(),
                auth: auth.clone(),
                body: body.clone(),
            });
            let keyed = auth == format!("Bearer {KEY}") || auth == format!("Bearer {KEY2}");
            let (status, ctype, out) = match (method.as_str(), target.as_str()) {
                ("POST", "/api/cli/exchange") => {
                    let code = serde_json::from_str::<serde_json::Value>(&body).unwrap()["code"]
                        .as_str()
                        .unwrap()
                        .to_string();
                    match code.as_str() {
                        "K7QM-4XPT" => (
                            200,
                            "application/json",
                            serde_json::json!({ "key": s.key, "api": "https://ignored.example",
                                "project": { "id": PID, "owner": "example", "repo": "shop", "name": "shop", "kind": s.kind } })
                            .to_string(),
                        ),
                        "USED-USED" => (409, "application/json", err("used_code", "This code was used already; copy a fresh line from the cabinet.")),
                        "GONE-GONE" => (410, "application/json", err("expired_code", "This code ran out (codes last 15 minutes); copy a fresh line from the cabinet.")),
                        _ => (404, "application/json", err("not_found", "This code is not one Exodus made; copy the line from the cabinet again.")),
                    }
                }
                _ if !keyed => (401, "application/json", err("unauthorized", "This key is not known; run the one line from the cabinet again to reconnect.")),
                ("GET", "/api/cli/project") => (
                    200,
                    "application/json",
                    serde_json::json!({ "id": PID, "owner": "example", "repo": "shop", "name": "shop", "kind": "move", "reports": 2,
                        "plan": { "from": 2, "done": 3, "total": 9 }, "needs": { "task": 4, "title": "A Google Cloud project" } })
                    .to_string(),
                ),
                ("POST", "/api/cli/audit/events") => (204, "application/json", String::new()),
                ("PUT", "/api/cli/answers") => (200, "application/json", r#"{"stored":1}"#.into()),
                ("POST", "/api/cli/reports") => {
                    if body.contains("\"refuse me\"") {
                        (400, "application/json", err("invalid", "This project holds 500 reports, the most; start a new project for the next ones."))
                    } else {
                        (200, "application/json", format!(r#"{{"n":2,"url":"http://app.test/p/{PID}/r/2","token":"tok","shared":false}}"#))
                    }
                }
                ("GET", "/api/cli/reports/2" | "/api/cli/reports/latest") if !s.report.is_empty() => {
                    (200, "application/json", s.report.clone())
                }
                ("GET", "/api/cli/reports/2/report.md") => (200, "text/markdown", "# Report 2 from the platform\n".into()),
                _ => (404, "application/json", err("not_found", "Nothing here.")),
            };
            drop(s);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    (url, state)
}

fn err(code: &str, message: &str) -> String {
    serde_json::json!({ "error": code, "message": message }).to_string()
}

/// A folder with its own HOME and the repository `shop` (no remote), named after the test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exodus-test-platform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::create_dir_all(dir.join("shop/.git")).unwrap();
    dir
}

/// The variables the platform client takes a proxy from; a test sets them itself, so the runner's own never apply.
const PROXY_VARS: [&str; 8] = [
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Runs the binary in `dir/shop` with HOME at `dir/home`: (exit code, stdout, stderr). No output may hold a key.
fn exodus(dir: &Path, key: Option<&str>, args: &[&str]) -> (i32, String, String) {
    exodus_with(dir, key, args, &[])
}

/// `exodus` with these variables set on top (no proxy unless one is among them).
fn exodus_with(dir: &Path, key: Option<&str>, args: &[&str], vars: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exodus"));
    cmd.args(args)
        .current_dir(dir.join("shop"))
        .env("HOME", dir.join("home"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("EXODUS_KEY");
    for v in PROXY_VARS {
        cmd.env_remove(v);
    }
    cmd.envs(vars.iter().copied());
    if let Some(k) = key {
        cmd.env("EXODUS_KEY", k);
    }
    let out = cmd.output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (o, e) = (text(&out.stdout), text(&out.stderr));
    for k in [KEY, KEY2] {
        let secret = &k[25..];
        assert!(
            !o.contains(secret) && !e.contains(secret),
            "{args:?} printed a key:\n{o}\n{e}"
        );
    }
    (out.status.code().unwrap(), o, e)
}

fn credentials(dir: &Path) -> PathBuf {
    dir.join("home/.config/exodus/credentials")
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn project_json(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("shop/.exodus/project.json")).unwrap()).unwrap()
}

fn point_at(dir: &Path, api: &str) {
    let mut p = project_json(dir);
    p["api"] = api.into();
    std::fs::write(dir.join("shop/.exodus/project.json"), p.to_string()).unwrap();
}

#[test]
fn init_with_a_code_links_and_keeps_the_key_in_a_0600_file_only() {
    let dir = folder("init");
    let (url, state) = platform();
    state.lock().unwrap().kind = "new".into();

    let (code, out, err) = exodus(&dir, None, &["init", "--code", "K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Linked to example/shop."), "{out}");
    assert!(
        out.contains("plan the new product"),
        "the kind came from the code: {out}"
    );

    let seen = state.lock().unwrap().seen.clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].method.as_str(), seen[0].target.as_str()),
        ("POST", "/api/cli/exchange")
    );
    assert!(seen[0].auth.is_empty(), "the exchange carries no credentials");
    let sent: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
    assert_eq!(sent["code"], "K7QM-4XPT");
    assert_eq!(sent["repo"], "shop", "no git remote: the folder's name");
    assert!(
        sent["device"].as_str().is_some_and(|d| !d.is_empty()) && sent["os"].is_string() && sent["cli"].is_string()
    );

    // the key is in the credentials file, 0600 in a 0700 folder, and nowhere in the repository
    let path = credentials(&dir);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    let creds = std::fs::read_to_string(&path).unwrap();
    assert!(creds.contains(KEY) && creds.contains(PID), "{creds}");
    assert!(
        creds.contains(&format!("api = \"{url}\"")),
        "the address that answered is kept: {creds}"
    );
    let p = project_json(&dir);
    assert_eq!(
        (p["project"].as_str(), p["kind"].as_str(), p["api"].as_str()),
        (Some(PID), Some("new"), Some(url.as_str()))
    );
    assert!(
        std::fs::read_to_string(dir.join("shop/.claude/skills/exodus/SKILL.md"))
            .unwrap()
            .contains("new product")
    );
    let mut stack = vec![dir.join("shop")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(d).unwrap() {
            let e = e.unwrap().path();
            if e.is_dir() {
                stack.push(e);
            } else {
                assert!(
                    !std::fs::read_to_string(&e).unwrap_or_default().contains(KEY),
                    "{} holds the key",
                    e.display()
                );
            }
        }
    }

    // the key works: status reads it from the credentials
    let (code, out, _) = exodus(&dir, None, &["status"]);
    assert_eq!(code, 0);
    assert!(
        out.contains(&format!("Key: exk_{PID}_...")) && out.contains("reachable"),
        "{out}"
    );

    // another project's entry is kept beside it; the same project's is replaced
    std::fs::write(
        &path,
        format!("{creds}\n[[projects]]\nid = \"OTHER\"\nkey = \"exk_OTHER_1\"\nname = \"other\"\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    state.lock().unwrap().key = KEY2.into();
    let (code, _, err) = exodus(&dir, None, &["init", "--code", "K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 0, "{err}");
    let creds = std::fs::read_to_string(&path).unwrap();
    assert!(
        creds.contains(KEY2) && !creds.contains(KEY) && creds.contains("exk_OTHER_1"),
        "{creds}"
    );
    assert_eq!(creds.matches("[[projects]]").count(), 2);
    assert_eq!(mode(&path), 0o600, "a file left wider is narrowed again");
}

#[test]
fn a_used_or_expired_code_exits_4_and_writes_nothing() {
    let dir = folder("badcode");
    let (url, _) = platform();
    for (code_arg, exit, says) in [
        (
            "USED-USED",
            4,
            "this code was used already (409), so nothing was linked. Copy a fresh line from the cabinet",
        ),
        (
            "GONE-GONE",
            4,
            "this code ran out (410; a code lasts 15 minutes), so nothing was linked. Copy a fresh line",
        ),
        (
            "NOPE-NOPE",
            3,
            "the platform refused (404): This code is not one Exodus made",
        ),
    ] {
        let (code, _, err) = exodus(&dir, None, &["init", "--code", code_arg, "--api", &url]);
        assert_eq!(code, exit, "{code_arg}: {err}");
        assert!(err.contains(says), "{err}");
        if exit == 4 {
            assert!(err.contains("rather than going on offline"), "{err}");
        }
    }
    assert!(!dir.join("shop/.exodus").exists() && !credentials(&dir).exists());

    // the wrong folder: refused before the code is sent, with the command for the right one
    let (url, state) = platform();
    std::fs::create_dir_all(dir.join("shop/web")).unwrap();
    for (folder, says) in [("home", "is your home folder"), ("shop/web", "inside the repository")] {
        let out = Command::new(env!("CARGO_BIN_EXE_exodus"))
            .args(["init", "--code", "K7QM-4XPT", "--api", &url])
            .current_dir(dir.join(folder))
            .env("HOME", dir.join("home"))
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{err}");
        assert!(
            err.contains(says) && err.contains("init --code K7QM-4XPT --api http://127.0.0.1:"),
            "{err}"
        );
    }
    assert!(state.lock().unwrap().seen.is_empty(), "no code was sent");
    // --here links a folder that is not a repository's top
    let out = Command::new(env!("CARGO_BIN_EXE_exodus"))
        .args([
            "init",
            "--code",
            "K7QM-4XPT",
            "--api",
            &url,
            "--here",
            "--agent",
            "codex",
        ])
        .current_dir(dir.join("shop/web"))
        .env("HOME", dir.join("home"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(state.lock().unwrap().seen.len(), 1);
    assert!(dir.join("shop/web/.exodus/project.json").is_file());

    // nobody answers: exit 2 with the URL
    let (code, _, err) = exodus(
        &dir,
        None,
        &["init", "--code", "K7QM-4XPT", "--api", "http://127.0.0.1:9"],
    );
    assert_eq!(code, 2);
    assert!(err.contains("POST http://127.0.0.1:9/api/cli/exchange"), "{err}");
    // a plain http address elsewhere is refused before anything is sent
    let (code, _, err) = exodus(
        &dir,
        None,
        &["init", "--code", "K7QM-4XPT", "--api", "http://example.com"],
    );
    assert_eq!(code, 1);
    assert!(err.contains("https://"), "{err}");
}

#[test]
fn login_relinks_the_repository_with_a_new_key() {
    let dir = folder("login");
    let (url, state) = platform();
    let (code, _, err) = exodus(&dir, None, &["login", "--code", "K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 1);
    assert!(err.contains("exodus init --code"), "{err}");
    assert!(state.lock().unwrap().seen.is_empty());

    let (code, _, _) = exodus(&dir, None, &["init"]);
    assert_eq!(code, 0);
    let (code, out, err) = exodus(&dir, None, &["login", "--code", "K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Linked to example/shop."), "{out}");
    assert_eq!(project_json(&dir)["project"], PID);
    assert!(std::fs::read_to_string(credentials(&dir)).unwrap().contains(KEY));
    // project.json now names the API, so the next login needs no --api
    state.lock().unwrap().key = KEY2.into();
    let (code, _, err) = exodus(&dir, None, &["login", "--code", "K7QM-4XPT"]);
    assert_eq!(code, 0, "{err}");
    assert!(std::fs::read_to_string(credentials(&dir)).unwrap().contains(KEY2));
}

#[test]
fn status_shows_the_platform_half_with_a_key_and_offline_without() {
    let dir = folder("status");
    let (url, state) = platform();
    let (code, out, _) = exodus(&dir, None, &["status"]);
    assert_eq!(code, 1);
    assert!(out.is_empty());

    exodus(&dir, None, &["init"]);
    let (code, out, _) = exodus(&dir, None, &["status"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("Project: shop (move)") && out.contains("not linked (offline)"),
        "{out}"
    );
    assert!(out.contains("no findings yet"), "{out}");
    assert!(state.lock().unwrap().seen.is_empty());

    point_at(&dir, &url);
    std::fs::write(
        dir.join("shop/.exodus/plan.md"),
        "# Plan\n\n- [x] 1. One\n- [ ] 2. Two\n- [ ] 3. Three\n",
    )
    .unwrap();
    let (code, out, err) = exodus(&dir, Some(KEY), &["status"]);
    assert_eq!(code, 0, "{err}");
    for line in [
        "Key: exk_P1abcdefghijklmnopqr_...",
        &format!("API: {url} - reachable"),
        "Reports on the platform: 2",
        "Plan (report 2): 3 of 9 tasks done",
        "Needs you: task 4, A Google Cloud project",
        "plan.md: 1 of 3 tasks done",
    ] {
        assert!(out.contains(line), "{line}:\n{out}");
    }
    let last = state.lock().unwrap().seen.last().cloned().unwrap();
    assert_eq!(
        (last.method.as_str(), last.target.as_str()),
        ("GET", "/api/cli/project")
    );

    let (code, out, _) = exodus(&dir, Some(KEY), &["status", "--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["platform"]["reports"], 2);
    assert_eq!(v["key"], format!("exk_{PID}_..."));
    assert_eq!(v["local"]["plan"]["total"], 3);

    // a revoked key: the platform's sentence, exit 3; nobody answering: "unreachable", exit 0
    let wrong = "exk_P1abcdefghijklmnopqr_00000000000000000000000000000000";
    let (code, out, err) = exodus(&dir, Some(wrong), &["status"]);
    assert_eq!(code, 3);
    assert!(
        err.contains("This key is not known") && out.contains("Project: shop"),
        "{err}"
    );
    assert!(!out.contains("0000000000") && !err.contains("0000000000"));
    point_at(&dir, "http://127.0.0.1:9");
    let (code, out, _) = exodus(&dir, Some(KEY), &["status"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("unreachable (GET http://127.0.0.1:9/api/cli/project"),
        "{out}"
    );
}

/// A SOCKS5 proxy on a local port, no authentication, CONNECT only: it records the target as it came (a name for
/// socks5h, an address for socks5), opens 127.0.0.1 at its port (where the fake platform listens) and pipes both ways.
fn socks5() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let mut c = client.unwrap();
            let mut head = [0u8; 2];
            c.read_exact(&mut head).unwrap();
            let mut methods = vec![0u8; head[1] as usize];
            c.read_exact(&mut methods).unwrap();
            assert!(head[0] == 5 && methods.contains(&0), "not a SOCKS5 greeting");
            c.write_all(&[5, 0]).unwrap();
            let mut req = [0u8; 4];
            c.read_exact(&mut req).unwrap();
            assert_eq!(req[..2], [5, 1], "not a CONNECT");
            let host = match req[3] {
                1 => {
                    let mut ip = [0u8; 4];
                    c.read_exact(&mut ip).unwrap();
                    std::net::Ipv4Addr::from(ip).to_string()
                }
                3 => {
                    let mut len = [0u8; 1];
                    c.read_exact(&mut len).unwrap();
                    let mut name = vec![0u8; len[0] as usize];
                    c.read_exact(&mut name).unwrap();
                    format!("name {}", String::from_utf8(name).unwrap())
                }
                4 => {
                    let mut ip = [0u8; 16];
                    c.read_exact(&mut ip).unwrap();
                    std::net::Ipv6Addr::from(ip).to_string()
                }
                t => panic!("address type {t}"),
            };
            let mut p = [0u8; 2];
            c.read_exact(&mut p).unwrap();
            let target = u16::from_be_bytes(p);
            log.lock().unwrap().push(format!("{host}:{target}"));
            let up = std::net::TcpStream::connect(("127.0.0.1", target)).unwrap();
            c.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
            let (mut c2, mut up2) = (c.try_clone().unwrap(), up.try_clone().unwrap());
            std::thread::spawn(move || std::io::copy(&mut c2, &mut up2));
            std::thread::spawn(move || std::io::copy(&mut { up }, &mut { c }));
        }
    });
    (port, seen)
}

#[test]
fn the_platform_client_goes_through_a_socks_proxy_from_the_environment() {
    let dir = folder("socks");
    let (url, state) = platform();
    let (port, proxied) = socks5();
    exodus(&dir, None, &["init"]);
    // by name, so socks5h shows: the name reaches the proxy unresolved
    let api_port = url.rsplit(':').next().unwrap().to_string();
    point_at(&dir, &format!("http://localhost:{api_port}"));
    let socks = format!("socks5h://127.0.0.1:{port}");

    for var in ["ALL_PROXY", "all_proxy", "HTTPS_PROXY", "https_proxy"] {
        let before = proxied.lock().unwrap().len();
        let (code, out, err) = exodus_with(&dir, Some(KEY), &["status"], &[(var, &socks)]);
        assert_eq!(code, 0, "{var}: {err}");
        assert!(out.contains("Reports on the platform: 2"), "{var}:\n{out}");
        let p = proxied.lock().unwrap();
        assert_eq!(p.len(), before + 1, "{var} did not go through the proxy");
        assert_eq!(p.last().unwrap(), &format!("name localhost:{api_port}"));
    }

    // socks5 resolves here (to 127.0.0.1 or ::1) and hands the proxy an address
    let (code, _, err) = exodus_with(
        &dir,
        Some(KEY),
        &["status"],
        &[("ALL_PROXY", &format!("socks5://127.0.0.1:{port}"))],
    );
    assert_eq!(code, 0, "{err}");
    let last = proxied.lock().unwrap().last().unwrap().clone();
    assert!(
        !last.starts_with("name ") && last.ends_with(&format!(":{api_port}")),
        "{last}"
    );

    // NO_PROXY sends the call straight to the platform
    let before = (proxied.lock().unwrap().len(), state.lock().unwrap().seen.len());
    for no in ["NO_PROXY", "no_proxy"] {
        let (code, out, _) = exodus_with(
            &dir,
            Some(KEY),
            &["status"],
            &[("ALL_PROXY", &socks), (no, "localhost")],
        );
        assert!(code == 0 && out.contains("Reports on the platform: 2"), "{no}:\n{out}");
    }
    assert_eq!(proxied.lock().unwrap().len(), before.0);
    assert_eq!(state.lock().unwrap().seen.len(), before.1 + 2);

    // a proxy nobody answers on is named in the error, without the password a proxy URL may carry
    let (code, out, _) = exodus_with(
        &dir,
        Some(KEY),
        &["status"],
        &[("ALL_PROXY", "socks5h://user:secret@127.0.0.1:9")],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("unreachable") && out.contains("(through the proxy socks5h://127.0.0.1:9)"),
        "{out}"
    );
    assert!(!out.contains("secret"), "{out}");
    let (code, _, err) = exodus_with(
        &dir,
        None,
        &[
            "init",
            "--code",
            "K7QM-4XPT",
            "--api",
            &format!("http://localhost:{api_port}"),
        ],
        &[("ALL_PROXY", "socks5h://127.0.0.1:9")],
    );
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("through the proxy socks5h://127.0.0.1:9"), "{err}");
}

#[test]
fn audit_answer_send_and_pull_go_through_the_platform_with_a_key() {
    let dir = folder("send");
    let (url, state) = platform();
    std::fs::write(
        dir.join("shop/package.json"),
        r#"{"name":"shop","scripts":{"start":"node server.js"},"dependencies":{"express":"4.19.2","pg":"8.11.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("shop/server.js"),
        "require('express')().listen(process.env.PORT)\n",
    )
    .unwrap();
    exodus(&dir, None, &["init"]);

    // without a key: nothing is sent and send says why
    let (code, _, _) = exodus(&dir, None, &["audit", "--quiet"]);
    assert_eq!(code, 0);
    let (code, out, err) = exodus(&dir, None, &["send"]);
    assert_eq!(code, 2);
    assert!(
        out.is_empty() && err.contains("Sending needs a linked account. `exodus report` builds"),
        "{err}"
    );
    for pull in [&["report", "pull"][..], &["report", "--pull"]] {
        let (code, _, err) = exodus(&dir, None, pull);
        assert_eq!(code, 2, "{err}");
    }
    assert!(state.lock().unwrap().seen.is_empty());

    // a report built here becomes what the fake platform serves as version 2
    let (code, _, err) = exodus(&dir, None, &["report"]);
    assert_eq!(code, 0, "{err}");
    let mut served: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("shop/.exodus/report.json")).unwrap()).unwrap();
    served["n"] = 2.into();
    served["token"] = "tok".into();
    state.lock().unwrap().report = served.to_string();
    std::fs::remove_file(dir.join("shop/.exodus/report.json")).unwrap();
    point_at(&dir, &url);

    // audit with a key: step 1, then step 3 with what it found and the questions
    let (code, _, err) = exodus(&dir, Some(KEY), &["audit", "--quiet"]);
    assert_eq!(code, 0, "{err}");
    let events: Vec<serde_json::Value> = state
        .lock()
        .unwrap()
        .seen
        .iter()
        .filter(|s| s.target == "/api/cli/audit/events")
        .map(|s| serde_json::from_str(&s.body).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["step"], 1);
    assert_eq!(events[1]["step"], 3);
    assert!(
        events[1]["found"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["text"].as_str().unwrap().contains("node server.js")),
        "{}",
        events[1]
    );
    let questions = events[1]["questions"].as_array().unwrap().clone();
    assert!(
        !questions.is_empty()
            && questions
                .iter()
                .all(|q| q["id"].is_string() && q["affects"].is_string())
    );
    // --offline posts nothing
    let before = state.lock().unwrap().seen.len();
    exodus(&dir, Some(KEY), &["audit", "--quiet", "--offline"]);
    assert_eq!(state.lock().unwrap().seen.len(), before);

    // an answer goes to the platform too
    let qid = questions[0]["id"].as_str().unwrap().to_string();
    let (code, out, err) = exodus(
        &dir,
        Some(KEY),
        &["answer", &qid, "About a thousand a day", "--found", "the logs"],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("The cabinet has it too."), "{out}");
    let put = state.lock().unwrap().seen.last().cloned().unwrap();
    assert_eq!((put.method.as_str(), put.target.as_str()), ("PUT", "/api/cli/answers"));
    let put: serde_json::Value = serde_json::from_str(&put.body).unwrap();
    assert_eq!(put[0]["id"], qid.as_str());
    assert_eq!(put[0]["found"], "the logs");
    assert!(put[0]["affects"].as_str().is_some_and(|a| !a.is_empty()));

    // SPEC exodus-fix F9: the agent's description goes into findings.json from stdin (or a file), a new audit keeps it,
    // the local report opens with it and send carries it; it travels to nothing on its own
    let before = state.lock().unwrap().seen.len();
    let text = "project: A shop.\ntoday: One Express process\non a host, with Postgres.\nplan: Move to Cloud Run.\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_exodus"))
        .arg("describe")
        .current_dir(dir.join("shop"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    exodus(&dir, Some(KEY), &["audit", "--quiet", "--offline"]);
    let findings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("shop/.exodus/findings.json")).unwrap()).unwrap();
    assert_eq!(
        findings["understanding"]["today"],
        "One Express process on a host, with Postgres."
    );
    std::fs::write(dir.join("text.txt"), "project: A shop.\ntoday: b\n").unwrap();
    let file = dir.join("text.txt");
    let (code, _, err) = exodus(&dir, None, &["describe", "--file", file.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(err.contains("`plan:` is missing"), "{err}");
    assert_eq!(state.lock().unwrap().seen.len(), before);

    // SPEC exodus-fix F2 (S1): `exodus report` with a key builds here and uploads nothing, so report then send is one
    // upload, one version
    let before = state.lock().unwrap().seen.len();
    let (code, out, err) = exodus(&dir, Some(KEY), &["report"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("(report 1,") && out.contains("not uploaded: `exodus send`"),
        "{out}"
    );
    let md = std::fs::read_to_string(dir.join("shop/.exodus/report.md")).unwrap();
    assert!(md.contains("## Our understanding\n\n**What it is.** A shop."), "{md}");
    assert_eq!(state.lock().unwrap().seen.len(), before);
    std::fs::remove_file(dir.join("shop/.exodus/report.json")).unwrap();

    // send: the findings, the answers and the agent; the version comes back into .exodus/
    let (code, out, err) = exodus(&dir, Some(KEY), &["send", "--agent", "Claude Code"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains(&format!("Report 2: http://app.test/p/{PID}/r/2")) && out.contains("Private"),
        "{out}"
    );
    assert!(out.contains("(report 2, built by the platform)"), "{out}");
    let seen = state.lock().unwrap().seen.clone();
    let tail: Vec<String> = seen[seen.len() - 3..]
        .iter()
        .map(|s| format!("{} {}", s.method, s.target))
        .collect();
    assert_eq!(
        tail,
        [
            "POST /api/cli/reports",
            "GET /api/cli/reports/2",
            "GET /api/cli/reports/2/report.md"
        ]
    );
    let upload: serde_json::Value = serde_json::from_str(&seen[seen.len() - 3].body).unwrap();
    assert_eq!(upload["findings"]["exodus"], "findings/1");
    assert_eq!(upload["answers"]["answers"][0]["id"], qid.as_str());
    assert_eq!(upload["agent"], "Claude Code");
    assert_eq!(upload["findings"]["understanding"]["plan"], "Move to Cloud Run.");
    let posts = |state: &std::sync::Mutex<State>| {
        let seen = &state.lock().unwrap().seen;
        seen.iter()
            .filter(|s| s.method == "POST" && s.target == "/api/cli/reports")
            .count()
    };
    assert_eq!(posts(&state), 1);
    assert!(seen.iter().all(|s| s.auth == format!("Bearer {KEY}")));
    assert_eq!(
        std::fs::read_to_string(dir.join("shop/.exodus/report.md")).unwrap(),
        "# Report 2 from the platform\n"
    );
    let (_, out, _) = exodus(&dir, Some(KEY), &["status"]);
    assert!(out.contains("Report 2 (from the platform"), "{out}");

    // report --pull: latest by default, a version by --report, 404 is exit 3; `report pull` still works
    let (code, _, err) = exodus(&dir, Some(KEY), &["report", "--pull"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        state
            .lock()
            .unwrap()
            .seen
            .iter()
            .any(|s| s.target == "/api/cli/reports/latest")
    );
    let (code, _, err) = exodus(&dir, Some(KEY), &["report", "--report", "7"]);
    assert_eq!(code, 3);
    assert!(err.contains("no report 7"), "{err}");
    let (code, _, err) = exodus(&dir, Some(KEY), &["report", "pull", "--report", "7"]);
    assert_eq!(code, 3, "{err}");
    // --pull --json prints only the file
    let (code, out, err) = exodus(&dir, Some(KEY), &["report", "--pull", "--json"]);
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["n"], 2);
    // --offline is still accepted, builds here and calls nothing; none of the report runs uploaded
    let before = state.lock().unwrap().seen.len();
    let (code, _, err) = exodus(&dir, Some(KEY), &["report", "--offline", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(state.lock().unwrap().seen.len(), before);
    assert_eq!(posts(&state), 1);

    // a refusal is the platform's sentence and exit 3
    let (code, _, err) = exodus(&dir, Some(KEY), &["send", "--agent", "refuse me"]);
    assert_eq!(code, 3);
    assert!(
        err.contains("the platform refused (400): This project holds 500 reports"),
        "{err}"
    );
    // a wrong key is 401, and the key is not in the message
    let wrong = "exk_P1abcdefghijklmnopqr_11111111111111111111111111111111";
    let (code, _, err) = exodus(&dir, Some(wrong), &["send"]);
    assert_eq!(code, 3);
    assert!(err.contains("(401)") && !err.contains("1111111111"), "{err}");
}

// install.sh as a person runs it (SPEC F1): `curl ... | sh -s -- CODE` from the home folder, in a repository and in an
// empty folder with --new. The release comes from a local folder through a stand-in `curl` first on PATH, and the
// exchange from the fake platform, so nothing leaves the machine; each run has its own HOME, without exodus on PATH.

/// The installer's script, fed to `sh -s` on stdin the way `curl | sh` does.
const INSTALL: &str = include_str!("../../../install.sh");

/// A release of this build: `exodus-<target>.tar.gz` for every target the installer may ask for, `SHA256SUMS`, and
/// `bin/curl`, which copies the asset a URL names from this folder.
fn release() -> &'static Path {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        // cargo's scratch folder for tests, rewritten each run
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("exodus-release");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let bin = Path::new(env!("CARGO_BIN_EXE_exodus"));
        let script = format!(
            r#"set -e
# stripped, a fifth of the size: each run copies it twice
cp '{}' exodus && (strip exodus 2>/dev/null || true)
tar -cf - exodus | gzip -1 > exodus.tar.gz && rm exodus
sum=$( (sha256sum exodus.tar.gz 2>/dev/null || shasum -a 256 exodus.tar.gz) | cut -d' ' -f1)
for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-apple-darwin aarch64-apple-darwin; do
    ln -s exodus.tar.gz "exodus-$t.tar.gz"; echo "$sum  exodus-$t.tar.gz" >> SHA256SUMS
done
cat > bin/curl <<'CURL'
#!/bin/sh
out= url=
while [ $# -gt 0 ]; do case "$1" in -o) out=$2; shift ;; --retry) shift ;; -*) ;; *) url=$1 ;; esac; shift; done
cp "$EXODUS_TEST_RELEASE/${{url##*/}}" "$out"
CURL
chmod 755 bin/curl"#,
            bin.display()
        );
        let out = Command::new("sh")
            .args(["-c", &script])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        dir
    })
}

/// `curl -fsSL .../install | sh -s -- ARGS` in `cwd` with HOME at `home`: (exit code, everything it printed).
fn install(home: &Path, cwd: &Path, args: &[&str]) -> (i32, String) {
    let mut child = Command::new("sh")
        .args(["-s", "--"])
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("HOME", home)
        // bash, macOS's sh, fills an unset SHELL with the login shell; --path picks the rc file by it
        .env("SHELL", "/bin/sh")
        .env(
            "PATH",
            format!("{}/bin:/usr/bin:/bin:/usr/sbin:/sbin", release().display()),
        )
        .env("EXODUS_TEST_RELEASE", release())
        // the installer unpacks beside the test's own files, which the test removes
        .env("TMPDIR", home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(INSTALL.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    // the whole conversation, for `cargo test --test platform install -- --nocapture`
    eprintln!("--- sh -s -- {} in {}\n{text}", args.join(" "), cwd.display());
    (out.status.code().unwrap(), text)
}

#[test]
fn install_from_the_home_folder_keeps_the_code_and_prints_the_command_that_spends_it() {
    let dir = folder("install-home");
    let (url, state) = platform();
    let home = dir.join("home");
    let (code, out) = install(&home, &home, &["K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("exodus: installed exodus ") && out.contains("is not on PATH"),
        "{out}"
    );
    let again = format!("~/.local/bin/exodus init --code K7QM-4XPT --api {url}");
    assert!(
        out.contains("~ is your home folder, not a repository, so the code was not used.")
            && out.contains(&format!("\n  cd <the repository> && {again}\n")),
        "{out}"
    );
    assert!(state.lock().unwrap().seen.is_empty(), "the code was not sent");
    assert!(home.join(".local/bin/exodus").is_file());
    let left: Vec<_> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, [".local"], "nothing but the binary in the home folder");

    // the printed command, in the repository, spends it there
    let out = Command::new("sh")
        .args(["-c", &again.replacen('~', "$HOME", 1)])
        .current_dir(dir.join("shop"))
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Linked to example/shop."));
    assert_eq!(state.lock().unwrap().seen.len(), 1);
    assert_eq!(project_json(&dir)["project"], PID);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn install_in_a_repository_links_it_and_a_used_code_asks_for_a_fresh_line() {
    let dir = folder("install-repo");
    let (url, state) = platform();
    let (home, shop) = (dir.join("home"), dir.join("shop"));
    std::fs::write(
        shop.join(".git/config"),
        "[remote \"origin\"]\n\turl = git@github.com:example/shop.git\n",
    )
    .unwrap();
    let (code, out) = install(&home, &shop, &["USED-USED", "--api", &url]);
    assert_eq!(code, 4, "{out}");
    assert!(out.contains("Copy a fresh line from the cabinet"), "{out}");
    assert!(!shop.join(".exodus").exists());

    let (code, out) = install(&home, &shop, &["K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Linked to example/shop."), "{out}");
    assert_eq!(state.lock().unwrap().seen.len(), 2);
    assert_eq!(project_json(&dir)["kind"], "move");
    assert_eq!(mode(&credentials(&dir)), 0o600);
    assert!(shop.join(".claude/skills/exodus/SKILL.md").is_file());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn install_with_new_plans_in_an_empty_folder_only() {
    let dir = folder("install-new");
    let (url, state) = platform();
    state.lock().unwrap().kind = "new".into();
    let home = dir.join("home");
    let idea = dir.join("idea");
    std::fs::create_dir_all(&idea).unwrap();

    // without --new an empty folder is no repository: the command adds it
    let (code, out) = install(&home, &idea, &["K7QM-4XPT", "--api", &url]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("is empty and not a repository") && out.contains(" --new\n"),
        "{out}"
    );
    assert!(state.lock().unwrap().seen.is_empty());

    let (code, out) = install(&home, &idea, &["K7QM-4XPT", "--new", "--api", &url]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("plan the new product"), "{out}");
    assert_eq!(state.lock().unwrap().seen.len(), 1);
    assert!(
        std::fs::read_to_string(idea.join(".claude/skills/exodus/SKILL.md"))
            .unwrap()
            .contains("new product")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn install_without_a_code_only_installs() {
    let dir = folder("install-only");
    let home = dir.join("home");
    let (code, out) = install(&home, &home, &[]);
    assert_eq!(code, 0, "{out}");
    let left: Vec<_> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, [".local"], "{out}");
    let (code, out) = install(&home, &home, &["--path"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        std::fs::read_to_string(home.join(".profile"))
            .unwrap()
            .contains("/.local/bin:$PATH")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
