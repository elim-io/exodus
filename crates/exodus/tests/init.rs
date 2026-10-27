//! `exodus init` and `exodus answer` run as the binary, in a fresh folder each.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// An empty folder under the system's temp dir, named after the test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exodus-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn exodus(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_exodus"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn json(dir: &Path, path: &str) -> serde_json::Value {
    serde_json::from_str(&read(dir, path)).unwrap()
}

#[test]
fn lays_out_everything_once() {
    let dir = folder("move");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(
        dir.join(".git/config"),
        "[remote \"origin\"]\n\turl = git@github.com:example/shop-api.git\n",
    )
    .unwrap();
    std::fs::write(dir.join(".gitignore"), "node_modules").unwrap(); // no newline at the end
    std::fs::write(dir.join("AGENTS.md"), "# Agents\n\nRun the tests before a commit.\n").unwrap();

    let (ok, out) = exodus(&dir, &["init"]);
    assert!(ok, "{out}");
    assert!(out.contains("No account linked."), "{out}");
    assert!(
        out.contains("--skill cloud-run-basics") && out.contains("on your yes"),
        "{out}"
    );

    let project = json(&dir, ".exodus/project.json");
    assert_eq!(project["owner"], "example");
    assert_eq!(project["name"], "shop-api");
    assert_eq!(project["kind"], "move");
    assert!(project.get("project").is_none() && project.get("api").is_none());
    assert!(read(&dir, ".exodus/README.md").contains("findings.json"));
    assert_eq!(read(&dir, ".gitignore"), "node_modules\n.exodus/\n");

    let skill = read(&dir, ".claude/skills/exodus/SKILL.md");
    assert!(skill.starts_with("---\nname: exodus\ndescription: "), "{skill}");
    let agents = read(&dir, "AGENTS.md");
    assert!(
        agents.starts_with("# Agents\n\nRun the tests before a commit.\n\n<!-- exodus:start"),
        "{agents}"
    );
    assert!(agents.contains("\n## Exodus\n") && agents.contains("\n### 2. Questions"));
    let cursor = read(&dir, ".cursor/rules/exodus.mdc");
    assert!(cursor.starts_with("---\ndescription: ") && cursor.contains("alwaysApply: false"));

    // the method, in every agent's file
    for (text, agent) in [(&skill, "Claude Code"), (&agents, "Codex"), (&cursor, "Cursor")] {
        for needle in [
            "exodus audit",
            "--found",
            "railway status",
            "heroku ps",
            "Ask the person only what is left",
            "never files",
            "exodus need",
            "exodus task done <N> --commit <sha>",
            "gcloud billing projects describe",
            "bq ls",
            // the agent's own words name the targets the report prices
            "a SQLite database moves to Firestore",
        ] {
            assert!(text.contains(needle), "{agent}: no {needle:?}");
        }
        assert!(text.contains(&format!("--agent \"{agent}\"")), "{agent}");
        // Google's skills and MCP: this agent's install line, the key only in the environment, none on a command line
        let id = match agent {
            "Claude Code" => "claude-code",
            "Codex" => "codex",
            _ => "cursor",
        };
        assert!(
            text.contains(&format!("--skill cloud-run-basics --agent {id}`"))
                && text.contains("DEVELOPERKNOWLEDGE_API_KEY"),
            "{agent}: no Google step"
        );
        assert!(
            !text.contains("--header") && !text.contains("YOUR_API_KEY"),
            "{agent}: a key on a command line"
        );
        assert!(!text.contains("{{"), "{agent}: a placeholder left");
        assert!(!text.contains('\u{2014}'), "{agent}: an em dash");
    }

    // again: the same files, no second line or section, the rest kept
    let (ok, out) = exodus(&dir, &["init"]);
    assert!(ok, "{out}");
    assert!(!out.contains(".gitignore"), "{out}");
    assert_eq!(read(&dir, ".gitignore"), "node_modules\n.exodus/\n");
    assert_eq!(read(&dir, "AGENTS.md"), agents);
    assert_eq!(read(&dir, ".claude/skills/exodus/SKILL.md"), skill);
}

#[test]
fn new_product_and_one_agent() {
    let dir = folder("new");
    std::fs::write(dir.join(".gitignore"), "/.exodus\n").unwrap();
    let (ok, out) = exodus(&dir, &["init", "--new", "--agent", "claude"]);
    assert!(ok, "{out}");
    let project = json(&dir, ".exodus/project.json");
    assert_eq!(project["kind"], "new");
    assert!(project.get("owner").is_none());
    assert!(project["name"].as_str().unwrap().starts_with("exodus-test-new-"));
    assert_eq!(read(&dir, ".gitignore"), "/.exodus\n");
    let skill = read(&dir, ".claude/skills/exodus/SKILL.md");
    for needle in [
        "`what`",
        "`who`",
        "`visits`",
        "`payments`",
        "`realtime`",
        "`stack`",
        "Do not run `exodus audit` here",
    ] {
        assert!(skill.contains(needle), "no {needle:?}");
    }
    assert!(!dir.join("AGENTS.md").exists() && !dir.join(".cursor").exists());

    // the kind stays new without the flag
    let (ok, _) = exodus(&dir, &["init", "--agent", "codex"]);
    assert!(ok);
    assert_eq!(json(&dir, ".exodus/project.json")["kind"], "new");
    assert!(read(&dir, "AGENTS.md").starts_with("<!-- exodus:start") && read(&dir, "AGENTS.md").contains("`realtime`"));

    // an idea question is answered before any findings exist
    let (ok, out) = exodus(&dir, &["answer", "visits", "About 300 a day.", "--value", "300"]);
    assert!(ok, "{out}");
    assert!(out.contains("Still open: what, who, growth"), "{out}");
    let (ok, out) = exodus(&dir, &["answer", "bill", "$40"]);
    assert!(!ok && out.contains("not a question here"), "{out}");
}

