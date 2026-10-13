//! What a project costs on Google Cloud, computed by plain code. Each part of the app lists what it would use in a
//! month (vCPU-seconds, GB, requests, jobs) and each use is priced at Google's list price for the region, less the
//! free tier. A use is rounded to the cent, the way a bill rounds each of its lines; a part is the sum of its uses,
//! the total the sum of the parts, so every number in a report adds up. The cabinet's TypeScript `price()` does the
//! same over `report.prices` and is held to these numbers to the cent.
//!
//! The prices are `prices.json`, dated and bundled, so a report builds offline.

use std::collections::HashMap;

use exodus_schema::{Assumptions, PlatformPrices, PricedUse, Prices, Use};

/// The month a per-visit use is multiplied over.
pub const DAYS: f64 = 30.0;

/// Cloud Run's Tier 2 regions cost this much of Tier 1.
const TIER2: f64 = 1.4;

/// `prices.json` as shipped with this build.
pub fn bundled() -> Prices {
    // Note: the file is compiled in and parsed by a test, so a malformed file never reaches a release
    #[allow(clippy::expect_used)]
    serde_json::from_str(include_str!("../prices.json")).expect("prices.json parses")
}

/// `platforms.json` as shipped with this build: the platforms' list prices for the Today column before a bill.
pub fn platforms() -> PlatformPrices {
    // Note: compiled in and parsed by a test, as prices.json
    #[allow(clippy::expect_used)]
    serde_json::from_str(include_str!("../platforms.json")).expect("platforms.json parses")
}

/// One part priced: its uses and their sum in cents.
#[derive(Debug, Clone, PartialEq)]
pub struct PricedPart {
    pub uses: Vec<PricedUse>,
    pub cents: i64,
}

/// The price of one unit of `sku` in `region`.
pub fn unit_price(p: &Prices, sku: &str, region: &str) -> Result<f64, String> {
    let d = p.skus.get(sku).ok_or_else(|| format!("unknown sku {sku}"))?;
    let r = p
        .regions
        .get(region)
        .ok_or_else(|| format!("unknown region {region}"))?;
    Ok(match (sku, d.scope.as_deref()) {
        ("gcs", _) => r.gcs,
        (_, Some("run")) if r.tier == 2 => d.price * TIER2,
        (_, Some("sql")) => d.price * r.sql,
        _ => d.price,
    })
}

/// The free tier in dollars. Cloud Run's is "a spending based discount using Tier 1 pricing", so in a Tier 2 region
/// it covers fewer seconds; Cloud Storage's 5 GB exists only in US regions.
fn free_usd(p: &Prices, sku: &str, region: &str) -> f64 {
    match (sku, p.regions.get(region)) {
        ("gcs", Some(r)) => r.gcs_free * r.gcs,
        _ => p.skus.get(sku).map_or(0.0, |d| d.free * d.price),
    }
}

/// A bill's rounding: the nearest cent, a half cent up (the tiny bias keeps 0.125 * 100 from landing on 12.4999).
fn cents(usd: f64) -> i64 {
    (usd * 100.0 + 1e-6).round() as i64
}

