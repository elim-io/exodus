//! `exodus check` (architecture §3): the audit's rules as a CI step. It prints what breaks or idles on Cloud Run, as
//! plain lines or GitHub annotations, and exits 1 when a finding of a `--fail-on` kind is there. With `EXODUS_KEY` in
//! the environment (never the machine's credentials) it tells the cabinet about the run; that report never changes
//! the exit code, so CI does not fail because the platform is away.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exodus_audit::rules::Rule;
use exodus_schema::Pattern;
use serde_json::json;

#[derive(clap::Args)]
pub struct Args {
    /// plain or github (workflow annotations)
    #[arg(long, default_value = "plain", value_parser = ["plain", "github"])]
    format: String,
    /// The kinds that fail the run
    #[arg(long, value_name = "KINDS", default_value = "breaks,idle", value_delimiter = ',', value_parser = ["breaks", "idle"])]
    fail_on: Vec<String>,
    /// A rule file, or a folder of them, added to the built-in rules (a rule with a built-in id replaces it)
    #[arg(long, value_name = "PATH")]
    rules: Option<PathBuf>,
}

pub fn run(args: Args) -> ExitCode {
    let root = Path::new(".");
    let mut rules = exodus_audit::rules::builtin();
    if let Some(path) = &args.rules {
        match own_rules(path) {
            Ok(own) => {
                rules.retain(|r| !own.iter().any(|o| o.file.id == r.file.id));
                rules.extend(own);
            }
            Err(e) => {
                eprintln!("exodus check: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let findings = match exodus_audit::audit_with(root, &rules, usize::MAX) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("exodus check: {e}");
            return ExitCode::FAILURE;
        }
    };
    let found: Vec<&Pattern> = findings
        .patterns
        .iter()
        .filter(|p| p.kind == "breaks" || p.kind == "idle")
        .collect();
    let fails = |p: &Pattern| args.fail_on.contains(&p.kind);
    for p in &found {
        if args.format == "github" {
            println!("{}", annotation(p, fails(p)));
        } else {
            println!("{}: {}: {} ({})\n  {}", p.at, p.kind, p.title, p.rule, p.fix);
        }
    }
    let failing = found.iter().filter(|p| fails(p)).count();
    if found.is_empty() {
        println!(
            "exodus check: nothing that breaks or idles on Cloud Run ({} rules).",
            rules.len()
        );
    } else if failing == 0 {
        println!(
            "exodus check: {} found, none of a kind that fails the run ({}).",
            found.len(),
            args.fail_on.join(", ")
        );
    } else {
        println!(
            "exodus check: {failing} found that fail the run ({}).",
            args.fail_on.join(", ")
        );
    }
    tell(root, &found);
    if failing > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `::error file=src/jobs.js,line=12,title=...::why fix`, escaped as GitHub's workflow commands want.
fn annotation(p: &Pattern, fails: bool) -> String {
    let data = |s: &str| s.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A");
    let prop = |s: &str| data(s).replace(':', "%3A").replace(',', "%2C");
    let (file, line) = match p.at.rsplit_once(':') {
        Some((f, l)) if l.parse::<u32>().is_ok() => (f, Some(l)),
        _ => (p.at.as_str(), None),
    };
    let mut s = format!("::{} file={}", if fails { "error" } else { "warning" }, prop(file));
    if let Some(l) = line {
        s += &format!(",line={l}");
    }
    s + &format!(
        ",title={}::{} {}",
        prop(&format!("exodus: {}", p.title)),
        data(&p.why),
        data(&p.fix)
    )
}

/// The rules of a file, or of every `.toml` in a folder (sorted, one level).
fn own_rules(path: &Path) -> Result<Vec<Rule>, String> {
    let files = if path.is_dir() {
        let mut v: Vec<PathBuf> = std::fs::read_dir(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };
    files
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
            Rule::parse(&text).map_err(|e| format!("{}: {e}", f.display()))
        })
        .collect()
}

/// `POST cli/check-runs` with `EXODUS_KEY` only: the commit, the CI file that runs the check, and what it found.
fn tell(root: &Path, found: &[&Pattern]) {
    let Some(link) = crate::platform::ci_link(root) else {
        return;
    };
    let Some(commit) = commit() else {
        eprintln!("exodus check: no commit (GITHUB_SHA, CI_COMMIT_SHA or git), so the cabinet was not told");
        return;
    };
    let cut = |s: &str, n: usize| s.replace(['\n', '\r'], " ").chars().take(n).collect::<String>();
    let hits: Vec<_> = found
        .iter()
        .take(200)
        .map(|p| json!({ "rule": cut(&p.rule, 64), "at": cut(&p.at, 300), "text": cut(&p.title, 500) }))
        .collect();
    let body = json!({ "commit": commit, "file": ci_file(root), "found": hits });
    match link.post("cli/check-runs", &body) {
        Ok(_) => eprintln!("exodus check: the cabinet has this run ({}).", &commit[..7]),
        Err(f) => eprintln!("exodus check: the cabinet was not told: {}", f.message()),
    }
}

fn commit() -> Option<String> {
    let ok = |c: String| {
        let c = c.trim().to_ascii_lowercase();
        ((7..=40).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_hexdigit())).then_some(c)
    };
    ["GITHUB_SHA", "CI_COMMIT_SHA"]
        .iter()
        .find_map(|v| std::env::var(v).ok().and_then(ok))
        .or_else(|| {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| ok(String::from_utf8_lossy(&out.stdout).into_owned()))
                .flatten()
        })
}

/// The first CI file that runs `exodus check`: GitHub workflows (sorted), GitLab, CircleCI, Jenkins.
fn ci_file(root: &Path) -> Option<String> {
    let mut workflows: Vec<String> = std::fs::read_dir(root.join(".github/workflows"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .map(|n| format!(".github/workflows/{n}"))
        .collect();
    workflows.sort();
    workflows
        .into_iter()
        .chain([".gitlab-ci.yml", ".circleci/config.yml", "Jenkinsfile"].map(String::from))
        .find(|f| std::fs::read_to_string(root.join(f)).is_ok_and(|t| t.contains("exodus check")))
}
