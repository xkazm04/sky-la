//! Supplies to VAT payers in other member states: the pack's EU-supply VAT
//! codes (EUSVC for services, EUGDS for goods) issue only for a VAT-registered
//! supplier and a customer with another member state's VAT number, post no
//! VAT, print the reverse-charge or exemption wording and go out as EN 16931
//! category AE (services) or K (goods).
//!
//! Hand workings (pack cz-2026): no Czech VAT, so base = gross.
//! - Services: 40 h × 1 500,00 = 60 000,00 Kč, so 6 000 000 haléřů on
//!   receivables (311) and on revenue (602), no line on 343.
//! - Goods: 2 ks × 25 000,00 = 50 000,00 Kč, so 5 000 000 haléřů.

mod common;

use common::{Books, books, draft, line};
use skyla_invoicing::{
    Customer, DocKind, Document, DraftInput, ExportInput, InvoicingError, create_draft, eu_vat_id,
    get, issue, next_number, to_cii, to_isdoc, to_ubl,
};

fn german(dic: Option<&str>) -> Customer {
    Customer {
        name: "Beispiel GmbH".into(),
        ico: None,
        dic: dic.map(str::to_owned),
        // No country named: it comes from the VAT number's prefix.
        address: Some("Hauptstraße 5\n10115 Berlin".into()),
    }
}

fn to_germany(kind: DocKind, series: &str, dic: Option<&str>, code: &str) -> DraftInput {
    DraftInput {
        customer: german(dic),
        ..draft(
            kind,
            series,
            vec![line("Konzultace", "40", "h", 150_000, code)],
        )
    }
}

fn issue_to_germany(b: &Books, input: &DraftInput) -> Result<Document, InvoicingError> {
    let id = create_draft(&b.conn, input)?;
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None)?;
    get(&b.conn, &b.pack, id)
}

fn input(doc: &Document) -> ExportInput<'_> {
    ExportInput {
        doc,
        related: None,
        advances: &[],
    }
}

#[test]
fn another_member_states_vat_number_has_its_shape() {
    for ok in [
        "DE123456789",
        "de 123 456 789",
        "FR12345678901",
        "FRAB123456789",
        "ATU12345678",
        "NL123456789B01",
        "EL123456789",
        "SK2020317068",
        "PL1234567890",
        "IE1234567WA",
        "RO12",
    ] {
        assert!(eu_vat_id(ok).is_some(), "{ok}");
    }
    for bad in [
        "CZ12345678",   // domestic, not a supply to another member state
        "GB123456789",  // not a member state
        "DE12345678",   // one digit short
        "DE1234567890", // one too many
        "DEA23456789",  // letters where Germany has digits
        "XX123456789",
        "123456789",
        "DE",
        "",
    ] {
        assert!(eu_vat_id(bad).is_none(), "{bad}");
    }
    let id = eu_vat_id("EL 123456789").unwrap();
    assert_eq!(
        (id.country.as_str(), id.number.as_str(), id.iso_country()),
        ("EL", "123456789", "GR")
    );
    assert_eq!(id.full(), "EL123456789");
}