/// Prices every use of every part in order; the free tier goes to the first parts that use a line. Free tiers are
/// per billing account, so this gives all of them to one app.
pub fn price(p: &Prices, parts: &[&[Use]], a: &Assumptions) -> Result<Vec<PricedPart>, String> {
    let mut left: HashMap<&str, f64> = HashMap::new();
    parts
        .iter()
        .map(|uses| {
            let uses = uses
                .iter()
                .map(|u| {
                    let qty = match u.per.as_deref() {
                        Some("visit") => u.qty * a.visits as f64 * DAYS,
                        Some("egress") => u.qty * a.egress_gb,
                        None => u.qty,
                        Some(other) => return Err(format!("unknown per {other}")),
                    };
                    let price = unit_price(p, &u.sku, &a.region)?;
                    let gross = qty * price;
                    let pool = left
                        .get(u.sku.as_str())
                        .copied()
                        .unwrap_or_else(|| free_usd(p, &u.sku, &a.region));
                    let free = pool.min(gross);
                    left.insert(&u.sku, pool - free);
                    Ok(PricedUse {
                        sku: u.sku.clone(),
                        how: u.how.clone(),
                        qty,
                        price,
                        gross,
                        free,
                        cents: cents(gross - free),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let cents = uses.iter().map(|u| u.cents).sum();
            Ok(PricedPart { uses, cents })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_prices_parse_and_every_sku_names_a_product() {
        let p = bundled();
        assert_eq!(p.date.len(), 10);
        for (id, s) in &p.skus {
            assert!(p.products.contains_key(&s.product), "{id} product {}", s.product);
            assert!(matches!(s.scope.as_deref(), None | Some("run" | "sql")), "{id} scope");
        }
        for (id, r) in &p.regions {
            assert!(matches!(r.tier, 1 | 2), "{id} tier");
        }
    }

    #[test]
    fn bundled_platforms_parse() {
        let p = platforms();
        assert_eq!(p.date.len(), 10);
        let names: Vec<&str> = p.platforms.keys().map(String::as_str).collect();
        assert_eq!(names, ["Fly.io", "Heroku", "Railway", "Render"]);
        assert_eq!(p.platforms["Heroku"].process.cents, 700.0);
    }

    #[test]
    fn unknown_names_are_errors_not_zero() {
        let p = bundled();
        let a = Assumptions {
            visits: 1,
            region: "europe-west1".into(),
            egress_gb: 0.0,
            egress_mb_per_visit: None,
            files_mb_per_visitor: None,
        };
        let u = |sku: &str, per: Option<&str>| Use {
            sku: sku.into(),
            qty: 1.0,
            per: per.map(Into::into),
            how: String::new(),
        };
        assert!(price(&p, &[&[u("nope", None)]], &a).is_err());
        assert!(price(&p, &[&[u("egress", Some("hour"))]], &a).is_err());
        let mars = Assumptions {
            region: "mars-north1".into(),
            ..a
        };
        assert!(price(&p, &[&[u("egress", None)]], &mars).is_err());
    }

    /// The free tiers and the egress price, as Google publishes them (Cloud Run: cloud.google.com/run/pricing; Tasks,
    /// Scheduler, Secret Manager, Artifact Registry and Cloud Storage: their pricing pages; read 2026-10-11, and the
    /// billing export of the same day shows each of these pools). A change here is a decision, not a typo.
    #[test]
    fn free_tiers_and_egress_are_what_google_publishes() {
        let p = bundled();
        let free = |sku: &str| p.skus[sku].free;
        // request-based services; the pool is per billing account and counted at Tier 1 prices
        assert_eq!(
            (free("run-cpu"), free("run-mem"), free("run-req")),
            (180_000.0, 360_000.0, 2_000_000.0)
        );
        // instance-based services and jobs: no request fee
        for sku in ["run-cpu-always", "job-cpu"] {
            assert_eq!(free(sku), 240_000.0, "{sku}");
        }
        for sku in ["run-mem-always", "job-mem"] {
            assert_eq!(free(sku), 450_000.0, "{sku}");
        }
        assert_eq!((free("tasks"), free("scheduler")), (1_000_000.0, 3.0));
        assert_eq!((free("secrets"), free("ar")), (6.0, 0.5));
        // Cloud Storage: 5,000 class A operations in any region, 5 GB of storage only in us-central1
        assert_eq!(free("gcs-write"), 5_000.0);
        assert_eq!(p.regions["us-central1"].gcs_free, 5.0);
        assert_eq!(p.regions["europe-west1"].gcs_free, 0.0);
        // $0.12 a GB everywhere, kept on purpose: Cloud Run bills $0.105 a GiB from Europe to Europe
        assert_eq!(p.skus["egress"].price, 0.12);
        assert_eq!(p.skus["egress"].free, 0.0);
    }

    /// Firestore Standard edition (cloud.google.com/firestore/pricing and the Cloud Billing Catalog, read 2026-10-13):
    /// 50,000 reads, 20,000 writes and 1 GiB a day free, 30 days of it a month, at europe-west1's price. The database
    /// window's `firestore` block says the same numbers per day and per 100,000.
    #[test]
    fn firestore_is_what_google_publishes_and_agrees_with_its_block() {
        let p = bundled();
        let (r, w, s) = (
            &p.skus["firestore-read"],
            &p.skus["firestore-write"],
            &p.skus["firestore-storage"],
        );
        assert_eq!((r.free, w.free, s.free), (1_500_000.0, 600_000.0, 1.0));
        let f = &p.firestore;
        assert_eq!((r.free, w.free), (f.free_reads_day * DAYS, f.free_writes_day * DAYS));
        assert_eq!(s.free, f.free_gib);
        assert!((r.price * 1e5 - f.read_per100k).abs() < 1e-12);
        assert!((w.price * 1e5 - f.write_per100k).abs() < 1e-12);
        assert_eq!(s.price, f.gib_month);
        assert!(r.scope.is_none() && w.scope.is_none() && s.scope.is_none());
    }

    #[test]
    fn a_month_of_free_reads_costs_nothing_and_the_next_million_does() {
        let p = bundled();
        let a = Assumptions {
            visits: 1,
            region: "us-east1".into(),
            egress_gb: 0.0,
            egress_mb_per_visit: None,
            files_mb_per_visitor: None,
        };
        let reads = |qty: f64| {
            let u = Use {
                sku: "firestore-read".into(),
                qty,
                per: None,
                how: String::new(),
            };
            price(&p, &[&[u]], &a).unwrap()[0].cents
        };
        assert_eq!((reads(1_500_000.0), reads(2_500_000.0)), (0, 33));
    }

    #[test]
    fn five_thousand_writes_a_month_are_free_and_the_next_ones_are_not() {
        let p = bundled();
        let a = Assumptions {
            visits: 1,
            region: "europe-west1".into(),
            egress_gb: 0.0,
            egress_mb_per_visit: None,
            files_mb_per_visitor: None,
        };
        let writes = |qty: f64| {
            let u = Use {
                sku: "gcs-write".into(),
                qty,
                per: None,
                how: String::new(),
            };
            price(&p, &[&[u]], &a).unwrap()[0].cents
        };
        assert_eq!((writes(5_000.0), writes(7_000.0)), (0, 1));
    }
}
