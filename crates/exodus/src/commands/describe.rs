//! `exodus describe`: records what the agent understood from reading the code, three short paragraphs the report opens
//! with ("Our understanding"), into `.exodus/findings.json`. The text comes on stdin or from a file, never on the
//! command line, labelled `project:`, `today:` and `plan:`; `exodus send` carries it with the findings.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exodus_schema::{Findings, Understanding};

#[derive(clap::Args)]
pub struct Args {
    /// Read the text from this file instead of stdin
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
}

const DIR: &str = ".exodus";
/// The whole text, at most; three paragraphs are a few hundred words.
const INPUT_MAX: u64 = 16 * 1024;
/// One paragraph, at most (the platform takes up to 2,000 characters a string).
const FIELD_MAX: usize = 1200;
const LABELS: [&str; 3] = ["project", "today", "plan"];

const USAGE: &str = "give the text on stdin or with --file, three paragraphs labelled like this:
project: what the product is and who it is for
today: how it is built and runs now: its parts, where the data lives, where it is deployed, what is outside
plan: what the move changes and what that gives";

pub fn run(args: Args) -> ExitCode {
    match describe(Path::new(DIR), &args) {
        Ok(()) => {
            println!(
                "Wrote our understanding into {DIR}/findings.json. `exodus send` (or `exodus report` without an account) puts it at the top of the report."
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exodus describe: {e}");
            ExitCode::FAILURE
        }
    }
}

fn describe(dir: &Path, args: &Args) -> Result<(), String> {
    let text = match &args.file {
        Some(p) => read(std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?)?,
        None if std::io::stdin().is_terminal() => return Err(USAGE.into()),
        None => read(std::io::stdin())?,
    };
    let understanding = parse(&text)?;
    let path = dir.join("findings.json");
    let mut findings: Findings = super::report::read(&path)?.ok_or_else(|| {
        format!("no {DIR}/findings.json: run `exodus audit` first (for a new product, write it first)")
    })?;
    findings.understanding = Some(understanding);
    let json = serde_json::to_string_pretty(&findings).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn read(from: impl Read) -> Result<String, String> {
    let mut text = String::new();
    from.take(INPUT_MAX + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read the text: {e}"))?;
    if text.len() as u64 > INPUT_MAX {
        return Err(format!(
            "the text is over {} KB; three short paragraphs are enough",
            INPUT_MAX / 1024
        ));
    }
    Ok(text)
}

/// The three paragraphs: a line that starts with a label opens one, the lines after it continue it.
fn parse(text: &str) -> Result<Understanding, String> {
    let mut fields: [String; 3] = Default::default();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let opened = line.split_once(':').and_then(|(head, rest)| {
            let i = LABELS.iter().position(|l| l.eq_ignore_ascii_case(head.trim()))?;
            Some((i, rest))
        });
        let rest = match (opened, current) {
            (Some((i, rest)), _) => {
                if !fields[i].is_empty() {
                    return Err(format!("`{}:` is given twice", LABELS[i]));
                }
                current = Some(i);
                rest
            }
            (None, Some(_)) => line,
            (None, None) if line.trim().is_empty() => continue,
            (None, None) => return Err(format!("the text starts before a label; {USAGE}")),
        };
        if let Some(i) = current {
            fields[i].push(' ');
            fields[i].push_str(rest.trim());
        }
    }
    for (label, field) in LABELS.iter().zip(fields.iter_mut()) {
        *field = field.split_whitespace().collect::<Vec<_>>().join(" ");
        if field.is_empty() {
            return Err(format!("`{label}:` is missing or empty; {USAGE}"));
        }
        if field.chars().count() > FIELD_MAX {
            return Err(format!(
                "`{label}:` is over {FIELD_MAX} characters; a few sentences are enough"
            ));
        }
        // a report can be shared by a link: it says whether an account exists, never which one
        if super::answer::billing_id(field) {
            return Err(format!("`{label}:` carries a billing account id; leave it out"));
        }
    }
    let [project, today, plan] = fields;
    Ok(Understanding { project, today, plan })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::parse;

    #[test]
    fn three_labelled_paragraphs_over_several_lines() {
        let u = parse(
            "Project: An API for bookings.\nIt serves small studios.\n\ntoday: One web process\n  and Postgres.\nplan:  Move to Cloud Run.\n",
        )
        .unwrap();
        assert_eq!(u.project, "An API for bookings. It serves small studios.");
        assert_eq!(u.today, "One web process and Postgres.");
        assert_eq!(u.plan, "Move to Cloud Run.");
    }

    #[test]
    fn a_missing_twice_given_or_unlabelled_paragraph_is_refused() {
        assert!(
            parse("project: a\ntoday: b\n")
                .unwrap_err()
                .contains("`plan:` is missing")
        );
        assert!(
            parse("project: a\nproject: b\ntoday: c\nplan: d")
                .unwrap_err()
                .contains("twice")
        );
        assert!(
            parse("Hello\nproject: a\ntoday: b\nplan: c")
                .unwrap_err()
                .contains("before a label")
        );
        assert!(
            parse(&format!("project: {}\ntoday: b\nplan: c", "x ".repeat(700)))
                .unwrap_err()
                .contains("over")
        );
        assert!(parse("project: a\ntoday: billed to 01ABCD-23EF45-6789AB\nplan: c").is_err());
    }
}
