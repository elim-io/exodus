//! The shapes of a project its root manifests do not show (SPEC F4): Firebase (Hosting and Functions 2nd gen are
//! on Google already and stay), a Vercel or Netlify project, GitHub Pages, a static site (Caddy, nginx, `serve`, a
//! Dockerfile on a file server's image), the deployable folders of a monorepo (each with its own Dockerfile) and a
//! mobile app beside a backend (not moved).

use std::collections::BTreeMap;

use exodus_schema::Process;
use serde_json::Value;

use super::{docker, platform, process};
use crate::{Facts, at, place};

/// The `clean` entries of parts that stay where they are start with this.
pub const STAYS: &str = "stays as it is: ";

/// How deep a deployable folder or a platform file may sit: `packages/api/Dockerfile` is two folders down.
const DEPTH: usize = 3;

/// Firebase, the hosts and the mobile apps; returns the projects of their own in folders (a `vercel.json` there),
/// which the caller adds after the root's processes.
pub fn read(f: &mut Facts, files: &BTreeMap<String, String>) -> Vec<Process> {
    firebase(f, files);
    mobile(f, files);
    hosts(f, files)
}

/// A web process that serves files.
pub fn static_site(id: &str, command: &str, server: &str, from: String) -> Process {
    let mut p = process(id, "web", "static site", command, from);
    p.framework = Some(server.into());
    p
}

/// `serve -s build`, `npx http-server public`: a start script that only serves files.
pub fn serve_script(script: &str) -> Option<&'static str> {
    let first = script
        .split_whitespace()
        .find(|w| !matches!(*w, "npx" | "pnpm" | "yarn" | "bunx" | "dlx" | "exec"))?;
    match first {
        "serve" => Some("serve"),
        "http-server" => Some("http-server"),
        _ => None,
    }
}

/// A Caddyfile or nginx config at the root: one that hands out files is a static site; nginx in front of PHP-FPM is
/// the PHP app's web process.
pub fn server_config(files: &BTreeMap<String, String>) -> Option<Process> {
    for (name, text) in files.iter().filter(|(n, _)| !n.contains('/')) {
        let words = |w: &str| text.lines().any(|l| l.trim_start().starts_with(w));
        if name == "Caddyfile" && (words("file_server") || words("root ")) {
            return Some(static_site("web", "caddy run", "Caddy", at(name, 1)));
        }
        if name.starts_with("nginx") && name.contains(".conf") {
            if text.contains("fastcgi_pass") {
                return Some(process("web", "web", "web", "", at(name, 1)));
            }
            if words("root ") && !text.contains("proxy_pass") {
                return Some(static_site("web", "nginx", "nginx", at(name, 1)));
            }
        }
    }
    None
}

/// Folders at most `DEPTH` down, each holding a file named `name`, that run on a server (not tests, docs, tools or
/// an app): `("packages/api/", "packages/api/Dockerfile")`.
fn deployable<'a>(files: &'a BTreeMap<String, String>, name: &str) -> Vec<(String, &'a String)> {
    let placer = place::Placer::new(files);
    files
        .keys()
        .filter(|p| p.rsplit('/').next() == Some(name) && p.matches('/').count() <= DEPTH)
        .filter(|p| !p.split('/').any(|s| s.starts_with('.')))
        .filter(|p| placer.off_server(p).is_none())
        .map(|p| (p[..p.len() - name.len()].to_string(), p))
        .collect()
}

/// Whether anything lies under `dir` (`""` is the root, else ending in `/`).
fn exists(files: &BTreeMap<String, String>, dir: &str) -> bool {
    dir.is_empty() || files.keys().any(|k| k.starts_with(dir))
}

/// A folder a platform file names, relative to the file's folder: it, or the folder it is built in, exists. A
/// `dist` that is not in git still counts when its parent is; a parent that is gone makes the file stale.
fn live(files: &BTreeMap<String, String>, base: &str, rel: &str) -> bool {
    let rel = rel.trim_start_matches("./").trim_matches('/');
    if rel.is_empty() || rel == "." {
        return true;
    }
    let path = format!("{base}{rel}/");
    let parent = path[..path.len() - 1]
        .rsplit_once('/')
        .map_or(String::new(), |(p, _)| format!("{p}/"));
    exists(files, &path) || exists(files, &parent)
}

