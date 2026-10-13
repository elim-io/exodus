//! `exodus init`: lays out `.exodus/` (git-ignored) and the instructions for the coding agent that works here: the
//! Claude Code skill, the Exodus section of `AGENTS.md` (Codex and the others) and the Cursor rule. Running it again
//! rewrites them from this build's templates and leaves everything else in those files alone. With `--code` it first
//! exchanges the cabinet's one-time code for a project key (architecture §7), which goes to the credentials file and
//! nowhere else, and the kind of the code decides the templates (A17). `exodus login --code` is the exchange alone.

use std::path::Path;
use std::process::ExitCode;

use exodus_schema::ProjectFile;
use serde_json::json;

use crate::platform::{self, Failure};

#[derive(clap::Args)]
pub struct Args {
    /// The one-time code from the cabinet (K7QM-4XPT): links this repository to your account
    #[arg(long, value_name = "CODE")]
    code: Option<String>,
    /// The platform's address
    #[arg(long, value_name = "URL")]
    api: Option<String>,
    /// Plan a new product in this folder instead of moving the code that is here
    #[arg(long)]
    new: bool,
    /// Link this folder with --code even when it is not the top folder of a git repository (a part of a monorepo, code
    /// kept without git)
    #[arg(long)]
    here: bool,
    /// The agents to lay out instructions for
    #[arg(long, value_enum, value_delimiter = ',', default_value = "claude,codex,cursor")]
    agent: Vec<Agent>,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum Agent {
    Claude,
    Codex,
    Cursor,
}

pub const MOVE: &str = include_str!("../../templates/move.md");
pub const NEW: &str = include_str!("../../templates/new.md");
const SKILL: &str = include_str!("../../templates/skill.md");
const AGENTS: &str = include_str!("../../templates/agents.md");
const CURSOR: &str = include_str!("../../templates/cursor.mdc");
const README: &str = include_str!("../../templates/readme.md");

/// The lines of `AGENTS.md` that open and close the section `init` owns.
const START: &str = "<!-- exodus:start";
const END: &str = "<!-- exodus:end -->";

#[derive(clap::Args)]
pub struct LoginArgs {
    /// A one-time code from the cabinet (K7QM-4XPT)
    #[arg(long, value_name = "CODE")]
    code: String,
    /// The platform's address
    #[arg(long, value_name = "URL")]
    api: Option<String>,
}

/// What the exchange gave: the project the key opens, the kind and the address that answered.
struct Linked {
    id: String,
    owner: Option<String>,
    name: String,
    kind: String,
    api: String,
}

pub fn run(args: Args) -> ExitCode {
    let root = Path::new(".");
    if let Some(code) = args.code.as_deref().filter(|_| !args.here) {
        let home = std::env::var_os("HOME").and_then(|h| std::fs::canonicalize(h).ok());
        let again = format!(
            "{} init --code {}{}{}",
            exe(home.as_deref()),
            quote(code.trim()),
            args.api
                .as_deref()
                .map(|a| format!(" --api {}", quote(a)))
                .unwrap_or_default(),
            if args.new { " --new" } else { "" }
        );
        let checked = std::fs::canonicalize(root)
            .map_err(|e| format!("this folder: {e}"))
            .and_then(|here| linkable(&here, home.as_deref(), args.new, &again));
        if let Err(why) = checked {
            eprintln!("exodus init: {why}");
            return ExitCode::FAILURE;
        }
    }
    let linked = match &args.code {
        Some(code) => match exchange(root, code, args.api.as_deref()) {
            Ok(l) => Some(l),
            Err(code) => return code,
        },
        None => None,
    };
    match init(root, &args, linked.as_ref()) {
        Ok(wrote) => {
            println!("Wrote {}.", wrote.join(", "));
            let new = linked.as_ref().map_or(args.new, |l| l.kind == "new");
            match &linked {
                Some(l) => println!("Linked to {}.", l.label()),
                None => println!(
                    "No account linked. `exodus audit` and `exodus report` work; `exodus init --code` links one."
                ),
            }
            println!(
                "Next: ask your agent to {} as the exodus instructions say.",
                if new {
                    "plan the new product"
                } else {
                    "audit this repository"
                }
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exodus init: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `exodus login --code`: a new key for this machine on the project of this repository (the old key stays until it is
/// revoked in Settings); the agent files are left as they are.
pub fn login(args: LoginArgs) -> ExitCode {
    let root = Path::new(".");
    let path = root.join(".exodus/project.json");
    let before: ProjectFile = match super::report::read(&path) {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("exodus login: no .exodus/project.json here; `exodus init --code` links a repository.");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("exodus login: {e}");
            return ExitCode::FAILURE;
        }
    };
    let api = args.api.as_deref().or(before.api.as_deref());
    let l = match exchange(root, &args.code, api) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let project = ProjectFile {
        project: Some(l.id.clone()),
        api: Some(l.api.clone()),
        owner: l.owner.clone().or(before.owner),
        name: before.name,
        kind: l.kind.clone(),
        cli: env!("CARGO_PKG_VERSION").into(),
    };
    let written = serde_json::to_string_pretty(&project)
        .map_err(|e| e.to_string())
        .and_then(|j| write(&path, &(j + "\n")));
    if let Err(e) = written {
        eprintln!("exodus login: {e}");
        return ExitCode::FAILURE;
    }
    println!("Linked to {}. Run `exodus status`.", l.label());
    ExitCode::SUCCESS
}

impl Linked {
    fn label(&self) -> String {
        match &self.owner {
            Some(o) => format!("{o}/{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Exchanges `code` at `api` (else the default) for a key and saves it; on failure prints why and returns the exit code:
/// 4 for an expired or used code, 3 another refusal, 2 no answer.
fn exchange(root: &Path, code: &str, api: Option<&str>) -> Result<Linked, ExitCode> {
    let fail = |m: String, code: u8| {
        eprintln!("exodus: {m}");
        ExitCode::from(code)
    };
    let api = api.unwrap_or(platform::DEFAULT_API).trim_end_matches('/').to_string();
    if !(api.starts_with("https://") || api.starts_with("http://127.0.0.1") || api.starts_with("http://localhost")) {
        return Err(fail(
            format!("--api {api}: an https:// address (http:// only on this machine)"),
            1,
        ));
    }
    let (owner, repo) = match super::origin(root) {
        Some((owner, name)) => (Some(owner), name),
        None => (None, folder_name(root).map_err(|e| fail(e, 1))?),
    };
    let body = json!({
        "code": code.trim(),
        "device": hostname(),
        "os": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "cli": env!("CARGO_PKG_VERSION"),
        "owner": owner,
        "repo": repo,
    });
    let v = match platform::exchange(&api, &body) {
        Ok(v) => v,
        // S2: an agent that met a spent code went on offline; this sentence says what to do instead, whatever the
        // platform's own words are
        Err(Failure::Refused(status @ (409 | 410), _)) => {
            let why = if status == 409 {
                "was used already (409)"
            } else {
                "ran out (410; a code lasts 15 minutes)"
            };
            return Err(fail(
                format!(
                    "this code {why}, so nothing was linked. Copy a fresh line from the cabinet, {api}, \
                     and run it in this folder. An agent running this asks the person for that line rather than \
                     going on offline."
                ),
                4,
            ));
        }
        Err(f @ Failure::Refused(..)) => return Err(fail(f.message(), 3)),
        Err(f) => return Err(fail(f.message(), 2)),
    };
    let s = |v: &serde_json::Value| v.as_str().map(str::to_string);
    let p = &v["project"];
    let (Some(key), Some(id), Some(kind)) = (s(&v["key"]), s(&p["id"]), s(&p["kind"])) else {
        return Err(fail(
            format!("POST {api}/api/cli/exchange: an answer without a key and a project"),
            2,
        ));
    };
    let linked = Linked {
        id,
        owner: s(&p["owner"]),
        name: s(&p["name"]).unwrap_or(repo),
        kind,
        // the address that answered, which is the one this machine can reach
        api,
    };
    let entry = platform::Entry {
        id: linked.id.clone(),
        key,
        api: Some(linked.api.clone()),
        owner: linked.owner.clone(),
        name: linked.name.clone(),
        device: hostname(),
    };
    match platform::save(&entry) {
        Ok(_) => Ok(linked),
        Err(e) => Err(fail(
            format!("the key was made but not kept ({e}); run the line again"),
            1,
        )),
    }
}

/// What `exodus init` writes, and what a Mac's Finder or Claude Code may leave in a folder first: a folder holding
/// nothing else is empty enough for a new product.
const LAID_OUT: &[&str] = &[".DS_Store", ".exodus", ".gitignore", ".claude", ".cursor", "AGENTS.md"];

/// Where a one-time code may be spent (SPEC F1): the top folder of a git repository other than the home folder, or with
/// `--new` a folder that holds nothing yet. Else why not and `again`, the same command, for the right folder: the code
/// is still unused, so it works there.
fn linkable(here: &Path, home: Option<&Path>, new: bool, again: &str) -> Result<(), String> {
    let at_home = Some(here) == home;
    if !at_home && here.join(".git").exists() {
        return Ok(());
    }
    let empty = !at_home
        && std::fs::read_dir(here)
            .map_err(|e| format!("this folder: {e}"))?
            .filter_map(Result::ok)
            .all(|e| LAID_OUT.contains(&&*e.file_name().to_string_lossy()));
    if new && empty {
        return Ok(());
    }
    // a repository in the home folder holds dotfiles, not this project
    let top = here
        .ancestors()
        .skip(1)
        .find(|t| t.join(".git").exists() && Some(*t) != home);
    let what = match top {
        _ if at_home => "is your home folder, not a repository".to_string(),
        Some(t) => format!("is a folder inside the repository {}", shell_path(t, home)),
        None if new => "is not empty and not a repository".into(),
        None if empty => "is empty and not a repository".into(),
        None => "is not a git repository".into(),
    };
    let repo = format!("run it in the repository's folder:\n  cd <the repository> && {again}");
    let run = match top {
        Some(t) if !at_home => format!(
            "Run it in the repository's top folder:\n  cd {} && {again}\nor add --here to link this folder as a \
             project of its own.",
            shell_path(t, home)
        ),
        _ if new => {
            format!("A new product starts in an empty folder:\n  mkdir <a new folder> && cd <a new folder> && {again}")
        }
        _ if empty => format!("For a new product, run here:\n  {again} --new\nTo move an app, {repo}"),
        _ if at_home => format!("To move an app, {repo}"),
        _ => format!("To move an app, {repo}\nor add --here to link this folder anyway."),
    };
    Err(format!(
        "{} {what}, so the code was not used. {run}",
        shell_path(here, home)
    ))
}

/// How the person runs this binary: `exodus` when that name on PATH is this file, else its path, such as
/// `~/.local/bin/exodus` when the installer's folder is not on PATH (S2: the agent found it only there).
fn exe(home: Option<&Path>) -> String {
    let Some(me) = std::env::current_exe().ok().and_then(|p| std::fs::canonicalize(p).ok()) else {
        return "exodus".into();
    };
    let on_path = std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|d| d.join("exodus"))
                .find(|p| p.is_file())
        })
        .and_then(|p| std::fs::canonicalize(p).ok());
    if on_path.as_ref() == Some(&me) {
        "exodus".into()
    } else {
        shell_path(&me, home)
    }
}

/// `path` as one word of a shell command line, under the home folder as `~/...`.
fn shell_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", quote(&rest.to_string_lossy())),
        None => quote(&path.to_string_lossy()),
    }
}

/// `s` as one shell word: as it is when plain, else in single quotes.
fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// This machine's name, as Settings lists the key.
fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn init(root: &Path, args: &Args, linked: Option<&Linked>) -> Result<Vec<String>, String> {
    let dir = root.join(".exodus");
    let project_path = dir.join("project.json");
    let before: Option<ProjectFile> = super::report::read(&project_path)?;
    // the kind is the code's when linked; else a folder planned as new stays new (A17: the kind belongs to the project)
    let kind = match linked {
        Some(l) => l.kind.as_str(),
        None if args.new || before.as_ref().is_some_and(|p| p.kind == "new") => "new",
        None => "move",
    };
    let (owner, name) = match super::origin(root) {
        Some((owner, name)) => (Some(owner), name),
        None => (None, folder_name(root)?),
    };
    let project = ProjectFile {
        project: linked
            .map(|l| l.id.clone())
            .or_else(|| before.as_ref().and_then(|p| p.project.clone())),
        api: linked.map(|l| l.api.clone()).or_else(|| before.and_then(|p| p.api)),
        owner,
        name,
        kind: kind.into(),
        cli: env!("CARGO_PKG_VERSION").into(),
    };
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())? + "\n";
    write(&project_path, &json)?;
    write(&dir.join("README.md"), README)?;
    let mut wrote = vec![".exodus/project.json".to_string()];
    if ignore(&root.join(".gitignore"))? {
        wrote.push(".gitignore (.exodus/)".into());
    }

    let body = if kind == "new" { NEW } else { MOVE };
    for agent in [Agent::Claude, Agent::Codex, Agent::Cursor] {
        if !args.agent.contains(&agent) {
            continue;
        }
        match agent {
            Agent::Claude => {
                let text = SKILL.replace("{{body}}", &fill(body, "Claude Code"));
                write(&root.join(".claude/skills/exodus/SKILL.md"), &text)?;
                wrote.push(".claude/skills/exodus/SKILL.md".into());
            }
            Agent::Codex => {
                // one level down: the section sits under the file's own title
                let section = AGENTS.replace("{{body}}", &fill(body, "Codex").replace("\n## ", "\n### "));
                let path = root.join("AGENTS.md");
                let old = std::fs::read_to_string(&path).unwrap_or_default();
                write(&path, &splice(&old, &section))?;
                wrote.push("AGENTS.md (the Exodus section)".into());
            }
            Agent::Cursor => {
                let text = CURSOR.replace("{{body}}", &fill(body, "Cursor"));
                write(&root.join(".cursor/rules/exodus.mdc"), &text)?;
                wrote.push(".cursor/rules/exodus.mdc".into());
            }
        }
    }
    Ok(wrote)
}

/// The body with the agent's own name in `exodus send --agent`.
fn fill(body: &str, agent: &str) -> String {
    body.trim_end().replace("{{agent}}", agent)
}

/// `old` with the Exodus section replaced, or appended after a blank line when it has none.
fn splice(old: &str, section: &str) -> String {
    if let (Some(a), Some(b)) = (old.find(START), old.find(END))
        && a < b
    {
        let rest = old[b + END.len()..].strip_prefix('\n').unwrap_or(&old[b + END.len()..]);
        return format!("{}{section}{rest}", &old[..a]);
    }
    let old = old.trim_end();
    if old.is_empty() {
        section.to_string()
    } else {
        format!("{old}\n\n{section}")
    }
}

/// Adds `.exodus/` to `.gitignore` unless a line already ignores it; true when it wrote.
fn ignore(path: &Path) -> Result<bool, String> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let ignored = old
        .lines()
        .map(str::trim)
        .any(|l| matches!(l, ".exodus" | ".exodus/" | "/.exodus" | "/.exodus/"));
    if ignored {
        return Ok(false);
    }
    let sep = if old.is_empty() || old.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    write(path, &format!("{old}{sep}.exodus/\n"))?;
    Ok(true)
}

fn folder_name(root: &Path) -> Result<String, String> {
    let abs = std::fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    abs.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "cannot name this folder; run it inside the repository".into())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_appends_once_and_replaces_in_place() {
        let s = AGENTS.replace("{{body}}", "one");
        let first = splice("# Notes\n\nkeep me\n", &s);
        assert!(first.starts_with("# Notes\n\nkeep me\n\n<!-- exodus:start"));
        let t = AGENTS.replace("{{body}}", "two");
        let second = splice(&format!("{first}\n## After\n"), &t);
        assert_eq!(second.matches(START).count(), 1);
        assert!(second.contains("two") && !second.contains("one"));
        assert!(second.ends_with("\n## After\n") && second.starts_with("# Notes\n\nkeep me\n\n"));
        assert_eq!(splice("", &s), s);
    }

    /// SPEC F1: a code is spent only at the top of a repository or, for a new product, in an empty folder; anywhere else
    /// the sentence names the place and gives the command for the right one.
    #[test]
    fn a_code_is_spent_only_where_it_belongs() {
        #![allow(clippy::unwrap_used)]
        let base = std::env::temp_dir().join(format!("exodus-test-linkable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in [
            "code/shop/.git",
            "code/shop/web",
            "idea/.claude",
            "Downloads",
            "My Projects/app/.git",
            "My Projects/app/web",
        ] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        std::fs::write(base.join("idea/.DS_Store"), "").unwrap();
        std::fs::write(base.join("Downloads/invoice.pdf"), "").unwrap();
        let home = std::fs::canonicalize(&base).unwrap();
        let again = "exodus init --code K7QM-4XPT";
        let check = |p: &str, new| linkable(&home.join(p), Some(&home), new, again);
        let refused = |p: &str, new| check(p, new).unwrap_err();

        assert!(check("code/shop", false).is_ok() && check("code/shop", true).is_ok());
        assert!(check("idea", true).is_ok(), "only what Finder and Claude Code leave");
        let e = refused("", false);
        assert!(
            e.starts_with("~ is your home folder, not a repository, so the code was not used."),
            "{e}"
        );
        assert!(
            e.contains("\n  cd <the repository> && exodus init --code K7QM-4XPT"),
            "{e}"
        );
        let e = refused("code/shop/web", false);
        assert!(
            e.contains("inside the repository ~/code/shop") && e.contains("\n  cd ~/code/shop && exodus init"),
            "{e}"
        );
        let e = refused("idea", false);
        assert!(
            e.contains("is empty") && e.contains("\n  exodus init --code K7QM-4XPT --new\n"),
            "{e}"
        );
        let e = refused("Downloads", false);
        assert!(
            e.contains("~/Downloads is not a git repository") && e.contains("--here"),
            "{e}"
        );
        assert!(refused("Downloads", true).contains("mkdir <a new folder>"));

        // a repository of dotfiles in the home folder is not this project's
        std::fs::create_dir_all(base.join(".git")).unwrap();
        assert!(refused("", false).contains("home folder"));
        assert!(refused("Downloads", false).contains("not a git repository"));
        let e = refused("My Projects/app/web", false);
        assert!(
            e.contains("cd ~/'My Projects/app' && "),
            "a path with a space is quoted: {e}"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// The findings example of the new-product instructions is a real findings file: it parses and builds a plan.
    #[test]
    fn new_product_example_builds() {
        let json = NEW
            .split("```json\n")
            .nth(1)
            .and_then(|s| s.split("\n```").next())
            .unwrap_or_default();
        let findings: exodus_schema::Findings =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("the example in new.md: {e}"));
        assert_eq!(findings.kind, "new");
        let input = exodus_report::Input {
            findings: &findings,
            answers: &[],
            prev: None,
            at: "2026-10-11T00:00:00Z",
            agent: None,
        };
        let report = exodus_report::build(&input, &exodus_cost::bundled(), &exodus_cost::platforms())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(report.cost.cloud_run > 0);
    }
}
