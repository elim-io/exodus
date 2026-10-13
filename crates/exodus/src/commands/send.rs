//! `exodus send` and `exodus report --pull` (architecture §3, §4.4): the findings and answers go to the platform, which
//! builds the next report version; the version comes back into `.exodus/report.json` and `.exodus/report.md` as the
//! cabinet shows it. `send` is the only command that uploads (SPEC exodus-fix F2). Both need a linked account;
//! without one they say so and exit 2.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exodus_schema::{Answers, Findings};
use serde_json::{Value, json};

use crate::platform::{self, Failure, Link};

#[derive(clap::Args)]
pub struct Args {
    /// The findings to send
    #[arg(long, value_name = "PATH", default_value = ".exodus/findings.json")]
    pub findings: PathBuf,
    /// The coding agent that ran the audit (Claude Code, Codex, Cursor); the report names it
    #[arg(long, value_name = "NAME")]
    pub agent: Option<String>,
}

#[derive(clap::Args)]
pub struct PullArgs {
    /// The version to pull (default the latest)
    #[arg(long, value_name = "N")]
    pub report: Option<u32>,
}

const DIR: &str = ".exodus";

pub fn run(args: Args) -> ExitCode {
    let Some(link) = platform::link(Path::new(".")) else {
        eprintln!("Sending needs a linked account. `exodus report` builds the report here.");
        return ExitCode::from(2);
    };
    let body = match upload(&args) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("exodus send: {e}");
            return ExitCode::FAILURE;
        }
    };
    let sent = match link.post("cli/reports", &body) {
        Ok(v) => v,
        Err(f) => return super::failed(&f),
    };
    let Some(n) = sent["n"].as_u64() else {
        eprintln!("exodus send: the platform answered without a report number");
        return ExitCode::from(2);
    };
    let code = pull(&link, &n.to_string(), false);
    if code == ExitCode::SUCCESS {
        // a new product's report is its plan, as `exodus report` and the Markdown call it
        let (what, label) = if body["findings"]["kind"] == "new" {
            ("plan", "Plan")
        } else {
            ("report", "Report")
        };
        if sent["unchanged"] == json!(true) {
            println!(
                "Nothing new since {what} {n}: the findings and answers are the ones it was built from, so it stays the latest."
            );
        }
        println!("{label} {n}: {}", sent["url"].as_str().unwrap_or(""));
        if sent["shared"] == json!(true) {
            println!("Shared by link.");
        } else {
            println!("Private: only you can open it until you share it on the report's page.");
        }
    }
    code
}

/// `{findings, answers, cli, agent}` from the files, checked enough that the platform's refusal is not the first word.
fn upload(args: &Args) -> Result<Value, String> {
    let findings: Findings = super::report::read(&args.findings)?
        .ok_or_else(|| format!("no {}: run `exodus audit` first", args.findings.display()))?;
    if findings.exodus != exodus_schema::FINDINGS {
        return Err(format!(
            "{} is {:?}, this exodus sends {:?}",
            args.findings.display(),
            findings.exodus,
            exodus_schema::FINDINGS
        ));
    }
    let answers: Option<Answers> = super::report::read(&Path::new(DIR).join("answers.json"))?;
    let agent = args.agent.as_deref().map(str::trim).filter(|a| !a.is_empty());
    Ok(json!({
        "findings": findings,
        "answers": answers.filter(|a| !a.answers.is_empty()),
        "cli": env!("CARGO_PKG_VERSION"),
        "agent": agent,
    }))
}

pub fn pull_command(args: PullArgs) -> ExitCode {
    let Some(link) = platform::link(Path::new(".")) else {
        eprintln!("Pulling a report needs a linked account. `exodus report` builds one here.");
        return ExitCode::from(2);
    };
    let n = args.report.map_or_else(|| "latest".to_string(), |n| n.to_string());
    pull(&link, &n, false)
}

/// Writes report `n` (a number or `latest`) into `.exodus/` and says which it was.
pub fn pull(link: &Link, n: &str, quiet: bool) -> ExitCode {
    let report = match link.get(&format!("cli/reports/{n}"), &[]) {
        Ok(v) => v,
        Err(Failure::Refused(404, _)) => {
            eprintln!("exodus: the platform holds no report {n} of this project; `exodus send` makes one.");
            return ExitCode::from(3);
        }
        Err(f) => return super::failed(&f),
    };
    // `latest` resolved: the Markdown of the same version, even if another lands in between
    let n = report["n"].as_u64().map_or_else(|| n.to_string(), |v| v.to_string());
    let md = match link.get_text(&format!("cli/reports/{n}/report.md"), &[]) {
        Ok(t) => t,
        Err(f) => return super::failed(&f),
    };
    let text = serde_json::to_string_pretty(&report).unwrap_or_default() + "\n";
    let written = std::fs::create_dir_all(DIR)
        .and_then(|_| std::fs::write(Path::new(DIR).join("report.json"), text))
        .and_then(|_| std::fs::write(Path::new(DIR).join("report.md"), &md));
    if let Err(e) = written {
        eprintln!("exodus: cannot write {DIR}/: {e}");
        return ExitCode::FAILURE;
    }
    if quiet {
        return ExitCode::SUCCESS;
    }
    let what = if report["project"]["kind"] == "new" {
        "plan"
    } else {
        "report"
    };
    if let Some(title) = report["verdict"]["title"].as_str() {
        println!("{title}.");
    }
    println!("Wrote {DIR}/report.json and {DIR}/report.md ({what} {n}, built by the platform).");
    ExitCode::SUCCESS
}
