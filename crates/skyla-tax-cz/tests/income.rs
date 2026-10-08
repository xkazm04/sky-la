//! WP-22 acceptance: the § 7 worksheet and the scenario engine reproduce
//! hand-computed cases, side effects included.
//!
//! Hand workings (pack cz-2026, 2026 values):
//! - Actual expenses: 1 571 000 − 383 200 = 1 187 800 profit; tax base
//!   1 187 800 (already whole hundreds); 15 % = 178 170; − 30 840 credit =
//!   147 330. Social: 55 % = 653 290 base; × 29,2 % = 190 760,68 → 190 761.
//!   Health: 50 % = 593 900 base; × 13,5 % = 80 176,50 → 80 177. Total 418 268.
//! - Flat rate 60 %: 942 600 expenses (below the 1 200 000 cap); profit
//!   628 400; 15 % = 94 260; − 30 840 = 63 420. Social: 55 % = 345 620 base;
//!   × 29,2 % = 100 921,04 → 100 922. Health: 314 200 × 13,5 % = 42 417.
//!   Total 206 759.
//! - The difference: −83 910 tax, −89 839 social, −37 760 health,
//!   −211 509 in all, and a pension assessment base 307 670 lower.

#![allow(clippy::unwrap_used)]

use skyla_money::{Currency, Money};
use skyla_rules::Pack;
use skyla_tax_cz::{
    Expenses, FlatRate, PlannedPurchase, ScenarioFacts, Section7, scenarios, worksheet,
};

fn czk(crowns: i64) -> Money {
    Money::new(crowns * 100, Currency::CZK)
}

fn review_example() -> Section7 {
    Section7 {
        on: "2026-12-31".into(),
        income: czk(1_571_000),
        actual_expenses: czk(383_200),
    }
}

#[test]
fn actual_expenses() {
    let pack = Pack::cz_2026().unwrap();
    let w = worksheet(&pack, &review_example(), Expenses::Actual).unwrap();
    assert_eq!(
        (w.profit, w.tax_base, w.tax_before_credits, w.tax),
        (czk(1_187_800), czk(1_187_800), czk(178_170), czk(147_330))
    );
    assert_eq!(
        (w.social.assessment_base, w.social.amount),
        (czk(653_290), czk(190_761))
    );
    assert_eq!(
        (w.health.assessment_base, w.health.amount),
        (czk(593_900), czk(80_177))
    );
    assert_eq!(w.total, czk(418_268));
}

#[test]
fn the_review_flat_rate_example_reproduces_exactly() {
    let pack = Pack::cz_2026().unwrap();
    let ex = review_example();
    let w = worksheet(&pack, &ex, Expenses::FlatRate(FlatRate::Trade)).unwrap();
    assert_eq!(w.expenses, czk(942_600));
    assert!(!w.capped);
    assert_eq!((w.profit, w.tax), (czk(628_400), czk(63_420)));
    assert_eq!(
        (w.social.assessment_base, w.social.amount),
        (czk(345_620), czk(100_922))
    );
    assert_eq!(w.health.amount, czk(42_417));
    assert_eq!(w.total, czk(206_759));
    let actual = worksheet(&pack, &ex, Expenses::Actual).unwrap();
    assert_eq!(actual.expenses, czk(383_200));
    assert_eq!(
        w.total.checked_sub(actual.total).unwrap(),
        czk(-211_509),
        "−83 910 tax, −89 839 social, −37 760 health"
    );
    assert_eq!(
        actual
            .social
            .assessment_base
            .checked_sub(w.social.assessment_base)
            .unwrap(),
        czk(307_670),
        "the pension assessment base is lower by this much"
    );
}

#[test]
fn the_cap_limits_the_deduction_not_the_income() {
    let pack = Pack::cz_2026().unwrap();
    let s7 = Section7 {
        on: "2026-12-31".into(),
        income: czk(2_500_000),
        actual_expenses: czk(0),
    };
    let w = worksheet(&pack, &s7, Expenses::FlatRate(FlatRate::Trade)).unwrap();
    // 60 % of 2 500 000 would be 1 500 000; the cap is 1 200 000.
    assert!(w.capped);
    assert_eq!((w.expenses, w.profit), (czk(1_200_000), czk(1_300_000)));
}

