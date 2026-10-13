//! `exodus audit`: reads the repository in the current folder into `.exodus/findings.json` and `.exodus/audit.log`.
//! With a linked account (and without `--offline`) it posts its steps to the platform's waiting screen (§4
//! `audit/events`): step 1 before reading, then step 3 with what it found and the questions it asks. A post that fails
//! is a line on stderr; the audit itself never needs the platform.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

#[derive(clap::Args)]
pub struct Args {
    /// Print the findings as JSON on stdout
    #[arg(long)]
    json: bool,
    /// Print nothing but errors
    #[arg(long)]
    quiet: bool,
    /// Do not stream progress to the platform (the audit itself never needs it)
    #[arg(long)]
    offline: bool,
}

pub fn run(args: Args) -> ExitCode {
    let root = Path::new(".");
    let link = if args.offline {
        None
    } else {
        crate::platform::link(root)
    };
    let post = |body: serde_json::Value| {
        if let Some(l) = &link
            && let Err(f) = l.post("cli/audit/events", &body)
        {
            eprintln!("exodus audit: the waiting screen was not told: {}", f.message());
        }
    };
    post(serde_json::json!({ "step": 1, "doing": "Reading the manifests" }));
    let started = super::now();
    let mut findings = match exodus_audit::audit(root) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("exodus audit: {e}");
            return ExitCode::FAILURE;
        }
    };
    findings.stats.started_at = started;
    findings.stats.finished_at = super::now();
    findings.cli = env!("CARGO_PKG_VERSION").into();
    // what the agent wrote with `exodus describe` is its reading of the code, not the scanner's: an audit keeps it
    let before: Option<exodus_schema::Findings> =
        super::report::read(Path::new(".exodus/findings.json")).ok().flatten();
    findings.understanding = before.and_then(|b| b.understanding);
    if let Some((owner, name)) = super::origin(root) {
        findings.repo.owner = Some(owner);
        findings.repo.name = name;
    }

    let json = match serde_json::to_string_pretty(&findings) {
        Ok(j) => j + "\n",
        Err(e) => {
            eprintln!("exodus audit: {e}");
            return ExitCode::FAILURE;
        }
    };
    let log = log(&findings);
    let written = std::fs::create_dir_all(".exodus")
        .and_then(|_| std::fs::write(".exodus/findings.json", &json))
        .and_then(|_| std::fs::write(".exodus/audit.log", &log));
    if let Err(e) = written {
        eprintln!("exodus audit: cannot write .exodus/: {e}");
        return ExitCode::FAILURE;
    }
    if link.is_some() {
        post(events(&findings));
    }
    if args.json {
        print!("{json}");
    } else if !args.quiet {
        print!("{log}");
    }
    ExitCode::SUCCESS
}

/// Step 3 for the waiting screen: the parts and patterns found (one line each, at most 200) and the questions.
fn events(f: &exodus_schema::Findings) -> serde_json::Value {
    let at = &f.stats.finished_at;
    let line = |text: String, pattern: bool| {
        let text: String = text.chars().take(300).collect();
        serde_json::json!({ "at": at, "text": text, "pattern": pattern })
    };
    let found: Vec<_> = f
        .processes
        .iter()
        .map(|p| line(format!("{} process {}: {}", p.kind, p.id, p.command), false))
        .chain(
            f.data
                .iter()
                .map(|d| line(format!("{} {} ({})", d.kind, d.id, d.engine), false)),
        )
        .chain(f.patterns.iter().map(|p| line(format!("{} ({})", p.title, p.at), true)))
        .take(200)
        .collect();
    let questions: Vec<_> = f
        .questions
        .iter()
        .map(|q| serde_json::json!({ "id": q.id, "q": q.q, "affects": q.affects }))
        .collect();
    let doing = if questions.is_empty() {
        "Read the code; nothing to ask".to_string()
    } else {
        format!("Questions for you: {}", questions.len())
    };
    serde_json::json!({ "step": 3, "doing": doing, "found": found, "questions": questions })
}

/// What the audit read, one line each, as the waiting screen's step 1 tells it.
fn log(f: &exodus_schema::Findings) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "read {} files ({} bytes)", f.stats.files, f.stats.bytes);
    let _ = writeln!(
        s,
        "platform: {} ({})",
        f.platform.name,
        if f.platform.file.is_empty() {
            "-"
        } else {
            &f.platform.file
        }
    );
    let _ = writeln!(s, "stack: {}", f.stack.join(", "));
    for p in &f.processes {
        let _ = writeln!(s, "process {} ({}): {} [{}]", p.id, p.kind, p.command, p.from);
    }
    for d in &f.data {
        let _ = writeln!(s, "data {} ({} {}) [{}]", d.id, d.kind, d.engine, d.from);
    }
    let _ = writeln!(
        s,
        "env names: {} ({} look like secrets; values not read)",
        f.settings.variables_total, f.settings.secrets_guess
    );
    for p in &f.patterns {
        let _ = writeln!(s, "pattern {} ({}): {} [{}]", p.id, p.kind, p.title, p.at);
    }
    let clean = exodus_audit::split_clean(&f.clean);
    if !clean.looked_for.is_empty() {
        let _ = writeln!(s, "looked for, not found: {}", clean.looked_for.join(", "));
    }
    for c in &clean.not_on_server {
        let _ = writeln!(s, "not on the server: {c}");
    }
    for c in &clean.stays {
        let _ = writeln!(s, "stays as it is: {c}");
    }
    let _ = writeln!(
        s,
        "questions: {}",
        f.questions.iter().map(|q| q.id.as_str()).collect::<Vec<_>>().join(", ")
    );
    let _ = writeln!(s, "wrote .exodus/findings.json");
    s
}
