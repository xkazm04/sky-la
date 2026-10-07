//! WP-14 acceptance: the EN 16931 writers. Every golden document is written
//! as UBL 2.1 (Peppol BIS Billing 3.0) and as CII D16B, pinned by snapshots,
//! and saved for `just einvoice`, which checks each one against its XSD and
//! the official Schematron (CEN EN 16931, OpenPeppol BIS 3) with zero errors.

mod common;

use std::path::PathBuf;

use common::{Books, books, draft, line};
use skyla_invoicing::{DocKind, Document, ExportInput, to_cii, to_ubl};

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("SKYLA_EINVOICE_OUT").map_or_else(
        || PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("einvoice-golden"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).expect("output dir");
    dir
}

fn input<'a>(
    doc: &'a Document,
    related: Option<&'a Document>,
    advances: &'a [Document],
) -> ExportInput<'a> {
    ExportInput {
        doc,
        related,
        advances,
    }
}

/// Writes both syntaxes for one golden case and returns them.
fn golden(b: &Books, name: &str, input: ExportInput<'_>) -> (String, String) {
    let ubl = to_ubl(&b.pack, input).expect("ubl");
    let cii = to_cii(&b.pack, input).expect("cii");
    let dir = out_dir();
    std::fs::write(dir.join(format!("{name}.ubl.xml")), &ubl).expect("write");
    std::fs::write(dir.join(format!("{name}.cii.xml")), &cii).expect("write");
    insta::assert_snapshot!(format!("{name}_ubl"), ubl);
    insta::assert_snapshot!(format!("{name}_cii"), cii);
    (ubl, cii)
}

#[test]
fn a_tax_invoice_at_two_rates() {
    let b = books(true);
    let doc = b.issue(
        &draft(
            DocKind::Invoice,
            "FV",
            vec![
                line("Návrh webu", "21", "h", 120_000, "OUT21"),
                line("Odborná publikace", "2", "ks", 45_050, "OUT12"),
                line("Konzultace", "2.5", "h", 150_000, "OUT21"),
            ],
        ),
        "2026-08-25",
    );
    let (ubl, cii) = golden(&b, "invoice", input(&doc, None, &[]));
    assert!(ubl.contains("<cbc:InvoiceTypeCode>380</cbc:InvoiceTypeCode>"));
    assert!(ubl.contains(r#"<cbc:EndpointID schemeID="9929">CZ92588034</cbc:EndpointID>"#));
    assert!(
        ubl.contains(r#"<cbc:PayableAmount currencyID="CZK">36038.62</cbc:PayableAmount>"#),
        "{ubl}"
    );
    assert!(cii.contains("<ram:DuePayableAmount>36038.62</ram:DuePayableAmount>"));
    assert!(cii.contains("<ram:PaymentReference>2026001</ram:PaymentReference>"));
}

#[test]
fn a_credit_note_states_positive_amounts_and_cites_the_invoice() {
    let b = books(true);
    let (invoice, credit) = b.credit_chain();
    let (ubl, cii) = golden(&b, "credit_note", input(&credit, Some(&invoice), &[]));
    assert!(ubl.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CreditNote "));
    assert!(ubl.contains(r#"<cbc:CreditedQuantity unitCode="H87">1</cbc:CreditedQuantity>"#));
    assert!(ubl.contains(r#"<cbc:PayableAmount currencyID="CZK">14520.00</cbc:PayableAmount>"#));
    assert!(ubl.contains("<cac:InvoiceDocumentReference>\n      <cbc:ID>2026-001</cbc:ID>"));
    assert!(!ubl.contains("<cac:PaymentMeans>"));
    assert!(cii.contains("<ram:TypeCode>381</ram:TypeCode>"));
    assert!(cii.contains("<ram:IssuerAssignedID>2026-001</ram:IssuerAssignedID>"));
}

#[test]
fn the_advance_is_taxed_once_and_deducted_on_the_final_invoice() {
    let b = books(true);
    let (advance, tax, fin) = b.advance_chain();
    let refused = to_ubl(&b.pack, input(&advance, None, &[])).expect_err("not a tax document");
    assert!(
        refused.to_string().contains("isn't a tax document"),
        "{refused}"
    );

    let (ubl, _) = golden(&b, "advance_tax_document", input(&tax, Some(&advance), &[]));
    assert!(ubl.contains("<cbc:InvoiceTypeCode>386</cbc:InvoiceTypeCode>"));
    assert!(ubl.contains(r#"<cbc:PrepaidAmount currencyID="CZK">121000.00</cbc:PrepaidAmount>"#));
    assert!(ubl.contains(r#"<cbc:PayableAmount currencyID="CZK">0.00</cbc:PayableAmount>"#));

    let (ubl, cii) = golden(
        &b,
        "final_invoice",
        input(&fin, None, std::slice::from_ref(&tax)),
    );
    assert!(ubl.contains("<cbc:Name>Odpočet zálohy DZ2026-01</cbc:Name>"));
    assert!(ubl.contains(r#"<cbc:TaxableAmount currencyID="CZK">50000.00</cbc:TaxableAmount>"#));
    assert!(ubl.contains(r#"<cbc:PayableAmount currencyID="CZK">60500.00</cbc:PayableAmount>"#));
    assert!(cii.contains("<ram:DuePayableAmount>60500.00</ram:DuePayableAmount>"));
}

#[test]
fn a_supplier_outside_vat_goes_out_as_cii() {
    let b = books(false);
    let doc = b.issue(
        &draft(
            DocKind::Invoice,
            "FV",
            vec![line("Překlad", "3.5", "h", 80_000, "NOVAT")],
        ),
        "2026-09-01",
    );
    let err = to_ubl(&b.pack, input(&doc, None, &[])).expect_err("no endpoint");
    assert!(err.to_string().contains("export CII instead"), "{err}");
    let cii = to_cii(&b.pack, input(&doc, None, &[])).expect("cii");
    std::fs::write(out_dir().join("non_payer.cii.xml"), &cii).expect("write");
    assert!(cii.contains("<ram:CategoryCode>O</ram:CategoryCode>"));
    assert!(cii.contains(
        "<ram:ExemptionReason>Dodavatel není plátcem DPH (§ 6 zákona o DPH).</ram:ExemptionReason>"
    ));
    assert!(
        !cii.contains("SpecifiedTaxRegistration"),
        "category O carries no VAT ids"
    );
    insta::assert_snapshot!("non_payer_cii", cii);
}
