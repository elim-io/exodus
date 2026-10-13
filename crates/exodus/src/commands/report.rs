//! `exodus report`: builds `.exodus/report.json` and `.exodus/report.md` from the audit's findings and the answers,
//! here, with the prices bundled in this build, linked account or not; nothing is uploaded (`exodus send` does that,
//! SPEC exodus-fix F2). With a report already there, the new one is the next version and says what changed; when
//! nothing changed, the old one stays. `--pull` fetches the platform's latest instead, `--report N` version N.

use std::path::Path;
use std::process::ExitCode;

use exodus_schema::{Answers, Findings, Report};

#[derive(clap::Args)]
pub struct Args {
    /// Fetch the latest report from the platform instead of building one (needs a linked account)
    #[arg(long)]
    pull: bool,
    /// Fetch report version N from the platform
    #[arg(long, value_name = "N")]
    report: Option<u32>,
    /// Print the Markdown on stdout
    #[arg(long)]
    md: bool,
    /// Print the JSON on stdout
    #[arg(long)]
    json: bool,
    /// Accepted for older instructions: the report is always built here unless --pull
    #[arg(long, hide = true)]
    offline: bool,
}

const DIR: &str = ".exodus";

pub fn run(args: Args) -> ExitCode {
    let file = |name: &str| std::fs::read_to_string(Path::new(DIR).join(name)).unwrap_or_default();
    if args.pull || args.report.is_some() {
        let n = args.report.map_or_else(|| "latest".to_string(), |n| n.to_string());
        let Some(link) = crate::platform::link(Path::new(".")) else {
            eprintln!(
                "exodus report: the platform's reports need a linked account. `exodus report` builds one here; `exodus init --code` links one."
            );
            return ExitCode::from(2);
        };
        let code = super::send::pull(&link, &n, args.json || args.md);
        if code == ExitCode::SUCCESS {
            if args.json {
                print!("{}", file("report.json"));
            }
            if args.md {
                print!("{}", file("report.md"));
            }
        }
        return code;
    }
    match build(Path::new(DIR)) {
        Ok((report, md, kept)) => {
            if args.json {
                print!("{}", json(&report));
            }
            if args.md {
                print!("{md}");
            }
            if !args.json && !args.md {
                let what = if report.project.kind == "new" { "plan" } else { "report" };
                if kept {
                    println!("Nothing changed since {what} {}; kept it.", report.n);
                } else {
                    println!("{}.", report.verdict.title);
                }
                println!(
                    "Wrote {DIR}/report.json and {DIR}/report.md ({what} {}, Google prices read {}).",
                    report.n, report.prices.date
                );
                if crate::platform::link(Path::new(".")).is_some() {
                    println!("Built here, not uploaded: `exodus send` puts it in the cabinet.");
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exodus report: {e}");
            ExitCode::FAILURE
        }
    }
}

fn json(r: &Report) -> String {
    serde_json::to_string_pretty(r).map_or_else(|e| format!("{{\"error\": \"{e}\"}}"), |j| j + "\n")
}

/// Reads the findings, the answers and the previous report in `dir`, builds the next one and writes both files.
/// Returns the report, its Markdown and whether the previous one was kept because nothing changed.
fn build(dir: &Path) -> Result<(Report, String, bool), String> {
    let findings: Findings = match read(&dir.join("findings.json"))? {
        Some(f) => f,
        None => return Err(format!("no {DIR}/findings.json: run `exodus audit` first")),
    };
    if findings.exodus != exodus_schema::FINDINGS {
        return Err(format!(
            "{DIR}/findings.json is {:?}, this exodus reads {:?}",
            findings.exodus,
            exodus_schema::FINDINGS
        ));
    }
    let answers: Answers = read(&dir.join("answers.json"))?.unwrap_or_default();
    if !answers.answers.is_empty() && answers.exodus != exodus_schema::ANSWERS {
        return Err(format!(
            "{DIR}/answers.json is {:?}, this exodus reads {:?}",
            answers.exodus,
            exodus_schema::ANSWERS
        ));
    }
    let prev: Option<Report> = read(&dir.join("report.json")).ok().flatten();
    // a report the platform built carries a token; one of another project or kind is not this one's history
    let prev = prev.filter(|p| p.project.name == findings.repo.name && p.project.kind == findings.kind);

    let input = exodus_report::Input {
        findings: &findings,
        answers: &answers.answers,
        prev: prev.as_ref(),
        at: &super::now(),
        agent: None,
    };
    let mut report = exodus_report::build(&input, &exodus_cost::bundled(), &exodus_cost::platforms())?;
    let mut kept = false;
    if let Some(p) = prev
        && report.since.as_ref().is_some_and(|s| s.lines.is_empty())
    {
        report = p;
        kept = true;
    }
    let md = exodus_report::markdown(&report, exodus_report::SITE, exodus_report::APP);
    std::fs::write(dir.join("report.json"), json(&report))
        .and_then(|_| std::fs::write(dir.join("report.md"), &md))
        .map_err(|e| format!("cannot write {DIR}/: {e}"))?;
    Ok((report, md, kept))
}

/// A JSON file, or `None` when it does not exist.
pub(super) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
