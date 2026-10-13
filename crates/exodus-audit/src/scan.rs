//! Walks the repository (gitignore-aware, vendored and built folders skipped) and reads its text files.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Folders that hold dependencies or build output, never the app's own code.
const SKIP: &[&str] = &[
    ".git",
    ".exodus",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".venv",
    "venv",
    "__pycache__",
    ".tox",
    "coverage",
    "tmp",
    "deps",
    "_build",
];

/// Bigger files are bundles or data, not code anyone wrote.
const MAX_BYTES: u64 = 1 << 20;

/// Files that hold values (`.env`, `prod.env`, `.envrc`, `.dev.vars`, keys, Terraform variables) are never opened.
/// The examples are read for names only.
pub fn is_env_file(name: &str) -> bool {
    if is_env_sample(name) {
        return false;
    }
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    name == ".env"
        || name.starts_with(".env.")
        || name == ".envrc"
        || name == ".dev.vars"
        || matches!(ext, "env" | "tfvars" | "pem" | "key" | "p12" | "pfx")
}

pub fn is_env_sample(name: &str) -> bool {
    matches!(
        name,
        ".env.example" | ".env.sample" | ".env.template" | ".env.dist" | "example.env"
    )
}

/// Every text file under `root`, by its path relative to `root` with `/` separators.
pub fn read(root: &Path) -> io::Result<BTreeMap<String, String>> {
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a folder", root.display()),
        ));
    }
    let walk = ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .git_global(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            let dir = e.file_type().is_some_and(|t| t.is_dir());
            !(is_env_file(&name) || dir && SKIP.contains(&name.as_ref()))
        })
        .build();
    let mut files = BTreeMap::new();
    for entry in walk.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) || entry.metadata().map_or(true, |m| m.len() > MAX_BYTES) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        // Binary or not UTF-8: not something the readers understand.
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if text.contains('\0') {
            continue;
        }
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        files.insert(rel, text);
    }
    Ok(files)
}
