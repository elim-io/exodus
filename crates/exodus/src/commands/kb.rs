//! `exodus kb` and `exodus ask` (architecture §3): the platform's knowledge base, an article by slug or a topic's list,
//! and the best passages for a question with their links. Both need a linked account; without one they say where the
//! articles are public and exit 2.

use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

use crate::platform::{self, Failure, Link};

/// Printed by both commands without a key.
pub const NO_KEY: &str = "The knowledge base needs a linked account; the articles are public at https://exodus.elim.io/blog.\n\
                          `exodus init --code` links one.";

#[derive(clap::Args)]
pub struct KbArgs {
    /// An article's slug, or a topic id (the list of that topic)
    pub what: Option<String>,
    /// List the topics and every article
    #[arg(long)]
    pub list: bool,
    /// List one topic's articles
    #[arg(long, value_name = "T")]
    pub topic: Option<String>,
}

#[derive(clap::Args)]
pub struct AskArgs {
    /// The question, in your words
    pub question: String,
    /// How many passages
    #[arg(short = 'n', default_value_t = 3)]
    pub n: usize,
}

fn linked() -> Result<Link, ExitCode> {
    platform::link(Path::new(".")).ok_or_else(|| {
        eprintln!("{NO_KEY}");
        ExitCode::from(2)
    })
}

/// The topic ids the platform knows (`GET /api/kb`'s `topics`).
const TOPICS: &[&str] = &[
    "cloud-run-basics",
    "pricing",
    "patterns-and-their-replacements",
    "from-another-platform",
    "cutover-and-data",
];

pub fn kb(args: KbArgs) -> ExitCode {
    let link = match linked() {
        Ok(l) => l,
        Err(code) => return code,
    };
    let topic = args
        .topic
        .clone()
        .or_else(|| args.what.clone().filter(|w| TOPICS.contains(&w.as_str())));
    match (&args.what, topic) {
        (_, Some(t)) => list(&link, Some(&t), false),
        (Some(slug), None) if !args.list => article(&link, slug),
        _ => list(&link, None, true),
    }
}

fn list(link: &Link, topic: Option<&str>, topics: bool) -> ExitCode {
    let query: Vec<(&str, &str)> = topic.map(|t| ("topic", t)).into_iter().collect();
    let v = match link.get("kb", &query) {
        Ok(v) => v,
        Err(f) => return super::failed(&f),
    };
    if topics {
        println!("Topics:");
        for t in v["topics"].as_array().into_iter().flatten() {
            println!("  {:<34} {} ({})", s(&t["id"]), s(&t["name"]), t["count"]);
        }
        println!();
    }
    let articles = v["articles"].as_array().cloned().unwrap_or_default();
    if articles.is_empty() {
        println!("No articles here yet.");
    }
    for a in &articles {
        println!(
            "{:<48} {} ({} min, checked {})",
            s(&a["slug"]),
            s(&a["title"]),
            a["minutes"],
            s(&a["checked"])
        );
    }
    println!("\n`exodus kb <slug>` prints one; `exodus ask \"<question>\"` searches them.");
    ExitCode::SUCCESS
}

fn article(link: &Link, slug: &str) -> ExitCode {
    let cache = Path::new(".exodus/cache/kb").join(format!("{}.md", safe(slug)));
    let v = match link.get(&format!("kb/{}", safe(slug)), &[]) {
        Ok(v) => v,
        Err(Failure::Refused(404, _)) => {
            eprintln!("exodus: no article {slug}; `exodus kb --list` lists them.");
            return ExitCode::from(3);
        }
        Err(Failure::Unreachable(m)) if cache.exists() => {
            eprintln!("exodus: {m}; printing the copy kept in {}", cache.display());
            return match std::fs::read_to_string(&cache) {
                Ok(text) => {
                    print!("{text}");
                    ExitCode::SUCCESS
                }
                Err(_) => ExitCode::from(2),
            };
        }
        Err(f) => return super::failed(&f),
    };
    let text = format!(
        "# {}\n\n{}\n\nChecked {}, {} min. Public at https://exodus.elim.io/blog/{}\n\n{}\n",
        s(&v["title"]),
        s(&v["summary"]),
        s(&v["checked"]),
        v["minutes"],
        s(&v["slug"]),
        s(&v["body"]).trim_end()
    );
    print!("{text}");
    // the copy for the agent to read again offline, when the repository has .exodus/
    if Path::new(".exodus").is_dir() {
        let written = std::fs::create_dir_all(cache.parent().unwrap_or(Path::new(".")))
            .and_then(|_| std::fs::write(&cache, &text));
        if let Err(e) = written {
            eprintln!("exodus: {} not written: {e}", cache.display());
        }
    }
    ExitCode::SUCCESS
}

pub fn ask(args: AskArgs) -> ExitCode {
    let question = args.question.trim();
    if question.is_empty() {
        eprintln!("exodus ask: the question is empty");
        return ExitCode::FAILURE;
    }
    let link = match linked() {
        Ok(l) => l,
        Err(code) => return code,
    };
    let n = args.n.clamp(1, 10).to_string();
    let v = match link.get("kb/search", &[("q", question), ("n", &n)]) {
        Ok(v) => v,
        Err(f) => return super::failed(&f),
    };
    let hits = v["hits"].as_array().cloned().unwrap_or_default();
    if hits.is_empty() {
        println!("Nothing in the knowledge base matches; try other words, or `exodus kb --list`.");
    }
    for (i, h) in hits.iter().enumerate() {
        let heading = h["heading"].as_str().map(|x| format!(" - {x}")).unwrap_or_default();
        println!(
            "{}. {}{heading}\n   {}\n   {}\n",
            i + 1,
            s(&h["title"]),
            s(&h["passage"]),
            s(&h["url"])
        );
    }
    ExitCode::SUCCESS
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// A slug as a path segment and a file name: letters, digits and dashes only.
fn safe(slug: &str) -> String {
    slug.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
