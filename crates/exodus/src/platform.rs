//! The platform client (architecture §3, §4, §7): which key and which API a command uses, the calls, the exchange of a
//! one-time code and the credentials file it writes. The key is read from `EXODUS_KEY` (CI) or the credentials entry of
//! this repository's project, goes only into the Authorization header and the 0600 credentials file, and never into
//! output, a log or an error.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The production API when neither `.exodus/project.json` nor the credentials name one.
pub const DEFAULT_API: &str = "https://app.exodus.elim.io";

/// A linked project: where its API is and the key that opens it.
pub struct Link {
    pub api: String,
    key: String,
}

impl Link {
    /// The key's id as `exodus status` shows it: `exk_<project id>_` and the secret masked.
    pub fn key_id(&self) -> String {
        match self.key.rsplit_once('_') {
            Some((head, _)) if self.key.starts_with("exk_") => format!("{head}_..."),
            _ => "a key in EXODUS_KEY (not an exk_ key)".into(),
        }
    }
}

#[derive(Deserialize, Default)]
struct Credentials {
    #[serde(default)]
    projects: Vec<Entry>,
}

/// One `[[projects]]` entry of the credentials file.
#[derive(Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub device: String,
}

/// The person's home folder: `HOME`, else (Windows) `USERPROFILE`.
pub fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `~/.config/exodus/credentials` (or under `XDG_CONFIG_HOME`); on Windows `%APPDATA%\exodus\credentials`.
pub fn credentials_path() -> Option<PathBuf> {
    let nonempty = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = nonempty("XDG_CONFIG_HOME")
        .or_else(|| if cfg!(windows) { nonempty("APPDATA") } else { None })
        .or_else(|| home_dir().map(|h| h.join(".config")))?;
    Some(base.join("exodus").join("credentials"))
}

/// The link of the repository at `root`: `EXODUS_KEY`, else the credentials entry whose id is `project.json`'s
/// project. None when there is no key.
pub fn link(root: &Path) -> Option<Link> {
    if let Some(link) = ci_link(root) {
        return Some(link);
    }
    let project = project_file(root);
    let project_api = project.as_ref().and_then(|p| p.api.clone());
    let id = project?.project?;
    let text = std::fs::read_to_string(credentials_path()?).ok()?;
    let creds: Credentials = toml::from_str(&text).ok()?;
    let entry = creds.projects.into_iter().find(|e| e.id == id)?;
    Some(Link {
        api: entry.api.or(project_api).unwrap_or_else(|| DEFAULT_API.into()),
        key: entry.key,
    })
}

/// The link of a CI run: `EXODUS_KEY` only, never the credentials of the machine (`exodus check`, so a developer's
/// local run is not counted as a CI run). The API is `project.json`'s, else production.
pub fn ci_link(root: &Path) -> Option<Link> {
    let key = std::env::var("EXODUS_KEY").ok().filter(|k| !k.trim().is_empty())?;
    Some(Link {
        api: project_file(root)
            .and_then(|p| p.api)
            .unwrap_or_else(|| DEFAULT_API.into()),
        key: key.trim().to_string(),
    })
}

fn project_file(root: &Path) -> Option<exodus_schema::ProjectFile> {
    std::fs::read_to_string(root.join(".exodus/project.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Why a call did not give an answer.
pub enum Failure {
    /// The platform answered with a refusal: its status and its own sentence.
    Refused(u16, String),
    /// No answer at all: the URL and what went wrong.
    Unreachable(String),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Refused(status, m) => format!("the platform refused ({status}): {m}"),
            Failure::Unreachable(m) => m.clone(),
        }
    }
}

impl Link {
    /// `GET <api>/api/<path>` with `query`, as JSON.
    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, Failure> {
        self.get_text(path, query).map(|t| json(&t))
    }

    /// `GET <api>/api/<path>` with `query`, as text (a report's Markdown).
    pub fn get_text(&self, path: &str, query: &[(&str, &str)]) -> Result<String, Failure> {
        let url = url(&self.api, path);
        let res = once_more(|| {
            let mut req = agent().get(&url).header("Authorization", &self.bearer());
            for (k, v) in query {
                req = req.query(*k, *v);
            }
            req.call()
        });
        finish("GET", &url, Some(&self.key), res)
    }

    /// `POST <api>/api/<path>` with a JSON body, as JSON (an empty answer, 204, is null).
    pub fn post(&self, path: &str, body: &Value) -> Result<Value, Failure> {
        let url = url(&self.api, path);
        let res = agent()
            .post(&url)
            .header("Authorization", &self.bearer())
            .header("Content-Type", "application/json")
            .send(body.to_string());
        finish("POST", &url, Some(&self.key), res).map(|t| json(&t))
    }

    /// `PUT <api>/api/<path>` with a JSON body, as JSON. A PUT is idempotent by the API's contract.
    pub fn put(&self, path: &str, body: &Value) -> Result<Value, Failure> {
        let url = url(&self.api, path);
        let res = once_more(|| {
            agent()
                .put(&url)
                .header("Authorization", &self.bearer())
                .header("Content-Type", "application/json")
                .send(body.to_string())
        });
        finish("PUT", &url, Some(&self.key), res).map(|t| json(&t))
    }

    /// `DELETE <api>/api/<path>`. A DELETE is idempotent by the API's contract.
    pub fn delete(&self, path: &str) -> Result<Value, Failure> {
        let url = url(&self.api, path);
        // the API takes a write only as JSON (its CSRF rule), a body-less one too
        let res = once_more(|| {
            agent()
                .delete(&url)
                .header("Authorization", &self.bearer())
                .header("Content-Type", "application/json")
                .call()
        });
        finish("DELETE", &url, Some(&self.key), res).map(|t| json(&t))
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.key)
    }
}

