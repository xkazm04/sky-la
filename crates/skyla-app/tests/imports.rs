//! WP-31 acceptance (import): issued invoices from Pohoda XML and a
//! Fakturoid CSV are previewed as new, already here, or with a problem;
//! committing posts only the new ones through the kernel, as balanced
//! entries the VAT return and the invoice list pick up, and importing the
//! same file again changes nothing. Credit notes import against their
//! invoice, in the books or in the same file. The samples are synthetic.

#![allow(clippy::unwrap_used)]

use base64::Engine as _;
use skyla_app::Core;

const POHODA: &[u8] = include_bytes!("../../../packages/fixtures/data/imports/pohoda-faktury.xml");
const FAKTUROID: &[u8] =
    include_bytes!("../../../packages/fixtures/data/imports/fakturoid-faktury.csv");
const POHODA_CREDIT: &[u8] =
    include_bytes!("../../../packages/fixtures/data/imports/pohoda-dobropisy.xml");
const FAKTUROID_CREDIT: &[u8] =
    include_bytes!("../../../packages/fixtures/data/imports/fakturoid-dobropisy.csv");

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn statuses(p: &skyla_app::dto::ImportPreviewDto) -> Vec<(String, String)> {
    p.documents
        .iter()
        .map(|d| (d.number.clone(), d.status.clone()))
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn pohoda_preview_then_commit_posts_only_the_new_invoices() {
    let core = Core::demo().unwrap();
    let vat_before = core.vat_return("2026-04-01", "2026-04-30").unwrap();
    let next_before = core.invoice_form().unwrap().next_number;
    let p = core
        .preview_invoice_import("pohoda-faktury.xml", &b64(POHODA))
        .unwrap();
    assert_eq!(p.source, "Pohoda");
    assert_eq!(
        statuses(&p),
        pairs(&[
            ("2026-038", "problem"),
            ("2026-041", "duplicate"),
            ("2026-044", "new"),
            ("2026-047", "new"),
            ("2026-049", "problem"),
        ])
    );
    assert_eq!(p.new, 2);
    assert!(p.documents[0].problems[0].contains("outside the periods"));
    assert!(p.documents[4].problems[0].contains("VAT doesn't match"));
    assert_eq!(
        p.problems.len(),
        1,
        "the received invoice: {:?}",
        p.problems
    );

    // The preview changed nothing.
    assert!(
        core.invoices()
            .unwrap()
            .iter()
            .all(|i| i.number.as_deref() != Some("2026-044"))
    );

    let done = core
        .commit_invoice_import("pohoda-faktury.xml", &b64(POHODA))
        .unwrap();
    assert_eq!(done.imported, ["2026-044", "2026-047"]);
    assert_eq!(done.new, 0, "everything posted is now already here");

    let invoices = core.invoices().unwrap();
    let i044 = invoices
        .iter()
        .find(|i| i.number.as_deref() == Some("2026-044"))
        .unwrap();
    assert_eq!(i044.gross.minor, 2_178_000);
    assert_eq!(
        i044.status, "overdue",
        "unpaid since May; the bank settles it"
    );

    // Output VAT for April grew by exactly the imported VAT.
    let vat_after = core.vat_return("2026-04-01", "2026-04-30").unwrap();
    assert_eq!(
        vat_after.output_tax.minor - vat_before.output_tax.minor,
        378_000 + 201_600
    );
    let tb = core.trial_balance(None, "2026-12-31").unwrap();
    assert_eq!(tb.total_debit.minor, tb.total_credit.minor);

    // The imported numbers fill the existing series (no new series), below
    // its next number.
    assert_eq!(core.invoice_form().unwrap().next_number, next_before);
    assert_eq!(next_before, "2026-115");

    // Again: nothing new.
    let again = core.commit_invoice_import("pohoda-faktury.xml", &b64(POHODA));
    assert!(again.is_err());
}

#[test]
fn fakturoid_totals_find_their_rate_and_a_credit_note_without_its_invoice_waits() {
    let core = Core::demo().unwrap();
    let p = core
        .preview_invoice_import("fakturoid-faktury.csv", &b64(FAKTUROID))
        .unwrap();
    assert_eq!(p.source, "Fakturoid");
    assert_eq!(
        statuses(&p),
        pairs(&[
            ("2026-051", "new"),
            ("2026-052", "new"),
            ("OD2026-003", "problem")
        ])
    );
    // It is a credit note now (no longer refused as "not imported yet"), but
    // this export doesn't say which invoice it corrects.
    assert_eq!(p.documents[2].kind, "credit_note");
    assert_eq!(p.documents[2].total.minor, -121_000);
    assert!(p.documents[2].problems[0].contains("names no original invoice"));
    let done = core
        .commit_invoice_import("fakturoid-faktury.csv", &b64(FAKTUROID))
        .unwrap();
    assert_eq!(done.imported, ["2026-051", "2026-052"]);
    let vat = core.vat_return("2026-05-01", "2026-05-31").unwrap();
    let row2 = vat.rows.iter().find(|x| x.row == "2").unwrap();
    assert_eq!(
        (row2.base.minor, row2.tax.minor),
        (200_000, 24_000),
        "the 12 % supply"
    );
}

fn balance_311(core: &Core) -> i64 {
    core.trial_balance(None, "2026-12-31")
        .unwrap()
        .rows
        .iter()
        .find(|r| r.code == "311")
        .map_or(0, |r| r.balance.minor)
}

fn invoice(core: &Core, number: &str) -> skyla_app::dto::InvoiceDto {
    core.invoices()
        .unwrap()
        .into_iter()
        .find(|i| i.number.as_deref() == Some(number))
        .unwrap()
}

#[test]
fn pohoda_credit_notes_post_against_their_invoices_originals_first() {
    let core = Core::demo().unwrap();
    let before_102 = invoice(&core, "2026-102");
    // Demo: 71 h x 1 200,00 = 85 200,00 + 21 % (17 892,00) = 103 092,00 gross;
    // 50 000,00 paid, so 53 092,00 open.
    assert_eq!(
        (before_102.gross.minor, before_102.open.minor),
        (10_309_200, 5_309_200)
    );
    let sept_before = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    let oct_before = core.vat_return("2026-10-01", "2026-10-31").unwrap();
    let tb_before = balance_311(&core);

    let p = core
        .preview_invoice_import("pohoda-dobropisy.xml", &b64(POHODA_CREDIT))
        .unwrap();
    assert_eq!(
        statuses(&p),
        pairs(&[
            // Partial: 12 100,00 of the 53 092,00 open.
            ("OD2026-001", "new"),
            // 48 400,00 > 53 092,00 - 12 100,00 = 40 992,00 left after OD2026-001.
            ("OD2026-002", "problem"),
            // 2026-114 is paid in full: nothing open to credit.
            ("OD2026-003", "problem"),
            ("OD2026-004", "problem"),
            // Listed before its invoice 2026-130 in the file; still new.
            ("DB26-001", "new"),
            ("2026-130", "new"),
        ])
    );
    assert_eq!(p.new, 3);
    assert_eq!(
        (p.documents[0].kind.as_str(), p.documents[0].total.minor),
        ("credit_note", -1_210_000)
    );
    let why = |i: usize| p.documents[i].problems.join("; ");
    assert!(
        why(1).contains("credits 48400,00 CZK but only 40992,00 CZK of invoice 2026-102"),
        "{}",
        why(1)
    );
    assert!(
        why(2).contains("credits 1210,00 CZK but only 0,00 CZK of invoice 2026-114"),
        "{}",
        why(2)
    );
    assert!(why(3).contains("2026-999") && why(3).contains("isn't among the issued invoices"));

    // The preview changed nothing.
    assert_eq!(invoice(&core, "2026-102").credited.minor, 0);
    assert!(
        core.invoices()
            .unwrap()
            .iter()
            .all(|i| i.number.as_deref() != Some("2026-130"))
    );

    let done = core
        .commit_invoice_import("pohoda-dobropisy.xml", &b64(POHODA_CREDIT))
        .unwrap();
    // Invoices first, then the credit notes in file order.
    assert_eq!(done.imported, ["2026-130", "OD2026-001", "DB26-001"]);
    assert_eq!(
        statuses(&done),
        pairs(&[
            ("OD2026-001", "duplicate"),
            ("OD2026-002", "problem"),
            ("OD2026-003", "problem"),
            ("OD2026-004", "problem"),
            ("DB26-001", "duplicate"),
            ("2026-130", "duplicate"),
        ]),
        "what was posted is already here; the refused stay refused"
    );

    // 2026-102: 53 092,00 - 12 100,00 = 40 992,00 open; the payment is untouched.
    let a = invoice(&core, "2026-102");
    assert_eq!(
        (a.paid.minor, a.credited.minor, a.open.minor),
        (5_000_000, 1_210_000, 4_099_200)
    );
    // 2026-130: 10 000,00 + 2 100,00 = 12 100,00; credited 4 840,00; 7 260,00 open.
    let b = invoice(&core, "2026-130");
    assert_eq!(
        (b.gross.minor, b.credited.minor, b.open.minor),
        (1_210_000, 484_000, 726_000)
    );

    // September: +2 100,00 (2026-130) - 840,00 (DB26-001) = +1 260,00.
    let sept = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    assert_eq!(
        sept.output_tax.minor - sept_before.output_tax.minor,
        210_000 - 84_000
    );
    // October: OD2026-001 takes 2 100,00 of output VAT back.
    let oct = core.vat_return("2026-10-01", "2026-10-31").unwrap();
    assert_eq!(oct.output_tax.minor - oct_before.output_tax.minor, -210_000);
    // Receivables: +12 100,00 - 12 100,00 - 4 840,00 = -4 840,00.
    assert_eq!(balance_311(&core) - tb_before, -484_000);
    let tb = core.trial_balance(None, "2026-12-31").unwrap();
    assert_eq!(tb.total_debit.minor, tb.total_credit.minor);

    // Again: nothing new, nothing changes.
    assert!(
        core.commit_invoice_import("pohoda-dobropisy.xml", &b64(POHODA_CREDIT))
            .is_err()
    );
    assert_eq!(invoice(&core, "2026-102").credited.minor, 1_210_000);
}

#[test]
fn fakturoid_credit_notes_by_type_and_original_column() {
    let core = Core::demo().unwrap();
    let sept_before = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    let p = core
        .preview_invoice_import("fakturoid-dobropisy.csv", &b64(FAKTUROID_CREDIT))
        .unwrap();
    assert_eq!(
        statuses(&p),
        pairs(&[
            ("2026-140", "new"),
            // Stated positive (5 000,00 + 1 050,00), kept as a credit.
            ("OD2026-010", "new"),
            // 24 200,00 > 24 200,00 - 6 050,00 = 18 150,00 still open.
            ("OD2026-011", "problem"),
            // Against the paid 2026-041, dated before the books begin.
            ("OD2026-012", "problem"),
        ])
    );
    assert_eq!(p.documents[1].total.minor, -605_000);
    assert!(
        p.documents[2]
            .problems
            .join("; ")
            .contains("credits 24200,00 CZK but only 18150,00 CZK of invoice 2026-140")
    );
    let all = p.documents[3].problems.join("; ");
    for part in [
        "outside the periods",
        "credits 1210,00 CZK but only 0,00 CZK of invoice 2026-041",
        "before the invoice 2026-041",
    ] {
        assert!(all.contains(part), "{part:?} in {all}");
    }
    let done = core
        .commit_invoice_import("fakturoid-dobropisy.csv", &b64(FAKTUROID_CREDIT))
        .unwrap();
    assert_eq!(done.imported, ["2026-140", "OD2026-010"]);
    // 24 200,00 gross credited by 6 050,00: 18 150,00 open.
    let inv = invoice(&core, "2026-140");
    assert_eq!(
        (inv.gross.minor, inv.credited.minor, inv.open.minor),
        (2_420_000, 605_000, 1_815_000)
    );
    // VAT: +4 200,00 - 1 050,00 = +3 150,00.
    let sept = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    assert_eq!(
        sept.output_tax.minor - sept_before.output_tax.minor,
        420_000 - 105_000
    );
}

#[test]
fn a_full_credit_leaves_nothing_to_credit_again_and_english_headers_read() {
    let core = Core::demo().unwrap();
    let csv = "Number,Document type,Issued on,Client,Client registration no,Original invoice,Subtotal,VAT,Total\n\
               A-1,Invoice,2026-09-10,Studio Brno s.r.o.,94722188,,\"1,000.00\",210.00,\"1,210.00\"\n\
               CN-1,Credit note,2026-09-12,Studio Brno s.r.o.,94722188,A-1,\"1,000.00\",210.00,\"1,210.00\"\n\
               CN-2,Credit note,2026-09-13,Studio Brno s.r.o.,94722188,A-1,100.00,21.00,121.00\n";
    let p = core
        .preview_invoice_import("export.csv", &b64(csv.as_bytes()))
        .unwrap();
    assert_eq!(
        statuses(&p),
        pairs(&[("A-1", "new"), ("CN-1", "new"), ("CN-2", "problem")])
    );
    assert!(p.documents[2].problems[0].contains("only 0,00 CZK of invoice A-1"));
    let done = core
        .commit_invoice_import("export.csv", &b64(csv.as_bytes()))
        .unwrap();
    assert_eq!(done.imported, ["A-1", "CN-1"]);
    let a = invoice(&core, "A-1");
    assert_eq!(
        (a.status.as_str(), a.credited.minor, a.open.minor),
        ("credited", 121_000, 0)
    );
}

#[test]
fn a_credit_note_for_another_customer_or_before_its_invoice_is_refused() {
    let core = Core::demo().unwrap();
    // 2026-102 is Studio Brno's (94722188), issued 2026-08-25.
    let csv = "Číslo;Vystaveno;Odběratel;IČO;Původní doklad;Bez DPH;DPH;Celkem\n\
               OD2026-020;10.09.2026;Northwind Traders s.r.o.;91341272;2026-102;-100,00;-21,00;-121,00\n\
               OD2026-021;20.08.2026;Studio Brno s.r.o.;94722188;2026-102;-100,00;-21,00;-121,00\n\
               OD2026-022;10.09.2026;Studio Brno s.r.o.;94722188;2026-102;-100,00;-22,00;-122,00\n";
    let p = core
        .preview_invoice_import("x.csv", &b64(csv.as_bytes()))
        .unwrap();
    assert_eq!(
        statuses(&p),
        pairs(&[
            ("OD2026-020", "problem"),
            ("OD2026-021", "problem"),
            ("OD2026-022", "problem"),
        ])
    );
    assert!(p.documents[0].problems[0].contains("another customer than invoice 2026-102"));
    assert!(p.documents[1].problems[0].contains("before the invoice 2026-102 it corrects"));
    assert!(p.documents[2].problems[0].contains("VAT doesn't match"));
    assert!(
        core.commit_invoice_import("x.csv", &b64(csv.as_bytes()))
            .is_err()
    );
}

#[test]
fn a_credit_note_whose_invoice_is_in_the_file_but_refused_is_refused_too() {
    let core = Core::demo().unwrap();
    // The invoice's VAT (220,00 on 1 000,00) isn't 21 % or 12 %.
    let csv = "Číslo;Vystaveno;Odběratel;IČO;Původní doklad;Bez DPH;DPH;Celkem\n\
               B-1;10.09.2026;Studio Brno s.r.o.;94722188;;1 000,00;220,00;1 220,00\n\
               OD-B1;12.09.2026;Studio Brno s.r.o.;94722188;B-1;-100,00;-21,00;-121,00\n";
    let p = core
        .preview_invoice_import("x.csv", &b64(csv.as_bytes()))
        .unwrap();
    assert_eq!(
        statuses(&p),
        pairs(&[("B-1", "problem"), ("OD-B1", "problem")])
    );
    assert!(p.documents[1].problems[0].contains("is in this file but won't import"));
}

#[test]
fn other_files_are_refused() {
    let core = Core::demo().unwrap();
    assert!(core.preview_invoice_import("x.bin", &b64(b"")).is_err());
    assert!(core.preview_invoice_import("x.bin", "not base64!").is_err());
}

#[test]
fn another_numbering_gets_its_own_series() {
    let core = Core::demo().unwrap();
    let csv = "Číslo;Vystaveno;Odběratel;IČO;Bez DPH;DPH;Celkem\n\
               FA-2026-0007;02.06.2026;Studio Brno s.r.o.;94722188;1 000,00;210,00;1 210,00\n\
               FA-2026-0008;09.06.2026;Studio Brno s.r.o.;94722188;2 000,00;420,00;2 420,00\n";
    let done = core
        .commit_invoice_import("fakturoid.csv", &b64(csv.as_bytes()))
        .unwrap();
    assert_eq!(done.imported, ["FA-2026-0007", "FA-2026-0008"]);
    let numbers: Vec<_> = core
        .invoices()
        .unwrap()
        .into_iter()
        .filter_map(|i| i.number)
        .filter(|n| n.starts_with("FA-"))
        .collect();
    assert_eq!(numbers.len(), 2);
}
