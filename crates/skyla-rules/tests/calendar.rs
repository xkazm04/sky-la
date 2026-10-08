//! WP-23 acceptance: the 2026 calendar comes from the pack, and deadlines
//! move past weekends and public holidays.

#![allow(clippy::unwrap_used)]

use skyla_rules::Pack;

fn due(pack: &Pack, facts: &[&str], obligation: &str, period: &str) -> (String, String) {
    let d = pack
        .calendar(2026, facts)
        .unwrap()
        .into_iter()
        .find(|d| d.obligation == obligation && d.period == period)
        .unwrap_or_else(|| panic!("{obligation} {period} not in the calendar"));
    (d.nominal, d.due)
}

const OSVC_MONTHLY_VAT: &[&str] = &["osvc", "vat_monthly"];

#[test]
fn weekends_and_holidays_move_deadlines() {
    let pack = Pack::cz_2026().unwrap();
    let f = OSVC_MONTHLY_VAT;
    // 25 October 2026 is a Sunday.
    assert_eq!(
        due(&pack, f, "vat.return.monthly", "2026-09"),
        ("2026-10-25".into(), "2026-10-26".into())
    );
    // 25 December is Christmas (a Friday), then a weekend: Monday 28th.
    assert_eq!(
        due(&pack, f, "vat.control_statement.monthly", "2026-11"),
        ("2026-12-25".into(), "2026-12-28".into())
    );
    // 1 May is Svátek práce (a Friday): Monday 4 May.
    assert_eq!(
        due(&pack, f, "income_tax.return.electronic", "2025"),
        ("2026-05-01".into(), "2026-05-04".into())
    );
    // 1 April 2026 is a Wednesday: no shift.
    assert_eq!(
        due(&pack, f, "income_tax.return.paper", "2025"),
        ("2026-04-01".into(), "2026-04-01".into())
    );
    // 8 May is Den vítězství (a Friday): Monday 11 May.
    assert_eq!(
        due(&pack, f, "insurance.health.advance", "2026-04"),
        ("2026-05-08".into(), "2026-05-11".into())
    );
    // 20 December 2026 is a Sunday.
    assert_eq!(
        due(&pack, f, "insurance.social.advance", "2026-11"),
        ("2026-12-20".into(), "2026-12-21".into())
    );
    // Easter Monday is 6 April 2026; 8 April is an ordinary Wednesday.
    assert_eq!(
        due(&pack, f, "insurance.health.advance", "2026-03"),
        ("2026-04-08".into(), "2026-04-08".into())
    );
}

#[test]
fn the_year_holds_exactly_its_deadlines() {
    let pack = Pack::cz_2026().unwrap();
    let all = pack.calendar(2026, OSVC_MONTHLY_VAT).unwrap();
    let count = |id: &str| all.iter().filter(|d| d.obligation == id).count();
    // December 2025's return is due in January 2026; December 2026's in 2027.
    assert_eq!(count("vat.return.monthly"), 12);
    assert_eq!(
        all.iter()
            .find(|d| d.obligation == "vat.return.monthly")
            .unwrap()
            .period,
        "2025-12"
    );
    assert_eq!(count("insurance.social.advance"), 12);
    assert_eq!(count("income_tax.return.paper"), 1);
    assert!(
        all.windows(2).all(|w| w[0].due <= w[1].due),
        "sorted by date"
    );
    assert!(
        all.iter()
            .all(|d| d.due.starts_with("2026-") && pack.is_working_day(&d.due))
    );
    // A non-payer gets no VAT deadlines.
    let osvc = pack.calendar(2026, &["osvc"]).unwrap();
    assert!(osvc.iter().all(|d| !d.obligation.starts_with("vat.")));
    assert_eq!(osvc.len(), 26);
}

#[test]
fn a_broken_obligation_is_refused() {
    let pack = include_str!("../../../rules/cz/2026/pack.toml");
    let broken = pack.replacen(
        "due = { month_offset = 1, day = 20 }",
        "due = { month_offset = 1, day = 31 }",
        1,
    );
    let err = Pack::from_toml(&broken).unwrap_err().to_string();
    assert!(
        err.contains("obligation insurance.social.advance: month offset"),
        "{err}"
    );
    let broken = pack.replacen(
        "due = { days_after_period = \"vat.return.due_days_after_period\" }",
        "due = { days_after_period = \"vat.rate.standard\" }",
        1,
    );
    let err = Pack::from_toml(&broken).unwrap_err().to_string();
    assert!(
        err.contains("vat.rate.standard isn't a days value"),
        "{err}"
    );
}
