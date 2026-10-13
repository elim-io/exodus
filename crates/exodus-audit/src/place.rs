//! Where a file's code runs. A rule counts only in code that runs on the server; a hit in browser code, build and
//! tool code, a mobile or desktop app, tests or docs is named in `clean` as "not on the server" instead (SPEC F3).
//! The verdict is by path first (folder and file names), then by the nearest package.json (a UI package with no
//! server library is browser code; a Tauri or Electron package is a desktop app), then by the code itself (DOM
//! APIs and no Node module).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

/// The `clean` entries of places that do not run on the server start with this.
pub const NOT_ON_SERVER: &str = "not on the server: ";

pub const BROWSER: &str = "browser code";
pub const TOOLS: &str = "build and tool code";
pub const MOBILE: &str = "mobile app";
pub const DESKTOP: &str = "desktop app";
pub const TESTS: &str = "tests";
pub const DOCS: &str = "docs and examples";

fn regex<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Regex, D::Error> {
    Regex::new(&String::deserialize(d)?).map_err(serde::de::Error::custom)
}

/// The places data, `rules/places.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Places {
    front: Vec<String>,
    folder: Vec<Folder>,
    file: Vec<Named>,
    package: Packages,
    code: Code,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Folder {
    why: String,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    suffixes: Vec<String>,
    #[serde(default)]
    front_only: bool,
    #[serde(default)]
    except_ext: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    why: String,
    #[serde(deserialize_with = "regex")]
    name: Regex,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Packages {
    desktop: Vec<String>,
    ui: Vec<String>,
    server: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Code {
    #[serde(deserialize_with = "regex")]
    dom: Regex,
    #[serde(deserialize_with = "regex")]
    node: Regex,
}

static PLACES: LazyLock<Places> = LazyLock::new(|| {
    #[allow(clippy::expect_used)] // compiled-in data; a test parses it
    toml::from_str(include_str!("../rules/places.toml")).expect("rules/places.toml")
});

/// The label a `why` in the data stands for, as the `&'static str` the rest of the audit carries.
fn label(why: &str) -> &'static str {
    [BROWSER, TOOLS, MOBILE, DESKTOP, TESTS, DOCS]
        .into_iter()
        .find(|l| *l == why)
        .unwrap_or(TOOLS)
}

/// What a package.json says about the code under it.
#[derive(Clone, Default)]
struct Package {
    desktop: bool,
    ui: bool,
    server: bool,
}

/// Places files against one set of files. A package.json is read once, not for every file under it.
pub struct Placer<'f> {
    files: &'f BTreeMap<String, String>,
    /// By folder: the nearest package.json's folder (`""` at the root, else ending in `/`) and what it says.
    packages: RefCell<HashMap<String, Option<(String, Package)>>>,
}

impl<'f> Placer<'f> {
    pub fn new(files: &'f BTreeMap<String, String>) -> Self {
        Placer {
            files,
            packages: RefCell::default(),
        }
    }

    /// Where the code of `path` runs when not on the server: the place to name (a folder ending in `/`, or the
    /// file) and what it is. `None` is server code, or code nothing here can place.
    pub fn off_server(&self, path: &str) -> Option<(String, &'static str)> {
        let places = &*PLACES;
        let files = self.files;
        let segs: Vec<&str> = path.split('/').collect();
        let (name, dirs) = segs.split_last()?;
        let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
        let front = places.front.iter().any(|f| f == ext);
        let folder = |i: usize| format!("{}/", segs[..=i].join("/"));

        for i in 0..dirs.len() {
            // folder names are compared in lower case: Laravel's is Console/Commands
            let one = dirs[i].to_ascii_lowercase();
            let two = (i > 0).then(|| format!("{}/{}", dirs[i - 1], dirs[i]).to_ascii_lowercase());
            for f in &places.folder {
                let named = f.names.iter().any(|n| *n == one || two.as_deref() == Some(n));
                let marked = f.prefixes.iter().any(|p| dirs[i].starts_with(p.as_str()))
                    || f.suffixes.iter().any(|s| dirs[i].ends_with(s.as_str()));
                if (named || marked) && (front || !f.front_only) && !f.except_ext.iter().any(|e| e == ext) {
                    // a site folder with its own Dockerfile is the web process: name the folder under it, not it
                    if f.front_only && files.contains_key(&format!("{}Dockerfile", folder(i))) {
                        if i + 1 < dirs.len() {
                            return Some((folder(i + 1), label(&f.why)));
                        }
                        continue;
                    }
                    return Some((folder(i), label(&f.why)));
                }
            }
        }

        if let Some(f) = places.file.iter().find(|f| f.name.is_match(name)) {
            return Some((path.to_string(), label(&f.why)));
        }
        if matches!(ext, "kt" | "java")
            && let Some((module, _)) = path.split_once("src/main/")
            && files.contains_key(&format!("{module}src/main/AndroidManifest.xml"))
        {
            return Some((format!("{module}src/main/"), MOBILE));
        }
        // A Tauri app is the folder that holds src-tauri/, with everything in it; a web app that also runs inside
        // one only depends on its library.
        if let Some(i) =
            (0..dirs.len()).find(|&i| files.contains_key(&format!("{}src-tauri/tauri.conf.json", folder(i))))
        {
            return Some((folder(i), DESKTOP));
        }
        if let Some((dir, pkg)) = self.package(path) {
            if pkg.desktop {
                return Some((dir, DESKTOP));
            }
            if front && pkg.ui && !pkg.server && !dir.is_empty() {
                return Some((dir, BROWSER));
            }
        }
        let text = files.get(path)?;
        (front && places.code.dom.is_match(text) && !places.code.node.is_match(text))
            .then(|| (path.to_string(), BROWSER))
    }

