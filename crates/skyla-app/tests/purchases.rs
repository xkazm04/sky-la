//! Improvement wave 3: received invoices in real books. A purchase posts
//! through the kernel with the VAT the pack gives, the kontrolní hlášení
//! itemises it with the supplier's DIČ, the return claims its input VAT,
//! and the advisors' gate pseudonymises suppliers and customers recorded on
//! documents (real books have no fixture naming them).

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::{EntitySetupDto, PurchaseDraftDto, PurchaseLineDraftDto};
use skyla_app::session::Gate;

const SUPPLIER: &str = "Kancelářské potřeby Novotný s.r.o.";

fn books() -> (tempfile::TempDir, Core) {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate
        .create(
            &EntitySetupDto {
                display_name: "Eva Malá".into(),
                ico: "27415830".into(),
                dic: Some("CZ8001011234".into()),
                address: "Dlouhá 1, 110 00 Praha 1".into(),
                vat_period: "monthly".into(),
                registration: "Zapsána v živnostenském rejstříku".into(),
                iban: Some("CZ6508000000192000145399".into()),
                bank_name: "ČSOB".into(),
                email: None,
                flat_rate_group: Some("liberal".into()),
            },
            "a long passphrase for the books",
        )
        .unwrap();
    (dir, core)
}

fn draft() -> PurchaseDraftDto {
    PurchaseDraftDto {
        supplier: SUPPLIER.into(),
        ico: Some("26965313".into()),
        dic: Some("CZ26965313".into()),
        number: "FP-2026-1187".into(),
        issue_date: "2026-10-03".into(),
        tax_point_date: None,
        due_date: Some("2026-10-17".into()),
        lines: vec![
            PurchaseLineDraftDto {
                description: "Monitor".into(),
                account: "501".into(),
                vat_code: Some("IN21".into()),
                base: "12 000,00".into(),
            },
            PurchaseLineDraftDto {
                description: "Papír".into(),
                account: "501".into(),
                vat_code: Some("IN21".into()),
                base: "800,00".into(),
            },
        ],
        stated_vat: Some("2 688,00".into()),
    }
}

#[test]
fn a_received_invoice_posts_and_reaches_the_returns() {
    let (_dir, core) = books();
    let p = core.record_purchase(&draft()).unwrap();
    assert_eq!(
        (p.base.minor, p.vat.minor, p.gross.minor),
        (1_280_000, 268_800, 1_548_800)
    );
    assert_eq!(p.status, "open");
    assert_eq!(core.purchases().unwrap().len(), 1);
    assert!(core.integrity().unwrap().balanced);

    let kh = core.control_statement("2026-10-01", "2026-10-31").unwrap();
    let item = kh.b2.iter().find(|i| i.number == "FP-2026-1187").unwrap();
    assert_eq!(item.vat_id, "CZ26965313");
    assert_eq!(item.counterparty, SUPPLIER);
    assert_eq!(item.tax_standard.minor, 268_800);
    let ret = core.vat_return("2026-10-01", "2026-10-31").unwrap();
    assert_eq!(ret.input_tax.minor, 268_800);

    let again = core.record_purchase(&draft()).unwrap_err().to_string();
    assert!(again.contains("already recorded"), "{again}");
}

#[test]
fn every_problem_is_listed_at_once() {
    let (_dir, core) = books();
    let mut d = draft();
    d.dic = None;
    d.ico = Some("12345678".into());
    d.lines[1].account = "321".into();
    d.lines[0].base = "12000.00".into();
    d.issue_date = "2025-12-30".into();
    let refused = core.record_purchase(&d).unwrap_err().to_string();
    for p in [
        "needs the supplier's DIČ",
        "IČO 12345678",
        "account 321 isn't an expense account",
        "isn't an amount like 1 200,00",
        "outside the periods",
    ] {
        assert!(refused.contains(p), "{p}: {refused}");
    }
    let mut d = draft();
    d.stated_vat = Some("2 700,00".into());
    let refused = core.record_purchase(&d).unwrap_err().to_string();
    assert!(
        refused.contains("688,00") && refused.contains("2 700,00"),
        "{refused}"
    );
    assert!(core.purchases().unwrap().is_empty(), "nothing was posted");
}

#[test]
fn suppliers_and_customers_on_documents_are_pseudonymised() {
    let (_dir, core) = books();
    core.record_purchase(&draft()).unwrap();
    let mut invoice: skyla_app::dto::InvoiceDraftDto =
        serde_json::from_value(skyla_app::recordings::scripted_draft()).unwrap();
    invoice.client = String::new();
    invoice.new_client = Some(skyla_app::dto::ClientDto {
        name: "Lesní ateliér s.r.o.".into(),
        ico: Some("26965313".into()),
        dic: None,
        address: Some("Jasmínová 12, 106 00 Praha 10".into()),
    });
    core.create_invoice_draft(&invoice).unwrap();
    let gate = core.gate_for("financial.findings").unwrap();
    let (out, _) = gate.text(&format!(
        "{SUPPLIER} invoiced; Lesní ateliér s.r.o. owes us"
    ));
    assert!(!out.contains("Novotný"), "{out}");
    assert!(!out.contains("Lesní"), "{out}");
}
