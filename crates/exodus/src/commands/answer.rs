//! `exodus answer`: records one answer to the audit's questions in `.exodus/answers.json`, with how the agent found
//! it (or nothing, when the person answered) and the machine value the report computes from (A14). The value is checked
//! here, at the file's trust boundary, so a report never prices a typo. With a linked account the answer also goes to
//! the platform (`PUT cli/answers`), so the waiting screen stops asking it; the file is what `exodus send` carries.

use std::path::Path;
use std::process::ExitCode;

use exodus_schema::{Answer, Answers, Findings, ProjectFile};
use serde_json::Value;

#[derive(clap::Args)]
pub struct Args {
    /// The question's id, as `.exodus/findings.json` lists it
    id: String,
    /// The answer in words, as the report shows it
    text: String,
    /// How you found it ("the railway CLI's usage for the last 30 days"); leave it out when the person answered
    #[arg(long)]
    found: Option<String>,
    /// What the report computes from: a number (visits, egress, growth, egressmb, filesmb), a region
    /// id (users, who), public, admin or none (connections: who holds them), or a JSON map of part id to cents a
    /// month, or {"total": cents} (bill)
    #[arg(long, value_name = "V", allow_negative_numbers = true)]
    value: Option<String>,
}

const DIR: &str = ".exodus";

