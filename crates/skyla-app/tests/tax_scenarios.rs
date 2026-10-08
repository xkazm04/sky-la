//! WP-22 acceptance (the core half): the scenarios come from the books or
//! from a projection the core parses, and every figure is the engine's.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::TaxProjectionDto;

fn projection() -> TaxProjectionDto {
    TaxProjectionDto {
        income: "1 571 000".into(),
        expenses: "383 200".into(),
        flat_rate: Some("trade".into()),
        purchase_description: "Laptop".into(),
        purchase_price: "60 000".into(),
    }
}

#[test]
fn the_review_projection_reproduces_in_the_core() {
    let core = Core::demo().unwrap();
    let t = core.income_tax_scenarios(Some(&projection())).unwrap();
    assert_eq!(t.source, "projection");
    let flat = t
        .scenarios
        .iter()
        .find(|s| s.id == "flat_rate.trade+purchase.next_year")
        .unwrap();
    assert_eq!(flat.expenses.minor, 94_260_000);
    assert_eq!(flat.flat_rate_percent.as_deref(), Some("60"));
    assert_eq!((flat.tax.minor, flat.total.minor), (6_342_000, 19_758_400));
    let actual = t
        .scenarios
        .iter()
        .find(|s| s.id == "actual+purchase.next_year")
        .unwrap();
    assert_eq!(actual.expenses.minor, 38_320_000);
    assert_eq!(actual.label, "Actual expenses · Laptop bought next year");
    assert_eq!(t.lowest_total, "flat_rate.trade+purchase.this_year");
}

#[test]
fn from_the_books_without_a_projection() {
    let core = Core::demo().unwrap();
    let t = core.income_tax_scenarios(None).unwrap();
    let cb = core.cash_basis("2026-01-01", "2026-10-07").unwrap();
    assert_eq!(t.source, "books");
    assert_eq!(t.income, cb.taxable_income);
    assert_eq!(t.actual_expenses, cb.deductible_expenses);
    assert_eq!(t.flat_rate.as_deref(), Some("trade"));
    assert_eq!(t.scenarios.len(), 2);
    assert!(t.assumptions[0].starts_with("From the books, 2026-01-01 to 2026-10-07"));
}

#[test]
fn a_bad_projection_is_refused_with_the_reason() {
    let core = Core::demo().unwrap();
    let mut p = projection();
    p.income = "1.571.000".into();
    let err = core.income_tax_scenarios(Some(&p)).unwrap_err().to_string();
    assert!(
        err.contains("income \"1.571.000\" isn't an amount"),
        "{err}"
    );
    let mut p = projection();
    p.flat_rate = Some("lawyer".into());
    assert!(core.income_tax_scenarios(Some(&p)).is_err());
}

#[test]
fn the_calendar_marks_the_next_deadline_from_the_as_of_date() {
    let core = Core::demo().unwrap();
    let all = core.obligations(2026).unwrap();
    // As of 7 October: the health advance for September was due on the 8th.
    let next: Vec<_> = all.iter().filter(|o| o.status == "next").collect();
    assert_eq!(next.len(), 1);
    assert_eq!(
        (next[0].obligation.as_str(), next[0].due.as_str()),
        ("insurance.health.advance", "2026-10-08")
    );
    let vat = all
        .iter()
        .find(|o| o.obligation == "vat.return.monthly" && o.period == "2026-09")
        .unwrap();
    assert!(vat.shifted && vat.due == "2026-10-26" && vat.status == "upcoming");
    assert!(vat.citation.starts_with("Zákon č. 235/2004 Sb."));
    assert!(
        all.iter()
            .filter(|o| o.due.as_str() < "2026-10-07")
            .all(|o| o.status == "past")
    );
}
