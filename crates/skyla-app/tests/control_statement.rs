//! WP-21 acceptance (the core half): the kontrolní hlášení is built from the
//! books, itemises above the pack's threshold, and its section C agrees
//! with the DPH return for the same months.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;

#[test]
fn september_from_the_demo_books() {
    let core = Core::demo().unwrap();
    let kh = core.control_statement("2026-09-01", "2026-09-30").unwrap();
    assert_eq!(kh.threshold.minor, 1_000_000);
    let numbers = |items: &[skyla_app::dto::KhItemDto]| {
        items.iter().map(|i| i.number.clone()).collect::<Vec<_>>()
    };
    // The AWS service under reverse charge, once, with its tax.
    assert_eq!(numbers(&kh.a2), ["AWS-2026-09"]);
    assert_eq!(
        (kh.a2[0].base_standard.minor, kh.a2[0].tax_standard.minor),
        (1_231_860, 258_691)
    );
    assert_eq!(numbers(&kh.a4), ["2026-114"]);
    assert_eq!(kh.a4[0].vat_id, "CZ91341272");
    assert_eq!(numbers(&kh.b2), ["PF-2026-0917"]);
    // The cables receipt (3 004,43 Kč) is under the threshold: summed in B.3.
    assert_eq!((kh.b3.documents, kh.b3.base_standard.minor), (1, 248_300));
    assert!(kh.problems.is_empty(), "{:?}", kh.problems);
    assert!(kh.matches_return);
    let ret = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    for row in &kh.c {
        let r = ret.rows.iter().find(|r| r.row == row.row).unwrap();
        assert_eq!(r.base, row.base, "ř. {}", row.row);
    }
    assert_eq!(kh.due_on, "2026-10-26", "the 25th falls on a Sunday");
}

#[test]
fn every_month_of_the_quarter_agrees_with_its_return() {
    let core = Core::demo().unwrap();
    for (from, to) in [
        ("2026-07-01", "2026-07-31"),
        ("2026-08-01", "2026-08-31"),
        ("2026-09-01", "2026-09-30"),
    ] {
        let kh = core.control_statement(from, to).unwrap();
        assert!(
            kh.matches_return && kh.problems.is_empty(),
            "{from}: {kh:?}"
        );
    }
}
