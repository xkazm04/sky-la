//! Improvement wave 8: a month in real books, end to end. Fresh encrypted
//! books invoice a new customer, take the payment from a bank statement,
//! record a supplier's invoice, compute the DPH return and the kontrolní
//! hlášení, export, back up and pass the drill, then come back the same
//! after reopening. The demo can't stand in for this: it is built from a
//! fixture.

#![allow(clippy::unwrap_used)]

use base64::Engine as _;
use skyla_app::dto::{
    ClientDto, EntitySetupDto, InvoiceDraftDto, PurchaseDraftDto, PurchaseLineDraftDto,
};
use skyla_app::session::Gate;

const PASS: &str = "a long passphrase for the books";

fn setup() -> EntitySetupDto {
    EntitySetupDto {
        display_name: "Eva Malá".into(),
        ico: "27415830".into(),
        dic: Some("CZ8001011234".into()),
        address: "Dlouhá 1\n110 00 Praha 1".into(),
        vat_period: "monthly".into(),
        registration: "Zapsána v živnostenském rejstříku".into(),
        iban: Some("CZ6508000000192000145399".into()),
        bank_name: "Fio".into(),
        email: Some("eva@example.cz".into()),
        flat_rate_group: Some("liberal".into()),
    }
}

/// A Fio statement for October paying invoice 2026-001 in full.
fn statement() -> String {
    let head = "\u{feff}\"accountId\";\"2000145399\"\n\"bankId\";\"2010\"\n\"currency\";\"CZK\"\n\
\"iban\";\"CZ6508000000192000145399\"\n\"bic\";\"FIOBCZPPXXX\"\n\"openingBalance\";\"0,00\"\n\
\"closingBalance\";\"30734,00\"\n\"dateStart\";\"01.10.2026\"\n\"dateEnd\";\"07.10.2026\"\n\
\"idFrom\";\"1\"\n\"idTo\";\"1\"\n\n";
    let columns = include_str!("../../skyla-bank/tests/samples/fio.csv")
        .lines()
        .find(|l| l.starts_with("\"ID pohybu\""))
        .unwrap()
        .to_owned();
    format!(
        "{head}{columns}\n\"1\";\"06.10.2026\";\"30734,00\";\"CZK\";\"123456789\";\"Lesní ateliér s.r.o.\";\"0300\";\"ČSOB\";\"0308\";\"2026001\";\"\";\"\";\"Faktura 2026-001\";\"Bezhotovostní příjem\";\"\";\"\";\"\";\"\";\"\"\n"
    )
}

#[test]
fn a_month_in_real_books() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate.create(&setup(), PASS).unwrap();

    // Invoice a new customer.
    let mut draft: InvoiceDraftDto =
        serde_json::from_value(skyla_app::recordings::scripted_draft()).unwrap();
    draft.client = String::new();
    draft.new_client = Some(ClientDto {
        name: "Lesní ateliér s.r.o.".into(),
        ico: Some("26965313".into()),
        dic: Some("CZ26965313".into()),
        address: Some("Jasmínová 12\n106 00 Praha 10".into()),
    });
    let saved = core.create_invoice_draft(&draft).unwrap();
    let issued = core.issue_invoice(saved.id, "2026-10-01").unwrap();
    assert_eq!(issued.number.as_deref(), Some("2026-001"));
    assert_eq!(issued.gross.minor, 3_073_400);
    let pdf = core.invoice_pdf(issued.id, "cs").unwrap();
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf.pdf_base64)
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    assert!(pdf.spayd.is_some(), "QR Platba from the supplier's IBAN");

    // The payment arrives and the matcher is certain.
    let b64 = base64::engine::general_purpose::STANDARD.encode(statement().as_bytes());
    let s = core.import_bank_statement("fio-2026-10.csv", &b64).unwrap();
    let line = s
        .lines
        .iter()
        .find(|l| l.amount.minor == 3_073_400)
        .unwrap();
    assert_eq!(line.status, "certain", "{line:?}");
    core.accept_certain_bank_lines().unwrap();
    let paid = core
        .invoices()
        .unwrap()
        .into_iter()
        .find(|i| i.id == issued.id)
        .unwrap();
    assert_eq!(paid.status, "paid");

    // A supplier's invoice.
    core.record_purchase(&PurchaseDraftDto {
        supplier: "Kancelářské potřeby Novotný s.r.o.".into(),
        ico: Some("26965313".into()),
        dic: Some("CZ26965313".into()),
        number: "FP-2026-1187".into(),
        issue_date: "2026-10-03".into(),
        tax_point_date: None,
        due_date: Some("2026-10-17".into()),
        lines: vec![PurchaseLineDraftDto {
            description: "Monitor".into(),
            account: "501".into(),
            vat_code: Some("IN21".into()),
            base: "12 800,00".into(),
        }],
        stated_vat: None,
    })
    .unwrap();

    // October's returns.
    let ret = core.vat_return("2026-10-01", "2026-10-31").unwrap();
    assert_eq!(
        (ret.output_tax.minor, ret.input_tax.minor),
        (533_400, 268_800)
    );
    assert_eq!(ret.payable.minor, 264_600);
    let kh = core.control_statement("2026-10-01", "2026-10-31").unwrap();
    assert!(kh.problems.is_empty(), "{:?}", kh.problems);
    assert!(kh.matches_return);
    assert_eq!(kh.a4.len(), 1);
    assert_eq!(kh.a4[0].vat_id, "CZ26965313");
    assert_eq!(kh.b2.len(), 1);

    // The Overview and Taxes see October's books.
    let periods = core.reporting_periods().unwrap();
    assert_eq!(periods.vat[0].label, "September 2026");

    // Export, back up, drill.
    let export = core.export_books().unwrap();
    assert!(export.files >= 5);
    core.backup_now("2026-10-07T12:00:00Z").unwrap();
    assert!(core.restore_drill().unwrap().passed);
    let integrity = core.integrity().unwrap();
    assert!(integrity.chain_intact && integrity.balanced);
    let head = integrity.head.clone();
    drop(core);

    // Reopened, it's all there.
    let core = gate.unlock(PASS, false).unwrap();
    let again = core.integrity().unwrap();
    assert!(again.chain_intact);
    assert_eq!(again.head, head);
    let paid = core
        .invoices()
        .unwrap()
        .into_iter()
        .find(|i| i.id == issued.id)
        .unwrap();
    assert_eq!(paid.status, "paid");
    assert_eq!(core.purchases().unwrap().len(), 1);
    let s = core.bank_statement().unwrap();
    assert!(s.lines.iter().all(|l| l.status == "booked"));
    assert_eq!(
        core.vat_return("2026-10-01", "2026-10-31")
            .unwrap()
            .payable
            .minor,
        264_600
    );
}
