//! `exodus status`: the project of this repository, the key's id (never the key), whether the platform answers, its
//! reports, the plan's progress and an open need; and what `.exodus/` holds here. Without a key it shows the local half
//! and says "offline".

use std::path::Path;
use std::process::ExitCode;

use exodus_schema::{Answers, Findings, ProjectFile, Report};
use serde_json::{Value, json};

use crate::platform::{self, Failure};

#[derive(clap::Args)]
pub struct Args {
    /// Print it as JSON
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> ExitCode {
    let root = Path::new(".");
    let read = |name: &str| {
        super::report::read::<Value>(&root.join(".exodus").join(name))
            .ok()
            .flatten()
    };
    let Some(project) = read("project.json").and_then(|v| serde_json::from_value::<ProjectFile>(v).ok()) else {
        eprintln!("exodus status: no .exodus/project.json here; `exodus init` lays it out.");
        return ExitCode::FAILURE;
    };
    let findings = read("findings.json").and_then(|v| serde_json::from_value::<Findings>(v).ok());
    let answers = read("answers.json").and_then(|v| serde_json::from_value::<Answers>(v).ok());
    let report = read("report.json").and_then(|v| serde_json::from_value::<Report>(v).ok());
    let plan = std::fs::read_to_string(root.join(".exodus/plan.md"))
        .ok()
        .map(|t| ticks(&t));

    let link = platform::link(root);
    let (platform, refused) = match &link {
        None => (json!("offline"), None),
        Some(l) => match l.get("cli/project", &[]) {
            Ok(v) => (v, None),
            Err(Failure::Unreachable(m)) => (json!({ "unreachable": m }), None),
            Err(f @ Failure::Refused(..)) => (json!({ "refused": f.message() }), Some(f)),
        },
    };
    let local = json!({
        "findings": findings.as_ref().map(|f| json!({ "at": f.stats.finished_at, "questions": f.questions.len() })),
        "answers": answers.as_ref().map_or(0, |a| a.answers.len()),
        "report": report.as_ref().map(|r| json!({ "n": r.n, "at": r.at, "verdict": r.verdict.title, "platform": r.token.is_some() })),
        "plan": plan.map(|(done, total)| json!({ "done": done, "total": total })),
    });

    if args.json {
        let out = json!({
            "project": project,
            "key": link.as_ref().map(|l| l.key_id()),
            "api": link.as_ref().map(|l| l.api.clone()),
            "platform": platform,
            "local": local,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        print(&project, link.as_ref(), &platform, &local);
    }
    match refused {
        Some(f) => {
            eprintln!("exodus: {}", f.message());
            ExitCode::from(3)
        }
        None => ExitCode::SUCCESS,
    }
}

fn print(project: &ProjectFile, link: Option<&platform::Link>, platform: &Value, local: &Value) {
    let name = match &project.owner {
        Some(o) => format!("{o}/{}", project.name),
        None => project.name.clone(),
    };
    println!("Project: {name} ({})", project.kind);
    match link {
        None => println!("Account: not linked (offline). `exodus init --code` links one."),
        Some(l) => {
            println!("Key: {}", l.key_id());
            let state = match platform {
                Value::Object(m) if m.contains_key("unreachable") => format!("unreachable ({})", s(&m["unreachable"])),
                Value::Object(m) if m.contains_key("refused") => s(&m["refused"]).to_string(),
                _ => "reachable".into(),
            };
            println!("API: {} - {state}", l.api);
            if let Some(n) = platform["reports"].as_u64() {
                println!("Reports on the platform: {n}");
            }
            if let Some(p) = platform.get("plan") {
                println!(
                    "Plan (report {}): {} of {} tasks done",
                    p["from"], p["done"], p["total"]
                );
            }
            if let Some(n) = platform.get("needs") {
                println!("Needs you: task {}, {}", n["task"], s(&n["title"]));
            }
        }
    }
    match &local["findings"] {
        Value::Null => println!("Here: no findings yet; `exodus audit` reads the repository."),
        f => println!(
            "Here: findings of {}, {} questions, {} answered",
            s(&f["at"]),
            f["questions"],
            local["answers"]
        ),
    }
    if let Some(r) = local["report"].as_object() {
        let from = if r["platform"] == true {
            "from the platform"
        } else {
            "built here"
        };
        println!("Report {} ({from}, {}): {}", r["n"], s(&r["at"]), s(&r["verdict"]));
    }
    if let Some(p) = local["plan"].as_object() {
        println!("plan.md: {} of {} tasks done", p["done"], p["total"]);
    }
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// The ticked and all boxes of a plan.md.
pub(super) fn ticks(text: &str) -> (usize, usize) {
    let boxes: Vec<&str> = text
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("- [ ]") || l.starts_with("- [x]"))
        .collect();
    (boxes.iter().filter(|l| l.starts_with("- [x]")).count(), boxes.len())
}
