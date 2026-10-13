//! `exodus key --ci [--name N]` (architecture A15): a second key of this project for CI, so `exodus check` can tell the
//! cabinet about its runs. The key goes to stdout only when stdout is a pipe (`exodus key --ci | gh secret set
//! EXODUS_KEY`); a terminal would show it, so a bare run refuses before minting anything.

use std::io::IsTerminal as _;
use std::path::Path;
use std::process::ExitCode;

use serde_json::json;

#[derive(clap::Args)]
pub struct Args {
    /// A key for CI (the only kind this command makes)
    #[arg(long, required = true)]
    ci: bool,
    /// The name Settings shows for it (default "ci")
    #[arg(long, value_name = "N", default_value = "ci")]
    name: String,
}

pub fn run(args: Args) -> ExitCode {
    if std::io::stdout().is_terminal() {
        eprintln!("exodus key: the key is written only into a pipe: `exodus key --ci | gh secret set EXODUS_KEY`");
        return ExitCode::FAILURE;
    }
    let Some(link) = crate::platform::link(Path::new(".")) else {
        eprintln!("exodus key: minting a key needs a linked account; `exodus init --code` links one.");
        return ExitCode::from(2);
    };
    let _ = args.ci;
    let minted = match link.post("cli/keys", &json!({ "device": args.name.trim(), "os": "ci" })) {
        Ok(v) => v,
        Err(f) => return super::failed(&f),
    };
    let Some(key) = minted["key"].as_str() else {
        eprintln!("exodus key: the platform answered without a key");
        return ExitCode::from(2);
    };
    println!("{key}");
    eprintln!(
        "exodus key: a key for {} is made; Settings lists it and revokes it apart from this machine's.",
        args.name.trim()
    );
    ExitCode::SUCCESS
}
