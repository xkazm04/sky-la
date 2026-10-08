//! The souhrnné hlášení from the books: supplies to VAT payers in other
//! member states are issued with the pack's EU-supply codes, feed ř. 20 and
//! 21 of the DPH return without touching its tax, stay out of the
//! kontrolní hlášení, and make up the statement per customer and code.
//!
//! Hand workings (October 2026, demo books):
//! - Services: 40 h × 1 500,00 = 60 000,00 Kč (6 000 000 haléřů), code 3.
//! - Goods: 2 ks × 25 000,00 = 50 000,00 Kč (5 000 000 haléřů), code 0.
//! - Total of the statement: 11 000 000. No VAT either way, so the return's
//!   output tax, input tax and payable amount don't move.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::{ClientDto, InvoiceDraftDto, InvoiceDraftLineDto};

fn to_germany(code: &str, quantity: &str, unit: &str, price: &str, known: bool) -> InvoiceDraftDto {
    InvoiceDraftDto {
        client: if known {
            "Beispiel GmbH".into()
        } else {
            String::new()
        },
        new_client: (!known).then(|| ClientDto {
            name: "Beispiel GmbH".into(),
            ico: None,
            // Typed with spaces and in lower case; the core keeps it tidy.
            dic: Some("de 123 456 789".into()),
            address: Some("Hauptstraße 5, 10115 Berlin, Německo".into()),
        }),
        due_days: 14,
        note: String::new(),
        lines: vec![InvoiceDraftLineDto {
            description: "Konzultace".into(),
            quantity: quantity.into(),
            unit: unit.into(),
            unit_price: price.into(),
            vat_code: code.into(),
        }],
    }
}

#[test]
fn the_statement_comes_from_the_issued_documents() {
    let core = Core::demo().unwrap();
    let (from, to) = ("2026-10-01", "2026-10-31");
    let return_before = core.vat_return(from, to).unwrap();
    let kh_before = core.control_statement(from, to).unwrap();
    assert!(
        core.recapitulative_statement(from, to)
            .unwrap()
            .lines
            .is_empty()
    );

    // The editor offers the codes to a VAT payer.
    let offered: Vec<_> = core
        .invoice_form()
        .unwrap()
        .vat_codes
        .into_iter()
        .filter(|c| c.code.starts_with("EU"))
        .map(|c| (c.code, c.rate_percent))
        .collect();
    assert_eq!(
        offered,
        [
            ("EUSVC".to_owned(), "0".to_owned()),
            ("EUGDS".to_owned(), "0".to_owned())
        ]
    );

    let service = core
        .create_invoice_draft(&to_germany("EUSVC", "40", "h", "1 500,00", false))
        .unwrap();
    assert_eq!(service.base.minor, 6_000_000);
    assert_eq!(service.vat.minor, 0);
    core.issue_invoice(service.id, "2026-10-07").unwrap();
    let goods = core
        .create_invoice_draft(&to_germany("EUGDS", "2", "ks", "25 000,00", true))
        .unwrap();
    core.issue_invoice(goods.id, "2026-10-07").unwrap();

    let st = core.recapitulative_statement(from, to).unwrap();
    let lines: Vec<_> = st
        .lines
        .iter()
        .map(|l| {
            (
                l.country.as_str(),
                l.vat_number.as_str(),
                l.sh_code.as_str(),
                l.base.minor,
                l.supplies,
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("DE", "123456789", "0", 5_000_000, 1),
            ("DE", "123456789", "3", 6_000_000, 1)
        ]
    );
    assert_eq!(st.total.minor, 11_000_000);
    assert!(st.problems.is_empty(), "{:?}", st.problems);
    assert_eq!(st.pack, "cz-2026@2026.1");

    // The return: the bases on ř. 20 and 21, no tax, nothing else moved.
    let ret = core.vat_return(from, to).unwrap();
    let row = |r: &str| {
        ret.rows
            .iter()
            .find(|x| x.row == r)
            .map(|x| (x.base.minor, x.tax.minor, x.side.clone()))
    };
    assert_eq!(row("20"), Some((5_000_000, 0, "output".into())));
    assert_eq!(row("21"), Some((6_000_000, 0, "output".into())));
    assert_eq!(
        (ret.output_tax, ret.input_tax, ret.payable),
        (
            return_before.output_tax,
            return_before.input_tax,
            return_before.payable
        )
    );
    assert!(ret.unmapped.is_empty());

    // The kontrolní hlášení doesn't list them (domestic supplies only).
    let kh = core.control_statement(from, to).unwrap();
    assert_eq!(kh, kh_before);

    // Another month has none.
    assert!(
        core.recapitulative_statement("2026-09-01", "2026-09-30")
            .unwrap()
            .lines
            .is_empty()
    );
    assert!(core.recapitulative_statement("2026-10-31", "oops").is_err());
}

#[test]
fn a_czech_customer_cant_be_sent_an_eu_supply() {
    let core = Core::demo().unwrap();
    let mut draft = to_germany("EUSVC", "1", "h", "1 000,00", true);
    draft.client = "Northwind Traders s.r.o.".into();
    draft.new_client = None;
    let saved = core.create_invoice_draft(&draft).unwrap();
    let refused = core
        .issue_invoice(saved.id, "2026-10-07")
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("EUSVC is for customers with a VAT number in another member state"),
        "{refused}"
    );
}
