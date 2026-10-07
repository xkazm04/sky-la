//! WP-15 acceptance: statutory late interest matches hand-computed cases.
//!
//! Each case was worked out by hand as principal × annual rate × days / 365,
//! days counted from the day after the due date through the payment day,
//! rounded to the haléř per period. The repo rates below are test inputs,
//! not ČNB history.

use skyla_invoicing::{RepoRate, late_interest};
use skyla_money::{Currency, Money, Rate};
use skyla_rules::Pack;

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

/// (annual rate, [(days, interest)] per period, total), in minor units.
type Outcome = (String, Vec<(i64, i64)>, i64);

fn rates() -> Vec<RepoRate> {
    [
        ("2024-12-20", "4.00"),
        ("2025-05-02", "3.50"),
        ("2026-09-01", "3.25"),
    ]
    .into_iter()
    .map(|(d, r)| RepoRate {
        effective_from: d.into(),
        rate: r.parse::<Rate>().expect("rate"),
    })
    .collect()
}

fn interest(principal: i64, due: &str, until: &str, paid: &[(&str, i64)]) -> Option<Outcome> {
    let pack = Pack::cz_2026().expect("pack");
    let paid: Vec<(String, Money)> = paid
        .iter()
        .map(|(d, m)| ((*d).to_owned(), czk(*m)))
        .collect();
    late_interest(&pack, &rates(), czk(principal), due, until, &paid)
        .expect("interest")
        .map(|i| {
            (
                i.annual_rate.normalize().to_string(),
                i.periods
                    .iter()
                    .map(|p| (p.days, p.interest.minor()))
                    .collect(),
                i.total.minor(),
            )
        })
}

#[test]
fn a_month_late_in_full() {
    // 100 000 × 11.5 % × 30 / 365 = 945.2054… → 945,21. Delay began in
    // April 2026: repo on 1 January 2026 (3.50) + 8.
    assert_eq!(
        interest(
            10_000_000,
            "2026-03-31",
            "2026-04-30",
            &[("2026-04-30", 10_000_000)]
        ),
        Some(("11.5".into(), vec![(30, 94_521)], 94_521))
    );
}

#[test]
fn a_partial_payment_splits_the_period() {
    // 100 000 × 11.5 % × 15 / 365 = 472.6027… → 472,60;
    // 50 000 × 11.5 % × 15 / 365 = 236.3013… → 236,30.
    assert_eq!(
        interest(
            10_000_000,
            "2026-03-31",
            "2026-05-31",
            &[("2026-04-15", 5_000_000), ("2026-04-30", 5_000_000)]
        ),
        Some(("11.5".into(), vec![(15, 47_260), (15, 23_630)], 70_890))
    );
}

#[test]
fn the_rate_is_fixed_by_the_half_year_the_delay_began() {
    // Due 29 June: the delay begins 30 June, in H1 2025, so the repo rate of
    // 1 January 2025 (4.00) applies for the whole delay, even though it fell
    // to 3.50 in May. 20 000 × 12 % × 47 / 365 = 309.0410… → 309,04.
    assert_eq!(
        interest(
            2_000_000,
            "2025-06-29",
            "2025-08-15",
            &[("2025-08-15", 2_000_000)]
        ),
        Some(("12".into(), vec![(47, 30_904)], 30_904))
    );
    // A day later the delay begins 1 July: 3.50 + 8.
    // 20 000 × 11.5 % × 46 / 365 = 289.8630… → 289,86.
    assert_eq!(
        interest(
            2_000_000,
            "2025-06-30",
            "2025-08-15",
            &[("2025-08-15", 2_000_000)]
        ),
        Some(("11.5".into(), vec![(46, 28_986)], 28_986))
    );
}

#[test]
fn unpaid_across_the_year_end() {
    // Delay from 1 December 2026 (H2: repo on 1 July 2026 is still 3.50; the
    // September cut doesn't count) through 31 January 2027, 62 days.
    // 80 000 × 11.5 % × 62 / 365 = 1 562.7397… → 1 562,74.
    assert_eq!(
        interest(8_000_000, "2026-11-30", "2027-01-31", &[]),
        Some(("11.5".into(), vec![(62, 156_274)], 156_274))
    );
}

#[test]
fn what_was_paid_on_time_does_not_accrue() {
    // 40 000 paid on the due date; 60 000 × 11.5 % × 10 / 365 = 189.0410… → 189,04.
    assert_eq!(
        interest(
            10_000_000,
            "2026-03-31",
            "2026-04-10",
            &[("2026-03-31", 4_000_000), ("2026-04-10", 6_000_000)]
        ),
        Some(("11.5".into(), vec![(10, 18_904)], 18_904))
    );
    assert_eq!(
        interest(
            10_000_000,
            "2026-03-31",
            "2026-06-30",
            &[("2026-03-20", 10_000_000)]
        ),
        None
    );
    assert_eq!(interest(10_000_000, "2026-03-31", "2026-03-31", &[]), None);
}

#[test]
fn names_the_missing_reference_rate_and_the_recovery_cost() {
    let pack = Pack::cz_2026().expect("pack");
    let err =
        late_interest(&pack, &[], czk(100), "2026-03-31", "2026-04-30", &[]).expect_err("no rates");
    assert!(
        err.to_string().contains("no ČNB repo rate for 2026-01-01"),
        "{err}"
    );
    let i = late_interest(&pack, &rates(), czk(100), "2026-03-31", "2026-04-30", &[])
        .expect("ok")
        .expect("late");
    assert_eq!(i.recovery_cost, czk(120_000));
    assert_eq!(
        (i.rate_date.as_str(), i.delay_from.as_str()),
        ("2026-01-01", "2026-04-01")
    );
}
