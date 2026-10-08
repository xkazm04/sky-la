//! Improvement wave 4: the screens report on periods the core works out
//! from the books' date and VAT status. The demo's date gives exactly the
//! periods its recordings were made for; real books on another date get
//! theirs.

#![allow(clippy::unwrap_used)]

use sha2::{Digest, Sha256};
use skyla_app::Core;
use skyla_app::dto::{EntitySetupDto, PeriodChoiceDto};
use skyla_app::session::Gate;

fn ids(list: &[PeriodChoiceDto]) -> Vec<(&str, &str, &str)> {
    list.iter()
        .map(|p| (p.label.as_str(), p.from.as_str(), p.to.as_str()))
        .collect()
}

#[test]
fn the_demo_reports_on_the_periods_it_was_recorded_for() {
    let p = Core::demo().unwrap().reporting_periods().unwrap();
    assert_eq!(
        (p.today.as_str(), p.year, p.books_from.as_str()),
        ("2026-10-07", 2026, "2026-04-01")
    );
    assert_eq!(
        ids(&p.vat),
        [
            ("September 2026", "2026-09-01", "2026-09-30"),
            ("August 2026", "2026-08-01", "2026-08-31"),
            ("July 2026", "2026-07-01", "2026-07-31"),
        ]
    );
    assert_eq!(
        ids(&p.quarters),
        [
            ("Q2 2026", "2026-04-01", "2026-06-30"),
            ("Q3 2026", "2026-07-01", "2026-09-30")
        ]
    );
    assert_eq!(p.last_quarter.unwrap().label, "Q3 2026");
    assert_eq!(p.prior_quarter.unwrap().label, "Q2 2026");
    assert_eq!(
        ids(std::slice::from_ref(&p.year_to_date)),
        [("Apr – Sep 2026", "2026-04-01", "2026-09-30")]
    );
}

fn setup(vat_period: &str) -> EntitySetupDto {
    EntitySetupDto {
        display_name: "Eva Malá".into(),
        ico: "27415830".into(),
        dic: (vat_period != "none").then(|| "CZ8001011234".into()),
        address: "Dlouhá 1, 110 00 Praha 1".into(),
        vat_period: vat_period.into(),
        registration: "Zapsána v živnostenském rejstříku".into(),
        iban: None,
        bank_name: "ČSOB".into(),
        email: None,
        flat_rate_group: Some("liberal".into()),
    }
}

#[test]
fn real_books_follow_their_date_and_vat_status() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate
        .create(&setup("monthly"), "a long passphrase for the books")
        .unwrap();
    let p = core.reporting_periods().unwrap();
    assert_eq!(p.books_from, "2026-01-01");
    assert_eq!(p.year_to_date.label, "Jan – Sep 2026");
    drop(core);

    // The same books opened in mid-January the year after.
    let key = skyla_store::DataKey::from_bytes(Sha256::digest(b"sky-la reproducible key 0").into());
    let core = Core::open_entity(&dir.path().join("books.db"), &key, "2027-01-15").unwrap();
    let p = core.reporting_periods().unwrap();
    assert_eq!(p.year, 2027);
    assert_eq!(
        p.vat.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(),
        ["December 2026", "November 2026", "October 2026"]
    );
    assert_eq!(
        p.quarters
            .iter()
            .map(|q| q.label.as_str())
            .collect::<Vec<_>>(),
        ["Q3 2026", "Q4 2026"]
    );
    assert_eq!(
        (
            p.year_to_date.label.as_str(),
            p.year_to_date.from.as_str(),
            p.year_to_date.to.as_str()
        ),
        ("Jan – Dec 2026", "2026-01-01", "2026-12-31")
    );
}

#[test]
fn quarterly_payers_file_quarters_and_non_payers_nothing() {
    for (vat, expect) in [
        ("quarterly", vec!["Q3 2026", "Q2 2026", "Q1 2026"]),
        ("none", vec![]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::reproducible(dir.path().to_path_buf());
        let (core, _) = gate
            .create(&setup(vat), "a long passphrase for the books")
            .unwrap();
        let p = core.reporting_periods().unwrap();
        assert_eq!(
            p.vat.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(),
            expect,
            "{vat}"
        );
    }
}
