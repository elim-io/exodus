//! `exodus plan`, `exodus task done` and `exodus need` (architecture §3, §4.4, §6.5): the report's tasks as
//! `.exodus/plan.md`, a task ticked with the commit that did it, and what the agent cannot do without the person. The
//! file is the agent's and the person's; with a linked account the platform's plan follows it and the cabinet ticks.
//! The file is written first, so a failed post is fixed by running the same command again.

use std::path::Path;
use std::process::ExitCode;

use exodus_schema::Report;
use serde_json::{Value, json};

use crate::platform::{self, Link};

const PLAN: &str = ".exodus/plan.md";

#[derive(clap::Args)]
pub struct Args {
    /// The report version to plan from (default the one in .exodus/report.json)
    #[arg(long, value_name = "N")]
    report: Option<u32>,
    /// Start again over a plan with tasks done
    #[arg(long)]
    force: bool,
}

#[derive(clap::Args)]
pub struct DoneArgs {
    /// The task's number in plan.md
    n: u32,
    /// The commit that did it (7 to 40 hex digits, in this repository)
    #[arg(long, value_name = "SHA")]
    commit: String,
}

#[derive(clap::Args)]
pub struct NeedArgs {
    /// What the person has to do, in one line
    #[arg(required_unless_present = "clear")]
    title: Option<String>,
    /// The task that waits on it
    #[arg(long, value_name = "N", required_unless_present = "clear")]
    task: Option<u32>,
    /// One step for the person; "text|command" adds a line to copy (repeatable, at most 12)
    #[arg(long = "step", value_name = "TEXT")]
    steps: Vec<String>,
    /// Withdraw the open request
    #[arg(long, conflicts_with_all = ["title", "task", "steps"])]
    clear: bool,
}

pub fn plan(args: Args) -> ExitCode {
    let root = Path::new(".");
    let link = platform::link(root);
    let local: Option<Report> = super::report::read(&root.join(".exodus/report.json")).ok().flatten();
    // the platform's version N is the one its plan follows: a report built here, or another N, is pulled first (A27)
    let want = args.report.or(local.as_ref().map(|r| r.n));
    let fresh = local
        .as_ref()
        .is_some_and(|r| Some(r.n) == want && (link.is_none() || r.token.is_some()));
    let report = if fresh {
        local
    } else {
        let Some(link) = &link else {
            match want {
                Some(n) => eprintln!(
                    "exodus plan: .exodus/report.json is not report {n}; pulling it needs a linked account (`exodus init --code`)."
                ),
                None => eprintln!("exodus plan: no .exodus/report.json; `exodus report` builds one."),
            }
            return ExitCode::from(2);
        };
        let n = want.map_or_else(|| "latest".to_string(), |n| n.to_string());
        let code = super::send::pull(link, &n, true);
        if code != ExitCode::SUCCESS {
            return code;
        }
        super::report::read(&root.join(".exodus/report.json")).ok().flatten()
    };
    let Some(report) = report else {
        eprintln!("exodus plan: .exodus/report.json does not read as a report");
        return ExitCode::FAILURE;
    };
    if !args.force
        && let Ok(old) = std::fs::read_to_string(PLAN)
    {
        let (done, total) = super::status::ticks(&old);
        if done > 0 {
            eprintln!(
                "exodus plan: {done} of {total} tasks are done in {PLAN}; `exodus plan --force` starts again from report {}.",
                report.n
            );
            return ExitCode::FAILURE;
        }
    }
    if let Some(link) = &link
        && let Err(f) = link.post("cli/plan", &json!({ "report": report.n, "force": args.force }))
    {
        return super::failed(&f);
    }
    if let Err(e) = std::fs::write(PLAN, render(&report)) {
        eprintln!("exodus plan: cannot write {PLAN}: {e}");
        return ExitCode::FAILURE;
    }
    println!("Wrote {PLAN}: {} tasks from report {}.", report.tasks.len(), report.n);
    if link.is_some() {
        println!("The cabinet's plan follows report {} now.", report.n);
    } else {
        println!("Not linked: the cabinet has no plan to tick.");
    }
    ExitCode::SUCCESS
}