/// `POST <api>/api/cli/exchange`: a one-time code for a project key (§7). No credentials; the answer holds the key, so
/// the caller writes it with `save` and prints nothing of it.
pub fn exchange(api: &str, body: &Value) -> Result<Value, Failure> {
    let url = url(api, "cli/exchange");
    let res = agent()
        .post(&url)
        .header("Content-Type", "application/json")
        .send(body.to_string());
    finish("POST", &url, None, res).map(|t| json(&t))
}

fn url(api: &str, path: &str) -> String {
    format!("{}/api/{path}", api.trim_end_matches('/'))
}

/// How long opening the connection (TLS included) may take before the call gives up, well inside the 30 s of the
/// whole call, so a connection that never opens is told apart from a slow answer.
const CONNECT: Duration = Duration::from_secs(10);

/// Runs an idempotent call (a GET, PUT or DELETE: the API's contract makes each safe to repeat) and runs it once more
/// when it failed before the request left: the name did not resolve or the connection did not open in time. Such a
/// request never reached the platform, so trying again cannot do anything twice. A POST is never repeated.
pub fn once_more<T>(mut call: impl FnMut() -> Result<T, ureq::Error>) -> Result<T, ureq::Error> {
    match call() {
        Err(ureq::Error::Timeout(ureq::Timeout::Connect | ureq::Timeout::Resolve)) => call(),
        done => done,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .timeout_connect(Some(CONNECT))
        .user_agent(concat!("exodus/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Names the proxy a call to `url` went through, as ureq picks it from the environment (ALL_PROXY, HTTPS_PROXY,
/// HTTP_PROXY and lowercase, minus NO_PROXY), so a refused connection points at the proxy rather than the platform.
/// Only the scheme, host and port: a proxy URL may carry a password.
fn via(url: &str) -> String {
    let (Some(p), Ok(uri)) = (ureq::Proxy::try_from_env(), url.parse()) else {
        return String::new();
    };
    if p.is_no_proxy(&uri) {
        return String::new();
    }
    let scheme = p.uri().scheme_str().unwrap_or("proxy");
    format!(" (through the proxy {scheme}://{}:{})", p.host(), p.port())
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

/// The answer's text when it is a 2xx; else the refusal with the platform's own sentence. A 429's sentence already says
/// when to try again.
fn finish(
    method: &str,
    url: &str,
    key: Option<&str>,
    res: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<String, Failure> {
    let mut res = res.map_err(|e| Failure::Unreachable(format!("{method} {url}: {e}{}", via(url))))?;
    let status = res.status().as_u16();
    let text = res
        .body_mut()
        .read_to_string()
        .map_err(|e| Failure::Unreachable(format!("{method} {url}: {e}")))?;
    if (200..300).contains(&status) {
        return Ok(text);
    }
    let said = json(&text)["message"].as_str().unwrap_or("no reason given").to_string();
    // a platform's sentence never holds the key, but a proxy's might echo a header back
    Err(Failure::Refused(
        status,
        key.map_or(said.clone(), |k| said.replace(k, "exk_...")),
    ))
}

/// Writes `entry` into the credentials file, replacing an entry with its id and keeping the others (and any field this
/// build does not know). The directory is 0700 and the file 0600 from its first byte: it is written beside and renamed.
pub fn save(entry: &Entry) -> Result<PathBuf, String> {
    use std::io::Write as _;
    #[cfg(unix)]
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    let path = credentials_path().ok_or("no HOME to keep the credentials in")?;
    let dir = path.parent().ok_or("no folder for the credentials")?;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut table: toml::Table = toml::from_str(&old).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut list = match table.remove("projects") {
        Some(toml::Value::Array(a)) => a,
        _ => Vec::new(),
    };
    list.retain(|v| v.get("id").and_then(toml::Value::as_str) != Some(entry.id.as_str()));
    list.push(toml::Value::try_from(entry).map_err(|e| e.to_string())?);
    table.insert("projects".into(), toml::Value::Array(list));
    let text = toml::to_string(&table).map_err(|e| e.to_string())?;

    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Note: on Windows there are no unix modes; the file sits under %APPDATA%, whose ACL already limits it to the
    // person's account
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::once_more;
    use ureq::{Error, Timeout};

    /// Runs `once_more` over calls that fail with `first`, then succeed; returns the result and the number of calls.
    fn run(first: Timeout) -> (bool, u32) {
        let mut calls = 0;
        let res = once_more(|| {
            calls += 1;
            if calls == 1 { Err(Error::Timeout(first)) } else { Ok(()) }
        });
        (res.is_ok(), calls)
    }

    #[test]
    fn a_call_that_never_left_is_tried_once_more_and_only_once() {
        assert_eq!(run(Timeout::Connect), (true, 2));
        assert_eq!(run(Timeout::Resolve), (true, 2));
        let mut calls = 0;
        let res: Result<(), Error> = once_more(|| {
            calls += 1;
            Err(Error::Timeout(Timeout::Connect))
        });
        assert!(matches!(res, Err(Error::Timeout(Timeout::Connect))));
        assert_eq!(calls, 2, "a second connect timeout is the answer");
    }

    #[test]
    fn a_call_that_may_have_reached_the_platform_is_not_repeated() {
        // the request was sent, or the answer was slow: the platform may have acted on it
        for t in [
            Timeout::Global,
            Timeout::SendBody,
            Timeout::RecvResponse,
            Timeout::RecvBody,
        ] {
            assert_eq!(run(t), (false, 1), "{t:?}");
        }
        let mut calls = 0;
        let ok = once_more(|| {
            calls += 1;
            Ok::<_, Error>(7)
        });
        assert_eq!((ok.unwrap(), calls), (7, 1));
    }
}