/// `firebase.json` at the root or one folder down. Hosting makes the platform Firebase; Functions, Firestore, the
/// Realtime Database and Storage are on Google already and stay. A file whose folders are all gone is a leftover.
fn firebase(f: &mut Facts, files: &BTreeMap<String, String>) {
    let found: Vec<(String, &String)> = deployable(files, "firebase.json")
        .into_iter()
        .filter(|(dir, _)| dir.matches('/').count() <= 1)
        .collect();
    for (dir, file) in found {
        let Ok(v) = serde_json::from_str::<Value>(&files[file]) else {
            continue;
        };
        let list = |key: &str| -> Vec<Value> {
            match &v[key] {
                Value::Array(a) => a.clone(),
                o @ Value::Object(_) => vec![o.clone()],
                _ => Vec::new(),
            }
        };
        let hosting: Vec<String> = list("hosting")
            .iter()
            .filter_map(|h| h["source"].as_str().or(h["public"].as_str()).map(String::from))
            .filter(|p| live(files, &dir, p))
            .collect();
        let functions: Vec<String> = list("functions")
            .iter()
            .map(|x| {
                x["source"]
                    .as_str()
                    .unwrap_or("functions")
                    .trim_matches('/')
                    .to_string()
            })
            .filter(|src| {
                files
                    .get(&format!("{dir}{src}/package.json"))
                    .is_some_and(|t| t.contains("\"firebase-functions\""))
            })
            .collect();
        // Stores alone count when the code talks to Firebase (often from the browser).
        let sdk = || {
            files.iter().any(|(p, t)| {
                !p.ends_with(".json")
                    && (t.contains("from \"firebase/") || t.contains("from 'firebase/") || t.contains("firebase-admin"))
            })
        };
        if hosting.is_empty() && functions.is_empty() && !sdk() {
            continue;
        }
        f.manifest(file, "firebase");
        if !hosting.is_empty() {
            platform(f, 5, "Firebase", file, None);
            f.stays.push(format!(
                "Firebase Hosting ({file}, files and rewrites served by Google)"
            ));
            f.hosted_root |= dir.is_empty() && hosting.iter().any(|h| h == ".");
        }
        let mut firestore: Option<String> = None;
        for src in functions {
            let folder = format!("{dir}{src}/");
            let code = || {
                files
                    .iter()
                    .filter(|(p, _)| p.starts_with(&folder) && !p.ends_with(".json"))
            };
            let v2 = code().any(|(_, t)| t.contains("firebase-functions/v2"))
                || files
                    .get(&format!("{folder}package.json"))
                    .and_then(|t| serde_json::from_str::<Value>(t).ok())
                    .and_then(|p| p["dependencies"]["firebase-functions"].as_str().map(String::from))
                    .and_then(|s| crate::stack::version(&s, 1))
                    .and_then(|m| m.parse::<u32>().ok())
                    .is_some_and(|m| m >= 6);
            if firestore.is_none() {
                firestore = code()
                    .find(|(_, t)| t.contains("getFirestore(") || t.contains(".firestore()"))
                    .map(|(p, _)| p.clone());
            }
            f.stays.push(if v2 {
                format!("{folder} (Firebase Functions 2nd gen, already on Cloud Run functions)")
            } else {
                format!("{folder} (Firebase Functions 1st gen, on Google; 2nd gen runs them on Cloud Run functions)")
            });
        }
        for (key, what) in [
            ("firestore", "Firestore"),
            ("database", "Firebase Realtime Database"),
            ("storage", "Cloud Storage for Firebase"),
        ] {
            let from = if v.get(key).is_some() {
                Some(file.clone())
            } else if key == "firestore" {
                firestore.clone()
            } else {
                None
            };
            if let Some(from) = from {
                f.stays.push(format!("{what} ({from})"));
            }
        }
    }
}

