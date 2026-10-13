//! `exodus bill` (architecture §3, §4.4 `POST cli/bill`): a month of the person's own Google Cloud bill for the moved
//! project, read from their BigQuery billing export with their own `gcloud` and `bq` login, read-only: `gcloud ...
//! describe/list`, `bq ls` and one `bq query` with a SELECT. It never enables an API, makes a dataset or table, or
//! changes IAM or billing. The rows are grouped by service, SKU and region; with a linked account they go to the
//! cabinet, which sets them against the report. The billing account id and project numbers never leave the machine.

use std::path::Path;
use std::process::{Command, ExitCode};

use serde_json::{Value, json};

#[derive(clap::Args)]
pub struct Args {
    /// The Google Cloud project the app moved to (default gcloud's current project)
    #[arg(long, value_name = "ID")]
    project: Option<String>,
    /// The month, YYYY-MM (default last month)
    #[arg(long, value_name = "YYYY-MM")]
    month: Option<String>,
    /// The export table, PROJECT.DATASET.TABLE (default: found with bq ls)
    #[arg(long, value_name = "TABLE")]
    table: Option<String>,
    /// Read and print the month, send nothing
    #[arg(long)]
    dry_run: bool,
}

/// The month of one project from the standard or detailed export (both carry these columns). Credits are summed
/// apart: free tier, discounts and commitments make the net the report compares with; promotions are left out,
/// because a trial credit is money the person would otherwise pay.
const QUERY: &str = "SELECT
  service.description AS service, sku.id AS sku_id, sku.description AS sku,
  IFNULL(location.region, location.location) AS region, usage.pricing_unit AS unit, currency,
  SUM(usage.amount_in_pricing_units) AS usage, SUM(cost) AS cost,
  SUM((SELECT IFNULL(SUM(c.amount), 0) FROM UNNEST(credits) c
       WHERE c.type IN ('FREE_TIER', 'DISCOUNT', 'SUSTAINED_USAGE_DISCOUNT')
          OR c.type LIKE 'COMMITTED_USAGE_DISCOUNT%')) AS credits
FROM `{table}`
WHERE invoice.month = @month AND project.id = @project
GROUP BY service, sku_id, sku, region, unit, currency
HAVING usage != 0 OR cost != 0 OR credits != 0
ORDER BY cost DESC";

const NO_EXPORT: &str = "No billing export to BigQuery was found for this project's billing account. Google keeps usage and cost per SKU only there. To turn it on:
  1. Console > Billing > Billing export (console.cloud.google.com/billing/export), choose the billing account.
  2. Tab \"BigQuery export\", \"Standard usage cost\", \"Edit settings\".
  3. Project: any project on that account. Dataset: a new one, location multi-region EU or US.
  4. Save. \"Detailed usage cost\" too, if you like: it splits Cloud Run per service.
It needs Billing Account Administrator or Costs Manager on the account and BigQuery User on the project. A multi-region
dataset also gets the previous month (it can take five days to catch up). Run `exodus bill` again once rows are there,
or pass --table PROJECT.DATASET.TABLE when you know it.";

pub fn run(args: Args) -> ExitCode {
    match read(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("exodus bill: {e}");
            ExitCode::FAILURE
        }
    }
}

