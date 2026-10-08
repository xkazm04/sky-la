//! WP-13 acceptance: ISDOC 6.0.2 documents are valid against the official
//! schema, and their content is pinned by snapshots.

mod common;

use std::path::Path;
use std::process::Command;

use common::{Books, books, draft, line};
use skyla_invoicing::{DocKind, Document, ExportInput, create_draft, get, issue, to_isdoc};

const XSD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/schemas/isdoc-invoice-6.0.2.xsd"
);

/// The document as ISDOC, checked against the schema, with its UUIDs
/// replaced so snapshots are stable.
fn isdoc(b: &Books, doc: &Document, related: Option<&Document>, advances: &[Document]) -> String {
    let xml = to_isdoc(
        &b.pack,
        ExportInput {
            doc,
            related,
            advances,
        },
    )
    .expect("isdoc");
    validate(&xml);
    let mut out = xml.replace(&doc.uid, "00000000-0000-0000-0000-000000000001");
    if let Some(r) = related {
        out = out.replace(&r.uid, "00000000-0000-0000-0000-000000000002");
    }
    out
}

fn validate(xml: &str) {
    let dir = std::env::temp_dir().join(format!("skyla-isdoc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join(format!("{}.isdoc", uuid_like(xml)));
    std::fs::write(&file, xml).expect("write");
    match Command::new("xmllint")
        .args(["--noout", "--nonet", "--schema", XSD])
        .arg(&file)
        .output()
    {
        Ok(out) => assert!(
            out.status.success(),
            "ISDOC isn't schema-valid:\n{}\n{xml}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(_) if std::env::var_os("CI").is_none() => {
            eprintln!("xmllint isn't installed; schema validation skipped");
        }
        Err(e) => panic!("xmllint is required in CI: {e}"),
    }
    let _ = std::fs::remove_file(Path::new(&file));
}

fn uuid_like(xml: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    xml.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[test]
fn writes_a_valid_tax_invoice() {
    let b = books(true);
    let id = create_draft(
        &b.conn,
        &draft(
            DocKind::Invoice,
            "FV",
            vec![
                line("Návrh webu", "21", "h", 120_000, "OUT21"),
                line("Odborná publikace", "2", "ks", 45_050, "OUT12"),
            ],
        ),
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-08-25", None).expect("issue");
    let doc = get(&b.conn, &b.pack, id).expect("get");
    let xml = isdoc(&b, &doc, None, &[]);
    assert!(xml.contains("<DocumentType>1</DocumentType>"));
    assert!(xml.contains("<VariableSymbol>2026001</VariableSymbol>"));
    assert!(
        xml.contains("<ID>19-2000145399</ID>"),
        "domestic account from the IBAN"
    );
    assert!(xml.contains("<BankCode>0800</BankCode>"));
    insta::assert_snapshot!("invoice", xml);
}

#[test]
fn writes_a_credit_note_referring_to_its_invoice() {
    let b = books(true);
    let (invoice, credit) = b.credit_chain();
    let xml = isdoc(&b, &credit, Some(&invoice), &[]);
    assert!(xml.contains("<DocumentType>2</DocumentType>"));
    assert!(xml.contains("<OriginalDocumentReference ref=\"original\">"));
    assert!(
        !xml.contains("<PaymentMeans>"),
        "a credit note asks for no payment"
    );
    insta::assert_snapshot!("credit_note", xml);
}

#[test]
fn writes_the_advance_chain() {
    let b = books(true);
    let (advance, tax, fin) = b.advance_chain();
    let a = isdoc(&b, &advance, None, &[]);
    assert!(a.contains("<DocumentType>4</DocumentType>"));
    assert!(
        !a.contains("<TaxPointDate>"),
        "an advance invoice has no tax point"
    );
    let t = isdoc(&b, &tax, Some(&advance), &[]);
    assert!(t.contains("<DocumentType>5</DocumentType>"));
    assert!(t.contains("<TaxableAmount>100000.00</TaxableAmount>"));
    let f = isdoc(&b, &fin, None, std::slice::from_ref(&tax));
    assert!(f.contains("<AlreadyClaimedTaxableAmount>100000.00</AlreadyClaimedTaxableAmount>"));
    assert!(f.contains("<PayableAmount>60500.00</PayableAmount>"));
    assert!(
        f.contains("<TaxedDeposit>\n      <ID>DZ2026-01</ID>"),
        "{f}"
    );
    insta::assert_snapshot!("advance_tax_document", t);
    insta::assert_snapshot!("final_invoice", f);
}

#[test]
fn writes_a_supplier_outside_vat() {
    let b = books(false);
    let id = create_draft(
        &b.conn,
        &draft(
            DocKind::Invoice,
            "FV",
            vec![line("Překlad", "3,5", "h", 80_000, "NOVAT")],
        ),
    );
    // Czech decimal commas aren't accepted; the app sends dots.
    assert!(id.is_err());
    let id = create_draft(
        &b.conn,
        &draft(
            DocKind::Invoice,
            "FV",
            vec![line("Překlad", "3.5", "h", 80_000, "NOVAT")],
        ),
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-01", None).expect("issue");
    let doc = get(&b.conn, &b.pack, id).expect("get");
    let xml = isdoc(&b, &doc, None, &[]);
    assert!(xml.contains("<VATApplicable>false</VATApplicable>"));
    assert!(!xml.contains("<PartyTaxScheme>\n          <CompanyID>CZ92588034"));
    assert!(xml.contains("<PayableAmount>2800.00</PayableAmount>"));
    insta::assert_snapshot!("non_payer", xml);
}

#[test]
fn refuses_drafts() {
    let b = books(true);
    let id = create_draft(
        &b.conn,
        &draft(
            DocKind::Invoice,
            "FV",
            vec![line("X", "1", "h", 100, "OUT21")],
        ),
    )
    .expect("draft");
    let doc = get(&b.conn, &b.pack, id).expect("get");
    let err = to_isdoc(
        &b.pack,
        ExportInput {
            doc: &doc,
            related: None,
            advances: &[],
        },
    )
    .expect_err("draft");
    assert!(err.to_string().contains("issued"), "{err}");
}

#[test]
fn writes_a_service_to_another_member_state() {
    let b = books(true);
    let id = create_draft(
        &b.conn,
        &skyla_invoicing::DraftInput {
            customer: skyla_invoicing::Customer {
                name: "Beispiel GmbH".into(),
                ico: None,
                dic: Some("DE123456789".into()),
                address: Some("Hauptstraße 5\n10115 Berlin".into()),
            },
            ..draft(
                DocKind::Invoice,
                "FV",
                // 40 h × 1 500,00 = 60 000,00, no Czech VAT.
                vec![line("Konzultace", "40", "h", 150_000, "EUSVC")],
            )
        },
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("issue");
    let doc = get(&b.conn, &b.pack, id).expect("get");
    let xml = isdoc(&b, &doc, None, &[]);
    assert!(xml.contains("<PayableAmount>60000.00</PayableAmount>"));
    assert!(xml.contains("<Country>\n"), "{xml}");
    assert!(xml.contains("<IdentificationCode>DE</IdentificationCode>"));
    assert!(
        xml.contains("Daň odvede zákazník"),
        "the pack's wording is in the note"
    );
    insta::assert_snapshot!("eu_service", xml);
}
