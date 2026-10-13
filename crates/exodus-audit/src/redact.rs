//! Text from the repository that goes into findings.json is cut of anything that looks like a value: the audit
//! reads env names, never env values, and a command or a source line must not carry one out by accident.

use regex::Regex;
use std::sync::LazyLock;

/// The patterns below are constants that the tests at the bottom compile and run.
#[allow(clippy::expect_used)]
fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("constant pattern")
}

/// `NAME=value ` in front of a command (`DATABASE_URL=postgres://... node server.js`).
static LEADING: LazyLock<Regex> = LazyLock::new(|| re(r#"^([A-Za-z_][A-Za-z0-9_]*)=(?:"[^"]*"|'[^']*'|\S*)\s*"#));
/// `--token=abc`, `--password abc`: a flag named like a secret.
static FLAG: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(--[\w-]*(?:secret|passw(?:or)?d|token|api-?key|key)[\w-]*)(?:=|\s+)[^\s-]\S*"));
/// `://user:password@host`
static USERINFO: LazyLock<Regex> = LazyLock::new(|| re(r#"(\w+://[^\s:/@'"]+:)[^\s@'"/]+@"#));
/// `secret: 'abc'`, `PASSWORD = "abc"`, `api_key="abc"`
static LITERAL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)(\w*(?:secret|passw(?:or)?d|token|api_?key|private_?key|credential)\w*["']?\s*[:=]\s*)(["'`])[^"'`]+["'`]"#,
    )
});

/// A start command with the values of its leading assignments and of its secret-named flags replaced by `…`.
pub fn command(command: &str) -> String {
    let mut out = String::new();
    let mut rest = command.trim_start();
    while let Some(m) = LEADING.captures(rest) {
        out.push_str(&format!("{}=… ", &m[1]));
        rest = &rest[m[0].len()..];
    }
    out.push_str(rest);
    let out = USERINFO.replace_all(&out, "${1}…@");
    FLAG.replace_all(&out, "${1}=…").into_owned()
}

/// Env names that `command` assigns in front of the program.
pub fn assigned(command: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = command.trim_start();
    while let Some((word, tail)) = rest.split_once(' ') {
        match word.split_once("=…") {
            Some((name, "")) if !name.is_empty() => names.push(name),
            _ => break,
        }
        rest = tail.trim_start();
    }
    names
}

/// A source line without credentials: user:password in a URL and the string after a secret-named key.
pub fn line(line: &str) -> String {
    let out = USERINFO.replace_all(line, "${1}…@");
    LITERAL.replace_all(&out, "${1}${2}…${2}").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_lose_values_and_keep_names() {
        let c = command("DATABASE_URL=postgres://u:pw@h/d PORT=80 node s.js --token=abc --port 80 --api-key xyz");
        assert_eq!(c, "DATABASE_URL=… PORT=… node s.js --token=… --port 80 --api-key=…");
        assert_eq!(assigned(&c), ["DATABASE_URL", "PORT"]);
        assert_eq!(
            command("gunicorn app:app -b 0.0.0.0:$PORT"),
            "gunicorn app:app -b 0.0.0.0:$PORT"
        );
        assert_eq!(command("node s.js --a=b"), "node s.js --a=b");
        assert!(assigned("node s.js A=…").is_empty());
    }

    #[test]
    fn lines_lose_credentials_and_keep_env_reads() {
        assert_eq!(
            line("app.use(session({ secret: 'hunter2' }));"),
            "app.use(session({ secret: '…' }));"
        );
        assert_eq!(line("SECRET_KEY = \"abc\""), "SECRET_KEY = \"…\"");
        assert_eq!(line("url = \"postgres://u:pw@h/db\""), "url = \"postgres://u:…@h/db\"");
        let env = "app.use(session({ secret: process.env.SESSION_SECRET, resave: false }));";
        assert_eq!(line(env), env);
    }
}