pub fn run(args: Args) -> ExitCode {
    match answer(Path::new(DIR), &args, &super::now()) {
        Ok(open) => {
            println!("Recorded {}.", args.id);
            if let Some(link) = crate::platform::link(Path::new(".")) {
                match share(&link, Path::new(DIR), &args.id) {
                    Ok(()) => println!("The cabinet has it too."),
                    Err(m) => eprintln!("exodus: kept here only, the platform did not take it: {m}"),
                }
            }
            if open.is_empty() {
                println!("Every question has an answer. Next: `exodus report`.");
            } else {
                println!("Still open: {}.", open.join(", "));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exodus answer: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Sends the recorded answer `id` with the question's `affects` line.
fn share(link: &crate::platform::Link, dir: &Path, id: &str) -> Result<(), String> {
    let answers: Answers = super::report::read(&dir.join("answers.json"))?.unwrap_or_default();
    let findings: Option<Findings> = super::report::read(&dir.join("findings.json"))?;
    let a = answers.answers.iter().find(|a| a.id == id).ok_or("no such answer")?;
    let affects = findings
        .iter()
        .flat_map(|f| &f.questions)
        .find(|q| q.id == id)
        .map_or("", |q| q.affects.as_str());
    let body = serde_json::json!([{
        "id": a.id, "q": a.q, "answer": a.answer, "found": a.found, "value": a.value, "affects": affects,
    }]);
    link.put("cli/answers", &body).map(|_| ()).map_err(|f| f.message())
}

/// Writes the answer and returns the ids of the questions still open.
fn answer(dir: &Path, args: &Args, at: &str) -> Result<Vec<String>, String> {
    let project: Option<ProjectFile> = super::report::read(&dir.join("project.json"))?;
    let findings: Option<Findings> = super::report::read(&dir.join("findings.json"))?;
    let new = project.is_some_and(|p| p.kind == "new") || findings.as_ref().is_some_and(|f| f.kind == "new");
    if findings.is_none() && !new {
        return Err(format!("no {DIR}/findings.json: run `exodus audit` first"));
    }
    let asked: Vec<(String, String)> = findings
        .as_ref()
        .map(|f| f.questions.iter().map(|q| (q.id.clone(), q.q.clone())).collect())
        .unwrap_or_default();

    // the bank's entry of this kind: a new product's idea questions, or a move's (the audit's and the agent's own)
    let bank = exodus_audit::bank();
    // an agent's own question applies to a new product too (the forecast's rates)
    let entry = bank
        .iter()
        .find(|b| b.id == args.id && ((b.when == "new") == new || b.when == "agent"));
    let q = match asked.iter().find(|(id, _)| *id == args.id) {
        Some((_, q)) => q.clone(),
        None => match entry {
            Some(b) if new || b.when == "agent" => b.q.replace("{platform}", "your host"),
            _ => {
                let ids: Vec<&str> = if asked.is_empty() {
                    bank.iter().filter(|b| b.when == "new").map(|b| b.id.as_str()).collect()
                } else {
                    asked.iter().map(|(id, _)| id.as_str()).collect()
                };
                return Err(format!(
                    "`{}` is not a question here; the questions are: {}",
                    args.id,
                    ids.join(", ")
                ));
            }
        },
    };
    if args.text.trim().is_empty() {
        return Err("the answer is empty".into());
    }
    // a report can be shared by link: an answer says whether an account exists, never which one
    if [Some(&args.text), args.found.as_ref()]
        .into_iter()
        .flatten()
        .any(|t| billing_id(t))
    {
        return Err("an answer carries no billing account id; say whether there is one and how many".into());
    }
    let value = match (&args.value, entry.and_then(|b| b.value.as_deref())) {
        (None, _) => None,
        (Some(_), None) => return Err(format!("`{}` carries no value; leave out --value", args.id)),
        (Some(v), Some(kind)) => Some(value(kind, v, findings.as_ref())?),
    };

    let path = dir.join("answers.json");
    let mut answers: Answers = super::report::read(&path)?.unwrap_or_default();
    answers.exodus = exodus_schema::ANSWERS.into();
    answers.answers.retain(|a| a.id != args.id);
    answers.answers.push(Answer {
        id: args.id.clone(),
        q,
        answer: args.text.trim().into(),
        found: args
            .found
            .as_ref()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty()),
        value,
        at: at.into(),
    });
    let json = serde_json::to_string_pretty(&answers).map_err(|e| e.to_string())? + "\n";
    std::fs::create_dir_all(dir)
        .and_then(|_| std::fs::write(&path, json))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    let ids: Vec<String> = if asked.is_empty() {
        bank.iter().filter(|b| b.when == "new").map(|b| b.id.clone()).collect()
    } else {
        asked.into_iter().map(|(id, _)| id).collect()
    };
    Ok(ids
        .into_iter()
        .filter(|id| answers.answers.iter().all(|a| a.id != *id))
        .collect())
}

/// The machine value of an answer, checked against its kind (A14).
fn value(kind: &str, v: &str, findings: Option<&Findings>) -> Result<Value, String> {
    match kind {
        "visits" | "egress" | "growth" | "rate" => {
            let n: f64 = v
                .trim()
                .parse()
                .map_err(|_| format!("--value for {kind} is a number, not {v:?}"))?;
            if !n.is_finite() || (n < 0.0 && kind != "growth") {
                return Err(format!("--value for {kind} is a number of 0 or more, not {v}"));
            }
            Ok(if n.fract() == 0.0 && n.abs() < 1e15 {
                Value::from(n as i64)
            } else {
                Value::from(n)
            })
        }
        "connections" => match v.trim().to_lowercase().as_str() {
            k @ ("public" | "admin" | "none") => Ok(Value::from(k)),
            _ => Err(
                "--value for connections is public (the app's users hold them), admin (only an admin page) or none"
                    .into(),
            ),
        },
        "users" => {
            let regions = exodus_cost::bundled().regions;
            if regions.contains_key(v.trim()) {
                Ok(Value::from(v.trim()))
            } else {
                let ids: Vec<&str> = regions.keys().map(String::as_str).collect();
                Err(format!("--value is a region id, one of: {}", ids.join(", ")))
            }
        }
        "bill" => {
            let map: serde_json::Map<String, Value> = serde_json::from_str(v).map_err(
                |_| r#"--value for bill is a JSON map of part id to cents, like '{"web": 2000, "other": 500}', or one total, '{"total": 2500}'"#,
            )?;
            let parts: Vec<&str> = findings
                .map(|f| {
                    f.processes
                        .iter()
                        .map(|p| p.id.as_str())
                        .chain(f.data.iter().map(|d| d.id.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            for (k, c) in &map {
                if !["total", "other", "egress"].contains(&k.as_str()) && !parts.contains(&k.as_str()) {
                    return Err(format!(
                        "`{k}` is not a part; the parts are: {}, egress, other for the rest, or total for the whole bill",
                        parts.join(", ")
                    ));
                }
                if c.as_u64().is_none() {
                    return Err(format!("the bill of `{k}` is whole cents of 0 or more, not {c}"));
                }
            }
            Ok(Value::Object(map))
        }
        "platform" => {
            // the names the audit and the prices use; another host keeps the name it was given
            let known = [
                "Railway",
                "Render",
                "Fly.io",
                "Heroku",
                "Vercel",
                "Netlify",
                "Firebase",
                "GitHub Pages",
            ];
            let v = v.trim();
            if v.is_empty() || v.chars().count() > 40 {
                return Err("--value for platform is the host's name, like Railway or Fly.io".into());
            }
            let bare = |s: &str| s.to_ascii_lowercase().replace(['.', ' '], "").replace("io", "");
            Ok(Value::from(
                known.iter().find(|k| bare(k) == bare(v)).copied().unwrap_or(v),
            ))
        }
        _ => Err(format!("this exodus cannot read a {kind} value")),
    }
}

/// Whether `s` holds a billing account id, three groups of six hex digits: `01ABCD-23EF45-6789AB`.
pub(super) fn billing_id(s: &str) -> bool {
    s.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').any(|w| {
        let g: Vec<&str> = w.split('-').collect();
        g.len() == 3
            && g.iter()
                .all(|x| x.len() == 6 && x.chars().all(|c| c.is_ascii_hexdigit()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_platform_value_is_the_name_the_prices_use() {
        assert_eq!(value("platform", " fly ", None).unwrap(), "Fly.io");
        assert_eq!(value("platform", "railway", None).unwrap(), "Railway");
        assert_eq!(value("platform", "a VPS at home", None).unwrap(), "a VPS at home");
        assert!(value("platform", "", None).is_err());
    }
}