/// plan.md as §6.5 draws it: a box, the number, the title, effort and phase, the finding it fixes, then the detail.
pub fn render(r: &Report) -> String {
    let name = match &r.project.owner {
        Some(o) => format!("{o}/{}", r.project.name),
        None => r.project.name.clone(),
    };
    let mut s = format!(
        "# Plan for {name}, from report {} (exodus plan, {})\n",
        r.n,
        super::today()
    );
    for t in &r.tasks {
        let phase = r
            .phases
            .iter()
            .find(|p| p.phase == t.phase)
            .map_or("", |p| p.name.as_str());
        s += &format!("- [ ] {}. {} ({}, {phase})", t.i, t.title, t.effort);
        if let Some(f) = t.fixes.as_ref().and_then(|id| r.findings.iter().find(|f| &f.id == id)) {
            s += &format!(" - fixes finding {}", f.i);
        }
        s.push('\n');
        if !t.detail.is_empty() {
            s += &format!("  {}\n", t.detail);
        }
    }
    s
}

pub fn done(args: DoneArgs) -> ExitCode {
    let commit = args.commit.trim().to_ascii_lowercase();
    if !(7..=40).contains(&commit.len()) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        eprintln!("exodus task done: --commit takes a git commit, 7 to 40 hex digits");
        return ExitCode::FAILURE;
    }
    let known = std::process::Command::new("git")
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .stderr(std::process::Stdio::null())
        .status();
    match known {
        Ok(s) if s.success() => {}
        Ok(_) => {
            eprintln!("exodus task done: {commit} is not a commit of this repository");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("exodus task done: cannot run git: {e}");
            return ExitCode::FAILURE;
        }
    }
    let mut lines = match plan_lines() {
        Ok(l) => l,
        Err(code) => return code,
    };
    let Some(at) = find(&lines, args.n) else {
        return missing(args.n, &lines);
    };
    // "- [x] 2. Title (L, Change the code) - done 2026-10-11, 4e1c9a2": a second commit keeps the first date
    let rest = &lines[at].trim_start()[6..];
    let (rest, date) = match rest.rsplit_once(" - done ") {
        Some((head, tail)) => (head, tail.split(',').next().unwrap_or("").to_string()),
        None => (rest, super::today()),
    };
    lines[at] = format!("- [x] {rest} - done {date}, {}", &commit[..7]);
    drop_need(&mut lines, Some(at));
    if let Err(code) = write(&lines) {
        return code;
    }
    let Some(link) = platform::link(Path::new(".")) else {
        println!(
            "Task {} ticked in {PLAN}. Not linked: the cabinet's plan will not tick.",
            args.n
        );
        return ExitCode::SUCCESS;
    };
    match link.post(&format!("cli/tasks/{}/done", args.n), &json!({ "commit": commit })) {
        Ok(v) => {
            println!(
                "Task {} done: {} of {} on the cabinet's plan.",
                args.n, v["done"], v["total"]
            );
            ExitCode::SUCCESS
        }
        Err(f) => told_not(&link, &f),
    }
}

pub fn need(args: NeedArgs) -> ExitCode {
    if args.clear {
        if let Ok(mut lines) = plan_lines()
            && drop_need(&mut lines, None)
            && let Err(code) = write(&lines)
        {
            return code;
        }
        let Some(link) = platform::link(Path::new(".")) else {
            println!("The request is gone from {PLAN}.");
            return ExitCode::SUCCESS;
        };
        return match link.delete("cli/needs") {
            Ok(_) => {
                println!("The request is withdrawn; the cabinet no longer shows it.");
                ExitCode::SUCCESS
            }
            Err(f) => told_not(&link, &f),
        };
    }
    let (Some(title), Some(n)) = (args.title.as_deref().map(str::trim), args.task) else {
        return ExitCode::FAILURE;
    };
    let steps = match body(title, &args.steps) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("exodus need: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut lines = match plan_lines() {
        Ok(l) => l,
        Err(code) => return code,
    };
    let Some(at) = find(&lines, n) else {
        return missing(n, &lines);
    };
    if lines[at].trim_start().starts_with("- [x]") {
        eprintln!("exodus need: task {n} is done already");
        return ExitCode::FAILURE;
    }
    // one request is open at a time: the old one goes, wherever it was
    drop_need(&mut lines, None);
    let at = find(&lines, n).unwrap_or(at);
    let mut end = at + 1;
    while end < lines.len() && lines[end].starts_with("  ") {
        end += 1;
    }
    let mut note = vec![format!("  > Needs you: {title}")];
    for (k, s) in steps.iter().enumerate() {
        let mut l = format!("  > {}. {}", k + 1, s["text"].as_str().unwrap_or(""));
        if let Some(c) = s["line"].as_str() {
            l += &format!(": `{c}`");
        }
        note.push(l);
    }
    lines.splice(end..end, note);
    if let Err(code) = write(&lines) {
        return code;
    }
    let Some(link) = platform::link(Path::new(".")) else {
        println!("Wrote the request under task {n} in {PLAN}. Not linked: ask the person in this session.");
        return ExitCode::SUCCESS;
    };
    match link.post("cli/needs", &json!({ "task": n, "title": title, "steps": steps })) {
        Ok(_) => {
            println!(
                "The cabinet shows the request on task {n} until `exodus task done {n}` or `exodus need --clear`."
            );
            ExitCode::SUCCESS
        }
        Err(f) => told_not(&link, &f),
    }
}

