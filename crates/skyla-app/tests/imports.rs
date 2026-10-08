//! WP-31 acceptance (import): issued invoices from Pohoda XML and a
//! Fakturoid CSV are previewed as new, already here, or with a problem;
//! committing posts only the new ones through the kernel, as balanced
//! entries the VAT return and the invoice list pick up, and importing the
//! same file again changes nothing. The samples are synthetic.

#![allow(clippy::unwrap_used)]

use base64::Engine as _;
use skyla_app::Core;

const POHODA: &[u8] = include_bytes!("../../../packages/fixtures/data/imports/pohoda-faktury.xml");
const FAKTUROID: &[u8] =
    include_bytes!("../../../packages/fixtures/data/imports/fakturoid-faktury.csv");

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
fn fakturoid_totals_find_their_rate_and_credit_notes_wait() {
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