#[test]
fn bases_round_down_to_hundreds_and_tax_up_to_crowns() {
    let pack = Pack::cz_2026().unwrap();
    let s7 = Section7 {
        on: "2026-12-31".into(),
        income: Money::new(50_012_345, Currency::CZK),
        actual_expenses: Money::new(10_000_000, Currency::CZK),
    };
    let w = worksheet(&pack, &s7, Expenses::Actual).unwrap();
    // Profit 400 123,45 → base 400 100; 15 % = 60 015; − 30 840 = 29 175.
    assert_eq!((w.tax_base, w.tax), (czk(400_100), czk(29_175)));
    // Social: 55 % of 400 123,45 = 220 067,9475 → 220 068; × 29,2 % =
    // 64 259,856 → 64 260. Health: 50 % = 200 061,725 → 200 062.
    assert_eq!(
        (w.social.assessment_base, w.social.amount),
        (czk(220_068), czk(64_260))
    );
}

#[test]
fn a_loss_owes_no_tax_and_the_credit_is_not_refunded() {
    let pack = Pack::cz_2026().unwrap();
    let loss = Section7 {
        on: "2026-12-31".into(),
        income: czk(100_000),
        actual_expenses: czk(150_000),
    };
    let w = worksheet(&pack, &loss, Expenses::Actual).unwrap();
    assert_eq!(
        (w.profit, w.tax_base, w.tax),
        (czk(-50_000), czk(0), czk(0))
    );
    assert_eq!((w.social.amount, w.health.amount), (czk(0), czk(0)));
    let small = Section7 {
        income: czk(250_000),
        ..loss
    };
    let w = worksheet(&pack, &small, Expenses::FlatRate(FlatRate::Trade)).unwrap();
    // 100 000 base; 15 000 tax; the 30 840 credit only reaches 15 000.
    assert_eq!((w.taxpayer_credit, w.tax), (czk(15_000), czk(0)));
}

#[test]
fn the_engine_compares_levers_with_side_effects() {
    let pack = Pack::cz_2026().unwrap();
    let a = scenarios(
        &pack,
        &ScenarioFacts {
            section7: review_example(),
            flat_rate: Some(FlatRate::Trade),
            planned_purchase: Some(PlannedPurchase {
                description: "Laptop".into(),
                price: czk(60_000),
            }),
        },
    )
    .unwrap();
    let ids: Vec<_> = a.scenarios.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "actual+purchase.this_year",
            "actual+purchase.next_year",
            "flat_rate.trade+purchase.this_year",
            "flat_rate.trade+purchase.next_year",
        ]
    );
    assert_eq!(a.baseline, "actual+purchase.this_year");
    // Under the flat rate the purchase changes nothing this year.
    let flat = &a.scenarios[2].worksheet;
    assert_eq!(flat.total, a.scenarios[3].worksheet.total);
    assert_eq!(flat.total, czk(206_759));
    assert_eq!(a.lowest_total, "flat_rate.trade+purchase.this_year");
    // Buying next year under actual expenses: 60 000 more profit this year.
    // Tax +9 000; social: 190 761 against 181 125 (620 290 × 29,2 % =
    // 181 124,68) = +9 636; health 30 000 × 13,5 % = +4 050.
    let next = &a.differences[1];
    assert_eq!(
        (next.tax, next.social, next.health),
        (czk(9_000), czk(9_636), czk(4_050))
    );
    // The flat rate against actual expenses with the laptop this year
    // (443 200 actual, 1 127 800 profit, 620 290 pension base): 274 670 lower.
    assert_eq!(a.differences[2].pension_base, czk(-274_670));
    assert!(a.questions.is_empty());
    assert!(a.not_evaluated.iter().any(|n| n.contains("Paušální daň")));
}

#[test]
fn a_purchase_above_the_threshold_is_not_guessed() {
    let pack = Pack::cz_2026().unwrap();
    let a = scenarios(
        &pack,
        &ScenarioFacts {
            section7: review_example(),
            flat_rate: None,
            planned_purchase: Some(PlannedPurchase {
                description: "Car".into(),
                price: czk(450_000),
            }),
        },
    )
    .unwrap();
    assert_eq!(a.scenarios.len(), 1, "only actual expenses, no timing");
    assert!(
        a.not_evaluated
            .iter()
            .any(|n| n.starts_with("Car: above the 80\u{a0}000,00\u{a0}Kč")),
        "{:?}",
        a.not_evaluated
    );
    assert_eq!(a.questions.len(), 1, "asks for the flat-rate group");
}
