//! One file per command (architecture §3).

use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform::Failure;

pub mod answer;
pub mod audit;
pub mod bill;
pub mod check;
pub mod describe;
pub mod init;
pub mod kb;
pub mod key;
pub mod plan;
pub mod report;
pub mod send;
pub mod status;

/// Prints why a call failed and returns its exit code (architecture §3): 3 the platform refused, 2 no answer.
pub fn failed(f: &Failure) -> ExitCode {
    eprintln!("exodus: {}", f.message());
    failed_code(f)
}

/// The exit code of a failed call, for a caller that said why itself.
pub fn failed_code(f: &Failure) -> ExitCode {
    match f {
        Failure::Refused(..) => ExitCode::from(3),
        Failure::Unreachable(_) => ExitCode::from(2),
    }
}

/// Now as ISO 8601 UTC, to the second.
pub fn now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Today in the person's own time zone (`TZ`, else the system's), as YYYY-MM-DD: the date plan.md shows, so a task
/// done at 01:28 is dated that day and not the day before in UTC. The platform keeps its times in UTC.
pub fn today() -> String {
    date_in(&jiff::tz::TimeZone::system(), jiff::Timestamp::now())
}

fn date_in(tz: &jiff::tz::TimeZone, at: jiff::Timestamp) -> String {
    at.to_zoned(tz.clone()).date().to_string()
}

/// Owner and name from the origin remote in `.git/config`: `git@github.com:owner/name.git` or `https://host/owner/name`.
pub fn origin(root: &Path) -> Option<(String, String)> {
    let config = std::fs::read_to_string(root.join(".git/config")).ok()?;
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == r#"[remote "origin"]"#;
        } else if in_origin && let Some(url) = line.strip_prefix("url") {
            let url = url
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .trim_end_matches('/')
                .trim_end_matches(".git");
            let mut parts = url.rsplit(['/', ':']);
            let name = parts.next()?.to_string();
            let owner = parts.next()?.to_string();
            return Some((owner, name));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    #[test]
    fn today_is_the_date_where_the_person_is() {
        use jiff::tz::{Offset, TimeZone};
        let at: jiff::Timestamp = "2026-10-11T23:28:00Z".parse().unwrap();
        let berlin = TimeZone::fixed(Offset::from_hours(2).unwrap());
        let chicago = TimeZone::fixed(Offset::from_hours(-5).unwrap());
        assert_eq!(super::date_in(&berlin, at), "2026-10-12");
        assert_eq!(super::date_in(&TimeZone::UTC, at), "2026-10-11");
        assert_eq!(super::date_in(&chicago, at), "2026-10-11");
        assert_eq!(super::today().len(), 10);
    }

    #[test]
    fn now_is_iso() {
        let n = super::now();
        assert_eq!(n.len(), 20);
        assert!(n.starts_with("20") && n.ends_with('Z'));
    }
}
