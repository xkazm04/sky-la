//! The souhrnné hlášení (EC Sales List) reproduces hand-computed cases.
//!
//! The pack names the EU supply codes and their statement codes: EUSVC is
//! code 3 (services), EUGDS code 0 (goods). Amounts below are in haléřů.
//!
//! September 2026, by customer and code:
//! - DE123456789, code 3: 2026-001 60 000,00 (6 000 000) + 2026-002
//!   10 000,00 (1 000 000) = 7 000 000, less the credit note OD-1 of
//!   15 000,00 (−1 500 000) that corrects 2026-001 (dated October, but it
//!   follows its original into September) = 5 500 000; two supplies, since a
//!   correction isn't one.
//! - DE123456789, code 0: 2026-002's goods, 50 000,00 = 5 000 000; one supply.
//! - AT U12345678, code 3: 2026-003, 25 000,00 = 2 500 000, typed as
//!   `atu 12345678`.
//! - FR12345678901: 2026-004 30 000,00 (3 000 000) is credited in full by
//!   OD-2 (−3 000 000), so the customer nets to nothing and drops out.
//! - Not in the statement: a domestic OUT21 sale; 2026-005 (an EU supply
//!   dated 2 October); OD-3, a credit note dated 25 September that corrects
//!   an August invoice (it belongs to August).
//! - Problems: 2026-006 has no customer VAT number; 2026-007 names a Czech one.
//!
//! Total of the lines: 5 500 000 + 5 000 000 + 2 500 000 = 13 000 000.
//!
//! August 2026: 2026-000 20 000,00 (2 000 000) to DE123456789, less OD-3's
//! 5 000,00 (−500 000) = 1 500 000, one supply.

#![allow(clippy::unwrap_used)]

use skyla_money::{Currency, Money};
use skyla_rules::Pack;
use skyla_tax_cz::{ShDocument, ShError, ShPart, recapitulative_statement};

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn doc(
    number: &str,
    vat_id: Option<&str>,
    date: &str,
    corrects: Option<&str>,
    parts: &[(&str, i64)],
) -> ShDocument {
    ShDocument {
        number: number.into(),
        counterparty: format!("Customer of {number}"),
        vat_id: vat_id.map(Into::into),
        date: date.into(),
        corrects_date: corrects.map(Into::into),
        parts: parts
            .iter()
            .map(|(code, base)| ShPart {
                vat_code: (*code).into(),
                base: czk(*base),
            })
            .collect(),
    }
}

fn books() -> Vec<ShDocument> {
    vec![
        doc(
            "2026-000",
            Some("DE123456789"),
            "2026-08-30",
            None,
            &[("EUSVC", 2_000_000)],
        ),
        doc(
            "2026-001",
            Some("DE123456789"),
            "2026-09-15",
            None,
            &[("EUSVC", 6_000_000)],
        ),
        doc(
            "2026-002",
            Some("DE123456789"),
            "2026-09-20",
            None,
            &[
                ("EUSVC", 1_000_000),
                ("EUGDS", 5_000_000),
                ("OUT21", 400_000),
            ],
        ),
        doc(
            "2026-003",
            Some("atu 12345678"),
            "2026-09-22",
            None,
            &[("EUSVC", 2_500_000)],
        ),
        doc(
            "2026-004",
            Some("FR12345678901"),
            "2026-09-10",
            None,
            &[("EUSVC", 3_000_000)],
        ),
        doc(
            "2026-domestic",
            Some("CZ91341272"),
            "2026-09-12",
            None,
            &[("OUT21", 1_000_000)],
        ),
        doc(
            "2026-005",
            Some("DE123456789"),
            "2026-10-02",
            None,
            &[("EUSVC", 900_000)],
        ),
        doc("2026-006", None, "2026-09-25", None, &[("EUSVC", 100_000)]),
        doc(
            "2026-007",
            Some("CZ91341272"),
            "2026-09-26",
            None,
            &[("EUGDS", 100_000)],
        ),
        // Credit notes, dated after what they correct.
        doc(
            "OD-1",
            Some("DE123456789"),
            "2026-10-05",
            Some("2026-09-15"),
            &[("EUSVC", -1_500_000)],
        ),
        doc(
            "OD-2",
            Some("FR12345678901"),
            "2026-09-12",
            Some("2026-09-10"),
            &[("EUSVC", -3_000_000)],
        ),
        doc(
            "OD-3",
            Some("DE123456789"),
            "2026-09-25",
            Some("2026-08-30"),
            &[("EUSVC", -500_000)],
        ),
    ]
}

#[test]
fn september_by_customer_and_code() {
    let pack = Pack::cz_2026().unwrap();
    let st = recapitulative_statement(&pack, "2026-09-01", "2026-09-30", Currency::CZK, &books())
        .unwrap();
    let lines: Vec<_> = st
        .lines
        .iter()
        .map(|l| {
            (
                l.country.as_str(),
                l.vat_number.as_str(),
                l.sh_code.as_str(),
                l.base.minor(),
                l.supplies,
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("AT", "U12345678", "3", 2_500_000, 1),
            ("DE", "123456789", "0", 5_000_000, 1),
            ("DE", "123456789", "3", 5_500_000, 2),
        ]
    );
    assert_eq!(st.total, czk(13_000_000));
    assert_eq!(
        st.problems,
        [
            "2026-006: the souhrnné hlášení needs the customer's VAT number in another member state",
            "2026-007: the souhrnné hlášení needs the customer's VAT number in another member state",
        ]
    );
    assert_eq!(
        (st.from.as_str(), st.to.as_str()),
        ("2026-09-01", "2026-09-30")
    );
}

#[test]
fn a_correction_follows_its_original_into_the_original_period() {
    let pack = Pack::cz_2026().unwrap();
    let aug = recapitulative_statement(&pack, "2026-08-01", "2026-08-31", Currency::CZK, &books())
        .unwrap();
    assert_eq!(aug.lines.len(), 1);
    // 2 000 000 − 500 000 = 1 500 000; the credit note isn't a second supply.
    assert_eq!(
        (
            aug.lines[0].base,
            aug.lines[0].supplies,
            aug.lines[0].sh_code.as_str()
        ),
        (czk(1_500_000), 1, "3")
    );
    assert!(aug.problems.is_empty());

    // October has the 2 October supply, and none of the corrections of September's.
    let oct = recapitulative_statement(&pack, "2026-10-01", "2026-10-31", Currency::CZK, &books())
        .unwrap();
    assert_eq!(oct.lines.len(), 1);
    assert_eq!(oct.lines[0].base, czk(900_000));
}

#[test]
fn a_code_the_pack_doesnt_know_is_an_error() {
    let pack = Pack::cz_2026().unwrap();
    let docs = [doc(
        "X",
        Some("DE123456789"),
        "2026-09-01",
        None,
        &[("NOPE", 100)],
    )];
    assert!(matches!(
        recapitulative_statement(&pack, "2026-09-01", "2026-09-30", Currency::CZK, &docs),
        Err(ShError::Rules(_))
    ));
}

#[test]
fn an_empty_period_is_an_empty_statement() {
    let pack = Pack::cz_2026().unwrap();
    let st = recapitulative_statement(&pack, "2026-01-01", "2026-01-31", Currency::CZK, &books())
        .unwrap();
    assert!(st.lines.is_empty() && st.problems.is_empty());
    assert_eq!(st.total, czk(0));
}