    /// The nearest package.json above `path`.
    fn package(&self, path: &str) -> Option<(String, Package)> {
        let from = path.rsplit_once('/').map_or("", |(d, _)| d);
        if let Some(known) = self.packages.borrow().get(from) {
            return known.clone();
        }
        let found = self.read_package(from);
        self.packages.borrow_mut().insert(from.to_string(), found.clone());
        found
    }

    fn read_package(&self, mut dir: &str) -> Option<(String, Package)> {
        loop {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            if let Some(text) = self.files.get(&format!("{prefix}package.json")) {
                let v: serde_json::Value = serde_json::from_str(text).ok()?;
                let deps: Vec<&String> = ["dependencies", "peerDependencies"]
                    .iter()
                    .filter_map(|k| v[k].as_object())
                    .flat_map(|o| o.keys())
                    .collect();
                let has = |list: &[String]| deps.iter().any(|d| list.contains(d));
                let p = &PLACES.package;
                return Some((
                    prefix,
                    Package {
                        desktop: has(&p.desktop),
                        ui: has(&p.ui),
                        server: has(&p.server),
                    },
                ));
            }
            if dir.is_empty() {
                return None;
            }
            dir = dir.rsplit_once('/').map_or("", |(d, _)| d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn off(path: &str, files: &[(&str, &str)]) -> Option<(String, &'static str)> {
        let mut map: BTreeMap<String, String> = files.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect();
        map.entry(path.into()).or_default();
        Placer::new(&map).off_server(path)
    }

    #[test]
    fn the_data_parses_and_every_why_is_a_label() {
        let all = [BROWSER, TOOLS, MOBILE, DESKTOP, TESTS, DOCS];
        let places = &*PLACES;
        for why in places
            .folder
            .iter()
            .map(|f| &f.why)
            .chain(places.file.iter().map(|f| &f.why))
        {
            assert!(all.contains(&why.as_str()), "{why}");
        }
    }

    #[test]
    fn file_names_say_what_a_file_is() {
        let at = |p: &str, w: &'static str| Some((p.to_string(), w));
        assert_eq!(off("pkg/db_test.go", &[]), at("pkg/db_test.go", TESTS));
        assert_eq!(off("app/test_users.py", &[]), at("app/test_users.py", TESTS));
        assert_eq!(off("web/gulpfile.js", &[]), at("web/gulpfile.js", TOOLS));
        assert_eq!(off("web/vite.config.ts", &[]), at("web/vite.config.ts", TOOLS));
        assert_eq!(off("app/gen_models.py", &[]), at("app/gen_models.py", TOOLS));
        assert_eq!(off("app/models.py", &[]), None);
        assert_eq!(off("app/generator.py", &[]), None);
    }

    #[test]
    fn places_code_that_does_not_run_on_the_server() {
        let at = |p: &str, w: &'static str| Some((p.to_string(), w));
        assert_eq!(off("website/start/carousel.js", &[]), at("website/", BROWSER));
        // with its own Dockerfile the site folder is the web process; the browser files are the folder under it
        let site = [("website/Dockerfile", "FROM caddy:2\n")];
        assert_eq!(off("website/start/carousel.js", &site), at("website/start/", BROWSER));
        assert_eq!(off("website/build.py", &[]), at("website/build.py", TOOLS));
        assert_eq!(off("tools/catalog.py", &[]), at("tools/", TOOLS));
        assert_eq!(off("ios/App/View.swift", &[]), at("ios/", MOBILE));
        assert_eq!(off("lib/mix/tasks/x.ex", &[]), at("lib/mix/tasks/", TOOLS));
        assert_eq!(
            off("hc/front/management/commands/x.py", &[]),
            at("hc/front/management/commands/", TOOLS)
        );
        assert_eq!(off("dev/gen/gen.py", &[]), at("dev/", TOOLS));
        assert_eq!(
            off("app/src/main/java/A.kt", &[("app/src/main/AndroidManifest.xml", "")]),
            at("app/src/main/", MOBILE)
        );
        // A UI package with no server library is browser code; one with a server library is not placed.
        let ui = r#"{"dependencies":{"vue":"3"}}"#;
        assert_eq!(
            off("packages/ui/src/poll.ts", &[("packages/ui/package.json", ui)]),
            at("packages/ui/", BROWSER)
        );
        let ssr = r#"{"dependencies":{"vue":"3","express":"4"}}"#;
        assert_eq!(
            off("packages/ui/src/poll.ts", &[("packages/ui/package.json", ssr)]),
            None
        );
        let tauri = r#"{"dependencies":{"@tauri-apps/api":"2"}}"#;
        let app = [("desk/package.json", tauri), ("desk/src-tauri/tauri.conf.json", "{}")];
        assert_eq!(off("desk/crates/b/src/main.rs", &app), at("desk/", DESKTOP));
        // A web app that also runs in Tauri keeps its server code on the server.
        assert_eq!(off("web/server/keys.go", &[("web/package.json", tauri)]), None);
        // DOM APIs without a Node module make a file browser code.
        assert_eq!(
            off("src/x.js", &[("src/x.js", "window.setInterval(f, 1000)")]),
            at("src/x.js", BROWSER)
        );
        assert_eq!(
            off("src/x.js", &[("src/x.js", "import fs from 'fs';\nwindow.x = 1")]),
            None
        );
        // Server code stays, Rust's src/bin too.
        assert_eq!(off("src/server.js", &[("src/server.js", "setInterval(f, 1000)")]), None);
        assert_eq!(off("src/bin/server.rs", &[]), None);
        assert_eq!(off("app/services/static_pages.py", &[]), None);
    }
}
