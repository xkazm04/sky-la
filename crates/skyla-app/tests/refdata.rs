//! WP-20 acceptance (the core half): the app works with fetching off,
//! fetches only when the user turns it on, records where every figure came
//! from, and refuses a pack update it can't verify.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use skyla_app::Core;

const DAILY: &str =
    include_str!("../../../packages/fixtures/data/refdata/demo-cnb-daily-2026-10-07.txt");
const REPO: &str =
    include_str!("../../../packages/fixtures/data/refdata/demo-cnb-repo-history.csv");

#[test]
fn nothing_is_fetched_unless_the_user_turns_it_on() {
    let core = Core::demo().unwrap();
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&calls);
    core.replace_fetcher(Box::new(move |url| {
        seen.lock().unwrap().push(url.to_owned());
        Ok(DAILY.to_owned())
    }));

    let start = core.reference_data().unwrap();
    assert!(!start.fetch_enabled, "off by default");
    assert_eq!(
        (start.fx_days, start.repo_changes, start.euro),
        (0, 0, None)
    );
    let refused = core.fetch_cnb_rates("2026-10-07").unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("fetching reference data is off"),
        "{refused}"
    );
    assert!(calls.lock().unwrap().is_empty(), "no request while off");

    // The whole app works without reference data.
    assert!(core.profit_and_loss("2026-07-01", "2026-09-30").is_ok());
    assert!(core.bank_statement().is_ok());

    core.set_reference_fetch(true).unwrap();
    let fetched = core.fetch_cnb_rates("2026-10-07").unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [
            "https://www.cnb.cz/cs/financni-trhy/devizovy-trh/kurzy-devizoveho-trhu/kurzy-devizoveho-trhu/denni_kurz.txt?date=07.10.2026"
        ]
    );
    assert_eq!(
        fetched.euro.as_deref(),
        Some("EUR 25,14 Kč on 2026-10-07 (ČNB #194)")
    );
    assert!(
        fetched.sources[0]
            .origin
            .starts_with("fetched https://www.cnb.cz/")
    );
}

#[test]
fn imports_record_their_origin_and_feed_late_interest() {
    let core = Core::demo().unwrap();
    let fx = core
        .import_reference_data("cnb_fx", "denni_kurz.txt", DAILY)
        .unwrap();
    assert_eq!(fx.fx_days, 1);
    assert_eq!(fx.sources[0].origin, "imported denni_kurz.txt");
    let bad = core
        .import_reference_data("cnb_fx", "x.txt", "not a rates file")
        .unwrap_err();
    assert!(bad.to_string().contains("x.txt"), "{bad}");

    // Without the repo history the final notice says what's missing.
    let before = core.dunning_queue("2026-10-26").unwrap();
    let last = before.iter().find(|n| n.number == "2026-102").unwrap();
    assert_eq!(last.tone, "final");
    assert!(last.interest.is_none());
    assert!(
        last.interest_problem
            .as_deref()
            .unwrap()
            .contains("import the ČNB rate history")
    );

    let repo = core
        .import_reference_data("cnb_repo", "repo.csv", REPO)
        .unwrap();
    assert_eq!(
        (repo.repo_changes, repo.repo_now.as_deref()),
        (2, Some("3,5 % since 2025-05-02"))
    );
    let after = core.dunning_queue("2026-10-26").unwrap();
    let last = after.iter().find(|n| n.number == "2026-102").unwrap();
    let interest = last.interest.as_ref().unwrap();
    // 53 092,00 open from 25 September (H2: 3,50 + 8) to 26 October, 32 days:
    // 53 092 × 11.5 % × 32 / 365 = 535.2837… → 535,28
    assert_eq!(
        (interest.annual_rate.as_str(), interest.total.minor),
        ("11.5", 53_528)
    );
    assert!(last.body_cs.contains("535,28"));
}

#[test]
fn a_pack_update_needs_a_trusted_key() {
    let core = Core::demo().unwrap();
    let pack = include_str!("../../../rules/cz/2026/pack.toml").replacen(
        "version = \"2026.1\"",
        "version = \"2026.2\"",
        1,
    );
    let err = core
        .install_pack_update(&pack, "untrusted comment: x\nRWQ")
        .unwrap_err();
    assert!(err.to_string().contains("no trusted signing key"), "{err}");
    assert_eq!(core.reference_data().unwrap().trusted_keys, 0);
    assert_eq!(
        core.rule_pack().provenance,
        "cz-2026@2026.1",
        "nothing changed"
    );
}