fn read(args: &Args) -> Result<ExitCode, String> {
    let today = super::now();
    let month = match &args.month {
        Some(m) if valid_month(m) => m.clone(),
        Some(_) => return Err("--month takes YYYY-MM, as 2026-09".into()),
        None => last_month(&today),
    };
    let project = match &args.project {
        Some(p) => p.trim().to_string(),
        None => tool("gcloud", &["config", "get-value", "project"])?.trim().to_string(),
    };
    if !id_ok(&project) || project == "(unset)" {
        return Err("name the moved project with --project ID (gcloud has no current project)".into());
    }
    let table = match &args.table {
        Some(t) => table_name(t).ok_or("--table takes PROJECT.DATASET.TABLE (or PROJECT:DATASET.TABLE)")?,
        None => match find_table(&project)? {
            Some(t) => t,
            None => {
                eprintln!("{NO_EXPORT}");
                return Ok(ExitCode::FAILURE);
            }
        },
    };
    let job_project = table.split('.').next().unwrap_or("").to_string();
    eprintln!("exodus bill: {month} of {project} from {table}");
    let out = tool(
        "bq",
        &[
            "query",
            &format!("--project_id={job_project}"),
            "--use_legacy_sql=false",
            "--format=json",
            "--max_rows=10000",
            &format!("--parameter=month:STRING:{}", month.replace('-', "")),
            &format!("--parameter=project:STRING:{project}"),
            &QUERY.replace("{table}", &table),
        ],
    )?;
    let found: Vec<Value> = if out.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(out.trim()).map_err(|e| format!("bq query answered something that is not JSON: {e}"))?
    };
    if found.is_empty() {
        return Err(format!(
            "the export holds no rows of {project} for {month}; a project moved this month has rows from its first day there"
        ));
    }
    if let Some(c) = found
        .iter()
        .filter_map(|r| r["currency"].as_str())
        .find(|c| *c != "USD")
    {
        return Err(format!(
            "the bill is in {c}; the report prices in USD, so the two cannot be compared yet"
        ));
    }
    if found.len() > 2000 {
        return Err(format!("{} rows; the platform takes at most 2,000", found.len()));
    }
    let rows: Vec<Value> = found.iter().map(row).collect();
    print(&rows);
    let final_from = format!("{}-04", next_month(&month));
    if today[..10] < *final_from {
        println!("The export runs hours behind and the month is open: this is partial until {final_from}.");
    }

    if args.dry_run {
        println!("Dry run: nothing sent.");
        return Ok(ExitCode::SUCCESS);
    }
    let Some(link) = crate::platform::link(Path::new(".")) else {
        println!(
            "Not linked: the cabinet compares the bill with the report once `exodus init --code` links this repository."
        );
        return Ok(ExitCode::SUCCESS);
    };
    let actual = match link.post("cli/bill", &json!({ "month": month, "rows": rows })) {
        Ok(v) => v["actual"].clone(),
        Err(f) => return Ok(super::failed(&f)),
    };
    let parts: i64 = actual["parts"]
        .as_object()
        .map_or(0, |m| m.values().filter_map(Value::as_i64).sum());
    let other: i64 = actual["other"]
        .as_array()
        .map_or(0, |a| a.iter().filter_map(|o| o["cents"].as_i64()).sum());
    let against = match actual["report"].as_u64() {
        Some(n) => format!("against report {n}"),
        None => "with no report yet to set it against".into(),
    };
    println!(
        "The cabinet has {month}: {} {against}, on the After tab.",
        usd(parts + other)
    );
    Ok(ExitCode::SUCCESS)
}

/// A row as `POST cli/bill` takes it: cents may carry fractions (the export keeps micro-dollars).
fn row(r: &Value) -> Value {
    let num = |v: &Value| v.as_f64().or_else(|| v.as_str()?.parse().ok()).unwrap_or(0.0);
    let cents = |v: &Value| (num(v) * 1_000_000.0).round() / 10_000.0;
    let text = |v: &Value| v.as_str().unwrap_or("").trim().to_string();
    let mut out = json!({
        "service": text(&r["service"]),
        "skuId": text(&r["sku_id"]),
        "sku": text(&r["sku"]),
        "usage": num(&r["usage"]),
        "unit": text(&r["unit"]),
        "costCents": cents(&r["cost"]),
        "creditsCents": cents(&r["credits"]),
    });
    if let Some(region) = r["region"].as_str().filter(|s| !s.is_empty()) {
        out["region"] = json!(region);
    }
    out
}

fn print(rows: &[Value]) {
    let f = |v: &Value| v.as_f64().unwrap_or(0.0);
    let s = |v: &Value| v.as_str().unwrap_or("").to_string();
    let mut net = 0.0;
    println!(
        "{:<22} {:<46} {:<14} {:>16} {:>10} {:>10}",
        "service", "sku", "region", "usage", "cost", "net"
    );
    for r in rows {
        let n = f(&r["costCents"]) + f(&r["creditsCents"]);
        net += n;
        let sku: String = s(&r["sku"]).chars().take(46).collect();
        let usage = format!("{:.2} {}", f(&r["usage"]), s(&r["unit"]));
        println!(
            "{:<22} {:<46} {:<14} {:>16} {:>10} {:>10}",
            s(&r["service"]).chars().take(22).collect::<String>(),
            sku,
            s(&r["region"]),
            usage,
            format!("${:.2}", f(&r["costCents"]) / 100.0),
            format!("${:.2}", n / 100.0)
        );
    }
    println!(
        "{} rows, net ${:.2} (cost less free tier, discounts and commitments)",
        rows.len(),
        net / 100.0
    );
}

fn usd(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

/// Runs a read-only gcloud or bq command; its stdout, or its last error line.
fn tool(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).args(args).output().map_err(|e| {
        format!("cannot run {program} ({e}); exodus bill reads the export with the Google Cloud CLI (gcloud and bq), signed in with `gcloud auth login`")
    })?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let last = err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no reason given");
    Err(format!("{program} {}: {}", args.first().unwrap_or(&""), last.trim()))
}