/// `vercel.json` and `netlify.toml` at the root name the platform; one in a folder is a project of its own, a part
/// to move.
fn hosts(f: &mut Facts, files: &BTreeMap<String, String>) -> Vec<Process> {
    let mut projects = Vec::new();
    for (name, host, kind) in [
        ("vercel.json", "Vercel", "vercel"),
        ("netlify.toml", "Netlify", "netlify"),
    ] {
        for (dir, file) in deployable(files, name) {
            // Netlify publishes a folder; one that is gone makes the file a leftover.
            let publish = files[file]
                .parse::<toml::Table>()
                .ok()
                .and_then(|t| t.get("build")?.get("publish")?.as_str().map(String::from));
            if publish.is_some_and(|p| !live(files, &dir, &p)) {
                continue;
            }
            f.manifest(file, kind);
            if dir.is_empty() {
                platform(f, 6, host, file, None);
            } else {
                let next = files
                    .get(&format!("{dir}package.json"))
                    .is_some_and(|t| t.contains("\"next\""));
                let id = dir.trim_end_matches('/').rsplit('/').next().unwrap_or("site");
                let mut p = process(id, "web", id, "", at(file, 1));
                p.framework = Some(if next { "Next.js" } else { "static site" }.into());
                projects.push(p);
            }
        }
    }
    projects
}

/// Nothing else runs: on a host of files (Vercel, Netlify, or GitHub Pages when a workflow deploys there; a Pages
/// workflow beside a server only publishes its docs) the site is the one web process.
pub fn hosted(f: &mut Facts, files: &BTreeMap<String, String>) {
    let pages = files.iter().find(|(p, t)| {
        p.starts_with(".github/workflows/")
            && [
                "actions/deploy-pages@",
                "peaceiris/actions-gh-pages@",
                "github-pages-deploy-action@",
            ]
            .iter()
            .any(|a| t.contains(a))
    });
    if let Some((file, _)) = pages {
        platform(f, 7, "GitHub Pages", file, None);
    }

    let host = f
        .platforms
        .iter()
        .find(|p| matches!(p.1.name.as_str(), "Vercel" | "Netlify" | "GitHub Pages"))
        .map(|p| (p.1.name.clone(), p.1.file.clone()));
    if let Some((name, file)) = host {
        f.processes.push(static_site("web", "", &name, at(&file, 1)));
    }
}

/// No process yet: each folder with its own Dockerfile is a part (a monorepo, or a site in `website/`).
pub fn folders(f: &mut Facts, files: &BTreeMap<String, String>) {
    for (dir, file) in deployable(files, "Dockerfile") {
        let d = docker(&files[file]);
        let id = dir.trim_end_matches('/').rsplit('/').next().unwrap_or("web");
        let from = at(file, d.line);
        f.manifest(file, "dockerfile");
        f.processes.push(match d.server() {
            Some(server) => static_site(id, d.cmd.as_deref().unwrap_or(""), server, from),
            None => process(id, "web", id, d.cmd.as_deref().unwrap_or(""), from),
        });
    }
}

