//! WP-12 acceptance: invoices render to PDF in Czech and English (text-layer
//! snapshots), and the QR code decodes back to the same SPAYD string.

use rusqlite::Connection;
use skyla_invoicing::{
    Accounts, Customer, DocKind, DraftInput, LineInput, Supplier, apply_schema, create_draft,
    define_series, draft_credit_note, get, issue, set_supplier, state,
};
use skyla_ledger::{ChartSpec, open_period};
use skyla_money::Currency;
use skyla_render::{Lang, invoice_pdf, qr_code};
use skyla_rules::Pack;

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");

struct Books {
    conn: Connection,
    pack: Pack,
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
    define_series(&conn, "FV", DocKind::Invoice, "{YYYY}-{NNN}", "Invoices").expect("series");
    define_series(
        &conn,
        "OD",
        DocKind::CreditNote,
        "OD{YY}{NNNN}",
        "Credit notes",
    )
    .expect("series");
    set_supplier(
        &conn,
        &Supplier {
            name: "Jana Nováková".into(),
            ico: Some("92588034".into()),
            dic: vat_payer.then(|| "CZ8051234567".into()),
            address: "Dlouhá 12\n110 00 Praha 1".into(),
            iban: Some("CZ65 0800 0000 1920 0014 5399".into()),
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
    }
}

fn line(description: &str, quantity: &str, unit: &str, price: i64, code: &str) -> LineInput {
    LineInput {
        description: description.into(),
        quantity: quantity.into(),
        unit: unit.into(),
        unit_price_minor: price,
        vat_code: code.into(),
        account: None,
    }
}

fn issued_invoice(b: &Books, lines: Vec<LineInput>) -> i64 {
    let id = create_draft(
        &b.conn,
        &DraftInput {
            kind: DocKind::Invoice,
            series: "FV".into(),
            customer: Customer {
                name: "Northwind Traders s.r.o.".into(),
                ico: Some("93617453".into()),
                dic: Some("CZ93617453".into()),
                address: Some("Vinohradská 1, 120 00 Praha 2".into()),
            },
            due_date: Some("2026-09-29".into()),
            tax_point_date: None,
            note: "Děkuji za spolupráci.".into(),
            lines,
            related_id: None,
            advances: Vec::new(),
        },
    )
    .expect("draft");
    issue(&b.conn, &b.pack, &Accounts::cz(), id, "2026-09-15", None).expect("issue");
    id
}

fn render(b: &Books, id: i64, related: Option<&str>, lang: Lang) -> skyla_render::Rendered {
    let doc = get(&b.conn, &b.pack, id).expect("get");
    let open = state(&b.conn, &b.pack, id).expect("state").open;
    let due = if open.minor() > 0 {
        open
    } else {
        doc.totals.gross
    };
    invoice_pdf(&doc, due, related, lang).expect("render")
}

#[test]
fn renders_a_czech_tax_invoice() {
    let b = books(true);
    let id = issued_invoice(
        &b,
        vec![
            line("Návrh produktu · září", "56", "h", 125_000, "OUT21"),
            line("Workshop", "1", "ks", 1_200_000, "OUT21"),
            line("Odborná publikace", "2", "ks", 45_000, "OUT12"),
        ],
    );
    let r = render(&b, id, None, Lang::Cs);
    assert!(r.pdf.starts_with(b"%PDF-"));
    assert_eq!(r.text.len(), 1);
    insta::assert_snapshot!("invoice_cs", r.text.join("\n--- page ---\n"));
}

#[test]
fn renders_an_english_invoice_for_a_non_vat_payer() {
    let b = books(false);
    let id = issued_invoice(&b, vec![line("Illustration", "3.5", "h", 90_000, "NOVAT")]);
    let r = render(&b, id, None, Lang::En);
    insta::assert_snapshot!("invoice_en_non_payer", r.text.join("\n--- page ---\n"));
}

#[test]
fn renders_a_credit_note_without_a_qr_code() {
    let b = books(true);
    let id = issued_invoice(&b, vec![line("Workshop", "1", "ks", 1_200_000, "OUT21")]);
    let note =
        draft_credit_note(&b.conn, &b.pack, id, "OD", None, "Workshop zrušen").expect("note");
    issue(&b.conn, &b.pack, &Accounts::cz(), note, "2026-09-20", None).expect("issue note");
    let r = render(&b, note, Some("2026-001"), Lang::Cs);
    assert_eq!(r.spayd, None);
    insta::assert_snapshot!("credit_note_cs", r.text.join("\n--- page ---\n"));
}

#[test]
fn renders_identically_twice() {
    let b = books(true);
    let id = issued_invoice(&b, vec![line("Workshop", "1", "ks", 1_200_000, "OUT21")]);
    assert_eq!(
        render(&b, id, None, Lang::Cs).pdf,
        render(&b, id, None, Lang::Cs).pdf
    );
}

#[test]
fn the_qr_code_decodes_back_to_the_spayd_string() {
    let b = books(true);
    let id = issued_invoice(
        &b,
        vec![line("Návrh produktu", "56", "h", 125_000, "OUT21")],
    );
    let r = render(&b, id, None, Lang::Cs);
    let payload = r.spayd.expect("an invoice has a QR code");
    assert_eq!(
        payload,
        "SPD*1.0*ACC:CZ6508000000192000145399+GIBACZPX*AM:84700.00*CC:CZK*DT:20260929*RN:Jana Nováková*MSG:Faktura 2026-001*X-VS:2026001"
    );

    // Rasterise the module matrix (4 px per module, quiet zone) and decode.
    let code = qr_code(&payload).expect("qr");
    let width = code.width();
    let colors = code.to_colors();
    let scale = 4;
    let size = (width + 8) * scale;
    let image = image_from(size, |x, y| {
        let (mx, my) = (x / scale, y / scale);
        let inside = (4..width + 4).contains(&mx) && (4..width + 4).contains(&my);
        inside && colors[(my - 4) * width + (mx - 4)] == qrcode::Color::Dark
    });
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(size, size, |x, y| image[y * size + x]);
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1);
    let (_, content) = grids[0].decode().expect("decode");
    assert_eq!(content, payload);
}

fn image_from(size: usize, dark: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    (0..size * size)
        .map(|i| if dark(i % size, i / size) { 0 } else { 255 })
        .collect()
}

/// Writes the sample PDFs for a visual check:
/// `SKYLA_PDF_OUT=dir cargo test -p skyla-render -- --ignored`.
#[test]
#[ignore = "writes files; run by hand"]
fn write_samples() {
    let Some(dir) = std::env::var_os("SKYLA_PDF_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let b = books(true);
    let id = issued_invoice(
        &b,
        vec![
            line("Návrh produktu · září", "56", "h", 125_000, "OUT21"),
            line("Workshop", "1", "ks", 1_200_000, "OUT21"),
            line("Odborná publikace", "2", "ks", 45_000, "OUT12"),
        ],
    );
    std::fs::write(
        dir.join("invoice-cs.pdf"),
        render(&b, id, None, Lang::Cs).pdf,
    )
    .expect("ok");
    std::fs::write(
        dir.join("invoice-en.pdf"),
        render(&b, id, None, Lang::En).pdf,
    )
    .expect("ok");
    let note = draft_credit_note(&b.conn, &b.pack, id, "OD", None, "Workshop zrušen").expect("ok");
    issue(&b.conn, &b.pack, &Accounts::cz(), note, "2026-09-20", None).expect("ok");
    std::fs::write(
        dir.join("credit-cs.pdf"),
        render(&b, note, Some("2026-001"), Lang::Cs).pdf,
    )
    .expect("ok");
}
