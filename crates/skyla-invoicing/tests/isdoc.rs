//! WP-13 acceptance: ISDOC 6.0.2 documents are valid against the official
//! schema, and their content is pinned by snapshots.

use std::path::Path;
use std::process::Command;

use rusqlite::Connection;
use skyla_invoicing::{
    Accounts, Customer, DocKind, Document, DraftInput, ExportInput, LineInput, Supplier,
    apply_schema, create_draft, define_series, draft_credit_note, get, issue, set_supplier,
    to_isdoc,
};
use skyla_ledger::{ChartSpec, NewEntry, NewLine, SourceKind, open_period, post_entry};
use skyla_money::{Currency, Money};
use skyla_rules::Pack;

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");
const XSD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/schemas/isdoc-invoice-6.0.2.xsd"
);

struct Books {
    conn: Connection,
    pack: Pack,
    accounts: Accounts,
}

fn books(vat_payer: bool) -> Books {
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    skyla_ledger::apply_schema(&conn).expect("ledger schema");
    apply_schema(&conn).expect("invoicing schema");
    skyla_ledger::seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    skyla_ledger::set_functional_currency(&conn, Currency::CZK).expect("ccy");
    open_period(&conn, "2026-01-01", "2026-12-31").expect("2026");
    for (series, kind, pattern) in [
        ("FV", DocKind::Invoice, "{YYYY}-{NNN}"),
        ("OD", DocKind::CreditNote, "OD{YY}{NNNN}"),
        ("ZF", DocKind::Advance, "ZF{YYYY}-{NN}"),
        ("DZ", DocKind::AdvanceTax, "DZ{YYYY}-{NN}"),
    ] {
        define_series(&conn, series, kind, pattern, series).expect("series");
    }
    set_supplier(
        &conn,
        &Supplier {
            name: "Jana Nováková".into(),
            ico: Some("25596641".into()),
            dic: vat_payer.then(|| "CZ25596641".into()),
            address: "Korunní 2569/108\n101 00 Praha 10".into(),
            iban: Some("CZ6508000000192000145399".into()),
            bic: Some("GIBACZPX".into()),
            email: Some("jana@example.cz".into()),
            vat_payer,
            registration: "Fyzická osoba zapsaná v živnostenském rejstříku".into(),
        },
    )
    .expect("supplier");
    Books {
        conn,
        pack: Pack::cz_2026().expect("pack"),
        accounts: Accounts::cz(),
    }
}

fn line(
    description: &str,
    quantity: &str,
    unit: &str,
    unit_price_minor: i64,
    vat: &str,
) -> LineInput {
    LineInput {
        description: description.into(),
        quantity: quantity.into(),
        unit: unit.into(),
        unit_price_minor,
        vat_code: vat.into(),
        account: None,
    }
}

fn draft(kind: DocKind, series: &str, lines: Vec<LineInput>) -> DraftInput {
    DraftInput {
        kind,
        series: series.into(),
        customer: Customer {
            name: "Studio Brno s.r.o.".into(),
            ico: Some("27082440".into()),
            dic: Some("CZ27082440".into()),
            address: Some("Masarykova 12, 602 00 Brno".into()),
        },
        due_date: Some("2026-09-29".into()),
        tax_point_date: None,
        note: "Děkuji za spolupráci & těším se na další <projekt>.".into(),
        lines,
        related_id: None,
        advances: Vec::new(),
    }
}

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
    let id = create_draft(
        &b.conn,
        &draft(
            DocKind::Invoice,
            "FV",
            vec![line("Workshop", "1", "ks", 1_200_000, "OUT21")],
        ),
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-10", None).expect("issue");
    let credit =
        draft_credit_note(&b.conn, &b.pack, id, "OD", None, "Workshop zrušen").expect("credit");
    issue(&b.conn, &b.pack, &b.accounts, credit, "2026-09-20", None).expect("issue");
    let (invoice, credit) = (
        get(&b.conn, &b.pack, id).expect("get"),
        get(&b.conn, &b.pack, credit).expect("get"),
    );
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
    let advance = create_draft(
        &b.conn,
        &draft(
            DocKind::Advance,
            "ZF",
            vec![line("Záloha na web", "1", "", 10_000_000, "OUT21")],
        ),
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, advance, "2026-08-01", None).expect("issue");
    let receipt = skyla_ledger::create_draft(
        &b.conn,
        &NewEntry {
            date: "2026-08-05".into(),
            source_kind: SourceKind::Bank,
            source_ref: None,
            memo: "Záloha".into(),
            created_by: "user".into(),
            lines: vec![
                NewLine::debit("221", Money::new(12_100_000, Currency::CZK)),
                NewLine::credit("324", Money::new(12_100_000, Currency::CZK)).expect("ok"),
            ],
        },
    )
    .expect("receipt");
    post_entry(&b.conn, receipt, None).expect("post");
    let mut tax = draft(
        DocKind::AdvanceTax,
        "DZ",
        vec![line("Přijatá záloha", "1", "", 12_100_000, "OUT21")],
    );
    tax.related_id = Some(advance);
    tax.due_date = None;
    tax.tax_point_date = Some("2026-08-05".into());
    let tax = create_draft(&b.conn, &tax).expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, tax, "2026-08-06", None).expect("issue");
    let mut fin = draft(
        DocKind::Invoice,
        "FV",
        vec![line("Web", "1", "", 15_000_000, "OUT21")],
    );
    fin.advances = vec![tax];
    fin.due_date = Some("2026-09-30".into());
    let fin = create_draft(&b.conn, &fin).expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, fin, "2026-09-15", None).expect("issue");

    let advance = get(&b.conn, &b.pack, advance).expect("get");
    let tax = get(&b.conn, &b.pack, tax).expect("get");
    let fin = get(&b.conn, &b.pack, fin).expect("get");
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
    assert!(!xml.contains("<PartyTaxScheme>\n          <CompanyID>CZ25596641"));
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