/// Mobile apps: an Xcode project, a Flutter app, an Android app module. The outermost folder of each is named.
fn mobile(f: &mut Facts, files: &BTreeMap<String, String>) {
    let placer = place::Placer::new(files);
    let mut apps: Vec<String> = Vec::new();
    for (p, t) in files {
        let app = if let Some(i) = p.find(".xcodeproj/") {
            let proj = &p[..i];
            match proj.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/"),
                None => format!("{proj}.xcodeproj/"),
            }
        } else if p.ends_with("pubspec.yaml") && t.contains("flutter:") {
            p[..p.len() - "pubspec.yaml".len()].to_string()
        } else if let Some(module) = p.strip_suffix("app/src/main/AndroidManifest.xml") {
            module.to_string()
        } else {
            continue;
        };
        if app.is_empty()
            || placer
                .off_server(&format!("{app}x"))
                .is_some_and(|(_, w)| w != place::MOBILE)
        {
            continue;
        }
        apps.push(app);
    }
    apps.sort();
    apps.dedup();
    let outer: Vec<String> = apps
        .iter()
        .filter(|a| !apps.iter().any(|b| b != *a && a.starts_with(b.as_str())))
        .cloned()
        .collect();
    f.apps = outer;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(files: &[(&str, &str)]) -> Facts {
        let map: BTreeMap<String, String> = files.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect();
        let mut f = Facts::default();
        super::super::read(&mut f, &map);
        f
    }

    #[test]
    fn firebase_parts_stay_and_hosting_names_the_platform() {
        let f = facts(&[
            (
                "firebase.json",
                r#"{"hosting":{"public":"public"},"functions":[{"source":"functions"}],"firestore":{}}"#,
            ),
            ("public/index.html", "<html>"),
            (
                "functions/package.json",
                r#"{"dependencies":{"firebase-functions":"^5.0.0"}}"#,
            ),
            (
                "functions/index.js",
                "const { onRequest } = require('firebase-functions/v2/https');",
            ),
        ]);
        assert_eq!(f.platforms[0].1.name, "Firebase");
        assert_eq!(
            f.stays,
            [
                "Firebase Hosting (firebase.json, files and rewrites served by Google)",
                "functions/ (Firebase Functions 2nd gen, already on Cloud Run functions)",
                "Firestore (firebase.json)",
            ]
        );
        assert!(f.processes.is_empty());
        // Functions alone do not say where the rest runs.
        let f = facts(&[
            ("firebase.json", r#"{"functions":{"source":"fn"}}"#),
            ("fn/package.json", r#"{"dependencies":{"firebase-functions":"^6.1.0"}}"#),
        ]);
        assert!(f.platforms.is_empty());
        assert_eq!(
            f.stays,
            ["fn/ (Firebase Functions 2nd gen, already on Cloud Run functions)"]
        );
        // Hosting of a folder that is gone is a leftover.
        let f = facts(&[("firebase.json", r#"{"hosting":{"public":"packages/old/dist"}}"#)]);
        assert!(f.platforms.is_empty() && f.stays.is_empty());
    }

    #[test]
    fn static_sites_and_deployable_folders() {
        let caddy = "FROM python:3.12 AS build\nRUN python build.py\nFROM caddy:2-alpine\nCOPY --from=build /s /srv\n";
        let f = facts(&[
            ("website/Dockerfile", caddy),
            ("tools/Dockerfile", "FROM alpine\nCMD [\"x\"]"),
        ]);
        assert_eq!(f.processes.len(), 1);
        let p = &f.processes[0];
        assert_eq!(
            (p.id.as_str(), p.name.as_str(), p.framework.as_deref(), p.from.as_str()),
            ("website", "static site", Some("Caddy"), "website/Dockerfile:3")
        );
        let f = facts(&[("Caddyfile", "localhost\n\nroot src\n\nfile_server\n")]);
        assert_eq!(f.processes[0].framework.as_deref(), Some("Caddy"));
        let f = facts(&[("package.json", r#"{"scripts":{"start":"npx serve -s build"}}"#)]);
        assert_eq!(f.processes[0].framework.as_deref(), Some("serve"));
        let f = facts(&[(
            "nginx.conf",
            "server {\n  root /var/www;\n  location ~ \\.php$ { fastcgi_pass app:9000; }\n}",
        )]);
        assert_eq!(f.processes[0].framework, None);
        let f = facts(&[
            ("api/Dockerfile", "FROM node:22\nCMD [\"node\", \"server.js\"]\n"),
            ("web/Dockerfile", "FROM nginx:1.27\n"),
            ("e2e-auth/Dockerfile", "FROM node:22\n"),
        ]);
        let got: Vec<_> = f
            .processes
            .iter()
            .map(|p| (p.id.as_str(), p.command.as_str()))
            .collect();
        assert_eq!(got, [("api", "node server.js"), ("web", "")]);
    }

    #[test]
    fn hosts_and_mobile_apps() {
        let f = facts(&[
            ("vercel.json", "{}"),
            ("Dockerfile", "FROM nginx\n"),
            ("docs-site/vercel.json", "{}"),
            ("docs-site/package.json", "{}"),
            ("examples/demo/vercel.json", "{}"),
        ]);
        assert_eq!(f.platforms[0].1.name, "Vercel");
        let got: Vec<_> = f
            .processes
            .iter()
            .map(|p| (p.id.as_str(), p.framework.as_deref()))
            .collect();
        assert_eq!(got, [("web", Some("nginx")), ("docs-site", Some("static site"))]);
        let f = facts(&[(".github/workflows/site.yml", "- uses: actions/deploy-pages@v4")]);
        assert_eq!(f.platforms[0].1.name, "GitHub Pages");
        assert_eq!(f.processes[0].name, "static site");
        let f = facts(&[
            ("mobile/pubspec.yaml", "flutter:\n  sdk: flutter"),
            ("mobile/ios/Runner.xcodeproj/project.pbxproj", "x"),
            ("Haptics.xcodeproj/project.pbxproj", "x"),
            ("server/Dockerfile", "FROM node\nCMD [\"node\", \"a.js\"]"),
        ]);
        assert_eq!(f.apps, ["Haptics.xcodeproj/", "mobile/"]);
        assert_eq!(f.processes.len(), 1);
    }
}