/// notes.md §6 steps 1 and 2: the project's billing account, then the export table among the account's projects
/// (the detailed one when both are there, since it names each Cloud Run service).
fn find_table(project: &str) -> Result<Option<String>, String> {
    let described = tool(
        "gcloud",
        &[
            "billing",
            "projects",
            "describe",
            project,
            "--format=value(billingAccountName,billingEnabled)",
        ],
    )?;
    let mut parts = described.split_whitespace();
    let account = parts
        .next()
        .and_then(|a| a.strip_prefix("billingAccounts/"))
        .filter(|a| a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .ok_or_else(|| format!("{project} has no billing account that gcloud can see"))?
        .to_string();
    if parts.next().is_some_and(|e| e.eq_ignore_ascii_case("false")) {
        return Err(format!("billing is off for {project}, so there is no bill to read"));
    }
    let suffix = account.replace('-', "_");
    let wanted = [
        format!("gcp_billing_export_resource_v1_{suffix}"),
        format!("gcp_billing_export_v1_{suffix}"),
    ];
    let candidates = tool(
        "gcloud",
        &[
            "billing",
            "projects",
            "list",
            &format!("--billing-account={account}"),
            "--format=value(projectId)",
        ],
    )
    .or_else(|_| tool("gcloud", &["projects", "list", "--format=value(projectId)"]))?;
    for p in candidates.lines().map(str::trim).filter(|p| id_ok(p)) {
        // no BigQuery in a project is an error here: skip it, never enable it
        let Ok(sets) = tool(
            "bq",
            &["ls", &format!("--project_id={p}"), "--format=json", "-n", "1000"],
        ) else {
            continue;
        };
        for ds in ids(&sets, "datasetReference", "datasetId") {
            let Ok(tables) = tool("bq", &["ls", "--format=json", "-n", "1000", &format!("{p}:{ds}")]) else {
                continue;
            };
            let have = ids(&tables, "tableReference", "tableId");
            if let Some(t) = wanted.iter().find(|w| have.contains(w)) {
                return Ok(Some(format!("{p}.{ds}.{t}")));
            }
        }
    }
    Ok(None)
}

fn ids(json: &str, reference: &str, field: &str) -> Vec<String> {
    serde_json::from_str::<Vec<Value>>(json.trim())
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v[reference][field].as_str().map(String::from))
        .filter(|s| s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
        .collect()
}

/// A project id (domain-scoped ones too): it goes into argv and into the query's table name, so nothing else passes.
fn id_ok(p: &str) -> bool {
    (4..=100).contains(&p.len())
        && p.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.:".contains(&b))
}

/// `P.D.T` or `P:D.T` as `P.D.T`, checked: it is written into the query between backticks.
fn table_name(t: &str) -> Option<String> {
    let t = t.trim();
    let (p, rest) = t
        .split_once(':')
        .filter(|(p, _)| !p.contains('.'))
        .or_else(|| t.split_once('.'))?;
    let (d, table) = rest.split_once('.')?;
    let word = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    (id_ok(p) && word(d) && word(table)).then(|| format!("{p}.{d}.{table}"))
}

fn valid_month(m: &str) -> bool {
    m.len() == 7
        && m.as_bytes()[4] == b'-'
        && m[..4].bytes().all(|b| b.is_ascii_digit())
        && m[5..].parse::<u32>().is_ok_and(|n| (1..=12).contains(&n))
}

fn last_month(today: &str) -> String {
    let (y, m): (i32, i32) = (today[..4].parse().unwrap_or(2026), today[5..7].parse().unwrap_or(1));
    if m == 1 {
        format!("{}-12", y - 1)
    } else {
        format!("{y}-{:02}", m - 1)
    }
}

fn next_month(month: &str) -> String {
    let (y, m): (i32, i32) = (month[..4].parse().unwrap_or(2026), month[5..7].parse().unwrap_or(1));
    if m == 12 {
        format!("{}-01", y + 1)
    } else {
        format!("{y}-{:02}", m + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_tables_and_rows() {
        assert_eq!(last_month("2026-01-05T00:00:00Z"), "2025-12");
        assert_eq!(last_month("2026-10-11T00:00:00Z"), "2026-09");
        assert_eq!(next_month("2026-12"), "2027-01");
        assert!(valid_month("2026-09") && !valid_month("2026-13") && !valid_month("2026-9"));
        assert_eq!(
            table_name("my-proj:billing.gcp_x").as_deref(),
            Some("my-proj.billing.gcp_x")
        );
        assert_eq!(
            table_name("my-proj.billing.gcp_x").as_deref(),
            Some("my-proj.billing.gcp_x")
        );
        assert_eq!(table_name("p`; DROP.x.y"), None);
        assert_eq!(table_name("my-proj.billing"), None);
        let r = row(
            &json!({"service": "Cloud Run", "sku_id": "4856-B847-F1EB", "sku": "Services CPU", "region": "europe-west1",
            "unit": "second", "currency": "USD", "usage": "22703.5", "cost": "0.544728", "credits": -0.544728}),
        );
        assert_eq!(r["costCents"], json!(54.4728));
        assert_eq!(r["creditsCents"], json!(-54.4728));
        assert_eq!(r["usage"], json!(22703.5));
    }
}