/// The steps as the platform takes them, checked as it checks them (A36).
fn body(title: &str, steps: &[String]) -> Result<Vec<Value>, String> {
    let one = |what: &str, s: &str, max: usize| {
        if s.is_empty() || s.contains(['\n', '\r']) || s.chars().count() > max {
            Err(format!("{what}: one line of 1 to {max} characters"))
        } else {
            Ok(())
        }
    };
    one("the title", title, 200)?;
    if steps.len() > 12 {
        return Err("at most 12 steps".into());
    }
    steps
        .iter()
        .map(|s| {
            let (text, line) = match s.split_once('|') {
                Some((t, l)) => (t.trim(), Some(l.trim()).filter(|l| !l.is_empty())),
                None => (s.trim(), None),
            };
            one("a step", text, 300)?;
            let mut step = json!({ "text": text });
            if let Some(l) = line {
                one("a step's line", l, 300)?;
                step["line"] = json!(l);
            }
            Ok(step)
        })
        .collect()
}

fn plan_lines() -> Result<Vec<String>, ExitCode> {
    match std::fs::read_to_string(PLAN) {
        Ok(t) => Ok(t.lines().map(String::from).collect()),
        Err(_) => {
            eprintln!("exodus: no {PLAN}; `exodus plan` writes it from the report.");
            Err(ExitCode::FAILURE)
        }
    }
}

fn write(lines: &[String]) -> Result<(), ExitCode> {
    std::fs::write(PLAN, lines.join("\n") + "\n").map_err(|e| {
        eprintln!("exodus: cannot write {PLAN}: {e}");
        ExitCode::FAILURE
    })
}

/// The line of task `n`: "- [ ] n. " or "- [x] n. ".
fn find(lines: &[String], n: u32) -> Option<usize> {
    let tail = format!(" {n}. ");
    lines.iter().position(|l| {
        let l = l.trim_start();
        (l.starts_with("- [ ]") || l.starts_with("- [x]")) && l[5..].starts_with(&tail)
    })
}

fn missing(n: u32, lines: &[String]) -> ExitCode {
    let total = lines.iter().filter(|l| l.trim_start().starts_with("- [")).count();
    eprintln!("exodus: task {n} is not in {PLAN} (it has tasks 1 to {total})");
    ExitCode::FAILURE
}

/// Removes the "> Needs you" note, under task `at` only or anywhere; true when one was there.
fn drop_need(lines: &mut Vec<String>, at: Option<usize>) -> bool {
    let before = lines.len();
    let mut owner = None;
    let mut k = 0;
    lines.retain(|l| {
        if !l.starts_with("  ") {
            owner = Some(k);
        }
        k += 1;
        !(l.starts_with("  > ") && (at.is_none() || owner == at))
    });
    lines.len() != before
}

/// The file changed but the platform was not told: the reason, and that the same command again tells it.
fn told_not(link: &Link, f: &platform::Failure) -> ExitCode {
    eprintln!(
        "exodus: {PLAN} is written, the cabinet ({}) is not: {}. Run the same command again to tell it.",
        link.api,
        f.message()
    );
    super::failed_code(f)
}