#[test]
fn a_service_to_a_german_company_posts_no_vat_and_goes_out_as_reverse_charge() {
    let b = books(true);
    let doc = issue_to_germany(
        &b,
        &to_germany(DocKind::Invoice, "FV", Some("DE123456789"), "EUSVC"),
    )
    .unwrap();

    // 40 h × 1 500,00 = 60 000,00; no VAT.
    assert_eq!(doc.totals.base.minor(), 6_000_000);
    assert_eq!(doc.totals.vat.minor(), 0);
    assert_eq!(doc.totals.gross.minor(), 6_000_000);
    assert_eq!(doc.totals.recap[0].rate, "0");
    let entry = skyla_ledger::get_entry(&b.conn, doc.entry_id.unwrap()).unwrap();
    let posted: Vec<_> = entry
        .lines
        .iter()
        .map(|l| {
            (
                l.account.as_str(),
                l.functional.minor(),
                l.vat_code.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        posted,
        [("311", 6_000_000, None), ("602", -6_000_000, Some("EUSVC"))]
    );

    // The pack's wording, in both languages.
    assert_eq!(doc.tax_notes.len(), 1);
    assert!(doc.tax_notes[0].cs.contains("Daň odvede zákazník"));
    assert!(doc.tax_notes[0].en.contains("Reverse charge"));

    // UBL: category AE, rate 0, the exemption text, both VAT numbers, Germany
    // as the customer's country, and no delivery block (that's for goods).
    let ubl = to_ubl(&b.pack, input(&doc)).unwrap();
    assert!(ubl.contains("<cbc:ID>AE</cbc:ID>\n"), "{ubl}");
    assert!(ubl.contains("<cbc:Percent>0</cbc:Percent>"));
    assert!(ubl.contains("čl. 196 směrnice 2006/112/ES"));
    assert!(ubl.contains(r#"<cbc:EndpointID schemeID="9930">DE123456789</cbc:EndpointID>"#));
    assert!(ubl.contains("<cbc:CompanyID>DE123456789</cbc:CompanyID>"));
    assert!(ubl.contains("<cbc:CompanyID>CZ92588034</cbc:CompanyID>"));
    assert!(ubl.contains("<cbc:IdentificationCode>DE</cbc:IdentificationCode>"));
    assert!(!ubl.contains("cac:Delivery"));
    assert!(ubl.contains(r#"<cbc:TaxAmount currencyID="CZK">0.00</cbc:TaxAmount>"#));

    let cii = to_cii(&b.pack, input(&doc)).unwrap();
    assert!(cii.contains("<ram:CategoryCode>AE</ram:CategoryCode>"));
    assert!(cii.contains("<ram:ID schemeID=\"VA\">DE123456789</ram:ID>"));
    assert!(cii.contains("<ram:CountryID>DE</ram:CountryID>"));
    assert!(!cii.contains("ShipToTradeParty"));
    assert!(cii.contains("<ram:DuePayableAmount>60000.00</ram:DuePayableAmount>"));

    // ISDOC has nowhere for the reason, so the statement goes in the note.
    let isdoc = to_isdoc(&b.pack, input(&doc)).unwrap();
    assert!(isdoc.contains("Daň odvede zákazník"), "{isdoc}");
    assert!(isdoc.contains("<VATApplicable>false</VATApplicable>"));
    assert!(isdoc.contains("<IdentificationCode>DE</IdentificationCode>"));
}

#[test]
fn goods_to_a_german_company_are_an_intra_community_supply() {
    let b = books(true);
    let mut input_doc = to_germany(DocKind::Invoice, "FV", Some("DE123456789"), "EUGDS");
    input_doc.lines = vec![line("Notebook", "2", "ks", 2_500_000, "EUGDS")];
    let doc = issue_to_germany(&b, &input_doc).unwrap();
    // 2 ks × 25 000,00 = 50 000,00.
    assert_eq!(doc.totals.gross.minor(), 5_000_000);
    assert!(doc.tax_notes[0].cs.contains("§ 64"));

    let ubl = to_ubl(&b.pack, input(&doc)).unwrap();
    assert!(ubl.contains("<cbc:ID>K</cbc:ID>\n"), "{ubl}");
    assert!(ubl.contains("čl. 138 směrnice 2006/112/ES"));
    // BR-IC-11 and BR-IC-12: the delivery date and the country of delivery.
    assert!(
        ubl.contains(
            "<cac:Delivery>\n    <cbc:ActualDeliveryDate>2026-09-15</cbc:ActualDeliveryDate>\n    <cac:DeliveryLocation>\n      <cac:Address>\n        <cac:Country>\n          <cbc:IdentificationCode>DE</cbc:IdentificationCode>"
        ),
        "{ubl}"
    );
    let cii = to_cii(&b.pack, input(&doc)).unwrap();
    assert!(cii.contains("<ram:CategoryCode>K</ram:CategoryCode>"));
    assert!(cii.contains(
        "<ram:ShipToTradeParty>\n        <ram:PostalTradeAddress>\n          <ram:CountryID>DE</ram:CountryID>"
    ), "{cii}");
    assert!(cii.contains("<ram:ActualDeliverySupplyChainEvent>"));
}

#[test]
fn issuing_refuses_the_codes_without_a_member_states_vat_number_or_a_vat_registration() {
    let b = books(true);
    let refuse = |input: &DraftInput| match issue_to_germany(&b, input) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("issued"),
    };
    // A Czech DIČ is a domestic customer.
    let e = refuse(&to_germany(
        DocKind::Invoice,
        "FV",
        Some("CZ91341272"),
        "EUSVC",
    ));
    assert!(
        e.contains("EUSVC is for customers with a VAT number in another member state")
            && e.contains("CZ91341272 isn't one"),
        "{e}"
    );
    // Malformed, missing, and blank.
    assert!(
        refuse(&to_germany(
            DocKind::Invoice,
            "FV",
            Some("DE12345"),
            "EUGDS"
        ))
        .contains("DE12345 isn't one")
    );
    assert!(
        refuse(&to_germany(DocKind::Invoice, "FV", None, "EUSVC"))
            .contains("the customer has none")
    );
    assert!(
        refuse(&to_germany(DocKind::Invoice, "FV", Some("  "), "EUSVC"))
            .contains("the customer has none")
    );
    // Advance documents don't take them (yet).
    let e = refuse(&to_germany(
        DocKind::Advance,
        "ZF",
        Some("DE123456789"),
        "EUSVC",
    ));
    assert!(e.contains("advance documents can't carry EUSVC"), "{e}");
    // Nothing was issued, so the numbering didn't move.
    assert_eq!(
        next_number(&b.conn, "FV", "2026-09-15").unwrap(),
        "2026-001"
    );

    // A supplier outside VAT can't charge it.
    let nb = books(false);
    let id = create_draft(
        &nb.conn,
        &to_germany(DocKind::Invoice, "FV", Some("DE123456789"), "EUSVC"),
    )
    .unwrap();
    let e = issue(&nb.conn, &nb.pack, &nb.accounts, id, "2026-09-15", None)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("the supplier isn't registered for VAT, so it can't charge EUSVC"),
        "{e}"
    );
}

#[test]
fn a_credit_note_against_an_eu_invoice_nets_it_off() {
    let b = books(true);
    let invoice = issue_to_germany(
        &b,
        &to_germany(DocKind::Invoice, "FV", Some("DE123456789"), "EUSVC"),
    )
    .unwrap();
    let credit =
        skyla_invoicing::draft_credit_note(&b.conn, &b.pack, invoice.id, "OD", None, "x").unwrap();
    issue(&b.conn, &b.pack, &b.accounts, credit, "2026-10-02", None).unwrap();
    let credit = get(&b.conn, &b.pack, credit).unwrap();
    assert_eq!(credit.totals.base.minor(), -6_000_000);
    assert_eq!(credit.totals.vat.minor(), 0);
    assert_eq!(credit.customer.dic.as_deref(), Some("DE123456789"));
}
