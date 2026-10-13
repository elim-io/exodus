//! The cabinet prototype's numbers, cent for cent. `prototype.json` is written by the prototype's own `price()`
//! (`node src/cost.fixture.ts` in the platform's app/): every report of its sample projects at its own assumptions,
//! in each region, at ten times the visits and at 10 GB more egress, plus the pricing article's worked examples.

#![allow(clippy::unwrap_used)]
use exodus_cost::{bundled, price};
use exodus_schema::{Assumptions, Use};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    assumptions: Assumptions,
    parts: Vec<Part>,
    cents: Vec<i64>,
    today: i64,
    cloud_run: i64,
}

#[derive(Deserialize)]
struct Part {
    id: String,
    today: i64,
    uses: Vec<Use>,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("prototype.json")).unwrap()
}

fn usd(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

#[test]
fn every_case_equals_the_prototype_to_the_cent() {
    let p = bundled();
    let cases = cases();
    assert_eq!(cases.len(), 56);
    let mut wrong = Vec::new();
    for c in &cases {
        let uses: Vec<&[Use]> = c.parts.iter().map(|x| x.uses.as_slice()).collect();
        let got = price(&p, &uses, &c.assumptions).unwrap();
        let cents: Vec<i64> = got.iter().map(|x| x.cents).collect();
        let total: i64 = cents.iter().sum();
        assert_eq!(c.parts.iter().map(|x| x.today).sum::<i64>(), c.today, "{}", c.name);
        // `cargo test -p exodus-cost -- --nocapture` prints the table: case, today, prototype, Rust
        println!("{}\t{}\t{}\t{}", c.name, usd(c.today), usd(c.cloud_run), usd(total));
        if cents != c.cents || total != c.cloud_run {
            let ids: Vec<&str> = c.parts.iter().map(|x| x.id.as_str()).collect();
            wrong.push(format!(
                "{}: {ids:?} rust {cents:?} = {total}, prototype {:?} = {}",
                c.name, c.cents, c.cloud_run
            ));
        }
        // a part is the sum of its uses, the total the sum of the parts
        for part in &got {
            assert_eq!(part.uses.iter().map(|u| u.cents).sum::<i64>(), part.cents, "{}", c.name);
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The numbers mock.check.ts holds, read through the same cases.
#[test]
fn the_numbers_the_prototype_checks() {
    let p = bundled();
    let cases = cases();
    let total = |name: &str| {
        let c = cases
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        let uses: Vec<&[Use]> = c.parts.iter().map(|x| x.uses.as_slice()).collect();
        let cloud_run: i64 = price(&p, &uses, &c.assumptions).unwrap().iter().map(|x| x.cents).sum();
        (c.today, cloud_run)
    };
    for (name, today, cloud_run) in [
        ("acme-api report 3", "$48.00", "$11.11"),
        ("shop-bot report 1", "$32.00", "$0.85"),
        ("render-landing report 1", "$25.00", "$54.39"),
        ("link-preview report 1", "$8.48", "$0.60"),
        ("storefront report 1", "$26.00", "$3.61"),
        ("invoices report 1", "$20.00", "$9.66"),
    ] {
        let (t, c) = total(name);
        assert_eq!((usd(t).as_str(), usd(c).as_str()), (today, cloud_run), "{name}");
    }
    let acme = total("acme-api report 3").1;
    assert!(total("acme-api report 3 in europe-west2").1 > acme, "Tier 2 costs more");
    assert!(
        total("acme-api report 3 at 20000 visits").1 > acme,
        "more visits cost more"
    );
    assert_eq!(
        total("acme-api report 3 at 14 GB egress").1 - acme,
        120,
        "10 GB more egress is $1.20"
    );
    assert_eq!(
        total("render-landing report 1 at 50000 visits").1,
        5439,
        "always on: visits do not move it"
    );
    assert_eq!(usd(total("kb: Cloud SQL db-f1-micro, 730 h and 10 GB").1), "$9.37");
    assert_eq!(usd(total("kb: always on, 1 vCPU and 0.5 GiB all month").1), "$44.71");
}