#[test]
fn answers_are_checked_and_replaced() {
    let dir = folder("answer");
    let (ok, out) = exodus(&dir, &["answer", "visits", "2000 a day"]);
    assert!(!ok && out.contains("run `exodus audit` first"), "{out}");

    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/node-railway"),
        &dir,
    );
    let (ok, out) = exodus(&dir, &["audit", "--quiet"]);
    assert!(ok, "{out}");
    let findings = read(&dir, ".exodus/findings.json");
    let parsed: serde_json::Value = serde_json::from_str(&findings).unwrap();
    let part = parsed["processes"][0]["id"].as_str().unwrap().to_string();

    for (args, wrong) in [
        (vec!["answer", "visits", "a lot", "--value", "many"], "is a number"),
        (vec!["answer", "visits", "a lot", "--value", "-5"], "0 or more"),
        (vec!["answer", "users", "Mars", "--value", "mars-north1"], "region id"),
        (
            vec!["answer", "bill", "$40", "--value", "{\"nope\": 4000}"],
            "not a part",
        ),
        (
            vec!["answer", "bill", "$40", "--value", "{\"other\": 40.5}"],
            "whole cents",
        ),
        (vec!["answer", "downtime", "Yes", "--value", "1"], "carries no value"),
        (vec!["answer", "payments", "Yes"], "not a question here"),
        (vec!["answer", "domains", "  "], "empty"),
        (
            vec!["answer", "account", "Yes: 01ABCD-23EF45-6789AB (Main)"],
            "no billing account id",
        ),
        (
            vec![
                "answer",
                "account",
                "Yes, one",
                "--found",
                "billingAccounts/01ABCD-23EF45-6789AB",
            ],
            "no billing account id",
        ),
    ] {
        let (ok, out) = exodus(&dir, &args);
        assert!(!ok && out.contains(wrong), "{args:?}: {out}");
    }
    assert!(!dir.join(".exodus/answers.json").exists());

    let bill = format!("{{\"{part}\": 2000, \"other\": 500}}");
    for args in [
        vec![
            "answer",
            "visits",
            "About 1,000 a day",
            "--found",
            "the logs",
            "--value",
            "1000",
        ],
        vec![
            "answer",
            "visits",
            "About 2,000 a day",
            "--found",
            "the railway CLI",
            "--value",
            "2000",
        ],
        vec!["answer", "users", "Europe", "--value", "europe-west1"],
        vec![
            "answer",
            "bill",
            "$25.00 a month",
            "--found",
            "the invoice",
            "--value",
            &bill,
        ],
        vec!["answer", "deadline", "No, nothing waits"],
    ] {
        let (ok, out) = exodus(&dir, &args);
        assert!(ok, "{args:?}: {out}");
    }
    let answers = json(&dir, ".exodus/answers.json");
    assert_eq!(answers["exodus"], "answers/1");
    let list = answers["answers"].as_array().unwrap();
    assert_eq!(list.len(), 4, "{answers}");
    let visits = list.iter().find(|a| a["id"] == "visits").unwrap();
    assert_eq!(visits["value"], 2000);
    assert_eq!(visits["found"], "the railway CLI");
    assert!(list.iter().find(|a| a["id"] == "users").unwrap().get("found").is_none());

    // the report computes from them
    let (ok, out) = exodus(&dir, &["report", "--offline", "--json"]);
    assert!(ok, "{out}");
    let report: serde_json::Value = serde_json::from_str(&out[out.find('{').unwrap()..]).unwrap();
    assert_eq!(report["assumptions"]["visits"], 2000, "{}", report["assumptions"]);
    assert_eq!(report["cost"]["today"], 2500);

    // SPEC exodus-fix F6: a bill as one total is a Today line of its own; F5: an agent's rate for the forecast
    for args in [
        vec!["answer", "bill", "$5.00 a month in all", "--value", r#"{"total": 500}"#],
        vec!["answer", "egressmb", "About 0.5 MB a visit", "--value", "0.5"],
    ] {
        let (ok, out) = exodus(&dir, &args);
        assert!(ok, "{args:?}: {out}");
    }
    let (ok, out) = exodus(&dir, &["report", "--json"]);
    assert!(ok, "{out}");
    let report: serde_json::Value = serde_json::from_str(&out[out.find('{').unwrap()..]).unwrap();
    assert_eq!(report["cost"]["today"], 500);
    assert_eq!(report["assumptions"]["egressMbPerVisit"], 0.5);
    let lines = report["cost"]["lines"].as_array().unwrap();
    let bill = lines.iter().find(|l| l["part"]["kind"] == "bill").unwrap();
    assert_eq!((&bill["part"]["today"], &bill["cents"]), (&500.into(), &0.into()));
    let egress = lines.iter().find(|l| l["part"]["kind"] == "traffic").unwrap();
    assert!(egress["part"]["today"].is_null(), "{egress}");
}
