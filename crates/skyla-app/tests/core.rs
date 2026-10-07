//! WP-09: the demo core answers every command from the engine, and the
//! committed IPC recordings are exactly what it answers today.

use std::path::PathBuf;

use skyla_app::Core;
use skyla_app::recordings::{RECORDINGS_PATH, Recording, record};

fn core() -> Core {
    Core::demo().expect("the demo core opens")
}

#[test]
fn statements_come_from_the_ledger() {
    let core = core();
    let q3 = core.profit_and_loss("2026-07-01", "2026-09-30").unwrap();
    assert_eq!(q3.profit.minor, 28_035_000);
    assert_eq!(q3.profit.currency, "CZK");
    assert!(core.balance_sheet("2026-09-30").unwrap().balances);
    let integrity = core.integrity().unwrap();
    assert!(integrity.chain_intact && integrity.balanced);
    assert_eq!(integrity.entries_checked, 52);
    let labels: Vec<String> = core
        .periods()
        .unwrap()
        .into_iter()
        .map(|p| p.label)
        .collect();
    assert_eq!(labels, ["Q2 2026", "Q3 2026", "Q4 2026"]);
}

#[test]
fn invoices_come_from_the_invoicing_module_and_payments_from_settlements() {
    let invoices = core().invoices().unwrap();
    let get = |n: &str| {
        invoices
            .iter()
            .find(|i| i.number.as_deref() == Some(n))
            .unwrap()
    };

    let brno = get("2026-102");
    assert_eq!(brno.status, "overdue");
    assert_eq!(brno.days_overdue, Some(13));
    assert_eq!(
        (brno.gross.minor, brno.paid.minor, brno.open.minor),
        (10_309_200, 5_000_000, 5_309_200)
    );
    assert_eq!(brno.pack.as_deref(), Some("cz-2026@2026.1"));

    let northwind = get("2026-114");
    assert_eq!(northwind.status, "paid");
    assert_eq!(northwind.paid_on.as_deref(), Some("2026-09-29"));
    assert_eq!(
        (northwind.base.minor, northwind.vat.minor),
        (7_000_000, 1_470_000)
    );
    assert_eq!(northwind.lines[0].base.minor, 7_000_000);
    assert_eq!(northwind.lines[0].vat.minor, 1_470_000);

    // Drafts have no number until they're issued; they come first.
    let drafts: Vec<_> = invoices.iter().take(2).collect();
    assert!(drafts.iter().all(|i| i.number.is_none()));
    let draft = drafts.iter().find(|i| i.status == "draft").unwrap();
    assert_eq!(
        (draft.base.minor, draft.vat.minor, draft.gross.minor),
        (8_000_000, 1_680_000, 9_680_000)
    );
    let scheduled = drafts.iter().find(|i| i.status == "scheduled").unwrap();
    assert_eq!(scheduled.scheduled_for.as_deref(), Some("2026-11-01"));
    assert_eq!(invoices.len(), 9);
}

#[test]
fn the_bank_import_ties_out_against_the_ledger() {
    let statement = core().bank_statement().unwrap();
    assert_eq!(statement.opening.minor, 88_545_350);
    assert!(statement.ties_out, "{statement:#?}");
    assert_eq!(statement.closing.minor, statement.reported_closing.minor);
    assert_eq!(
        statement.lines.first().map(|l| l.date.as_str()),
        Some("2026-10-06")
    );
}

#[test]
fn every_proposed_entry_balances_and_its_vat_is_the_engines() {
    let proposals = core().proposals().unwrap();
    assert_eq!(proposals.len(), 8);
    for p in proposals
        .iter()
        .filter_map(|p| p.entry.as_ref().map(|e| (p, e)))
    {
        assert!(p.1.balanced, "{} doesn't balance", p.0.id);
    }
    let google = proposals.iter().find(|p| p.id == "p-google").unwrap();
    let vat_line = google
        .entry
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .find(|l| l.account == "343" && l.debit.is_some())
        .unwrap();
    assert_eq!(vat_line.debit.as_ref().unwrap().minor, 24_211);
    assert_eq!(google.amount.as_ref().unwrap().minor, -115_292);
}

#[test]
fn the_committed_recordings_match_the_core() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(RECORDINGS_PATH);
    let current = record(&core());
    assert!(
        current.iter().all(|r| !r.is_error),
        "a canonical request failed: {:#?}",
        current.iter().find(|r| r.is_error)
    );
    let json = serde_json::to_string_pretty(&current).unwrap() + "\n";
    if std::env::var_os("UPDATE_RECORDINGS").is_some() {
        std::fs::write(&path, &json).unwrap();
        return;
    }
    let committed: Vec<Recording> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
            .unwrap_or_default();
    assert!(
        committed == current,
        "{RECORDINGS_PATH} is stale; regenerate it with `just recordings`"
    );
}

#[test]
fn the_vat_return_maps_the_ledger_onto_form_rows_with_the_pack() {
    let core = core();
    let sep = core.vat_return("2026-09-01", "2026-09-30").unwrap();
    let rows: Vec<(&str, i64, i64)> = sep
        .rows
        .iter()
        .map(|r| (r.row.as_str(), r.base.minor, r.tax.minor))
        .collect();
    assert_eq!(
        rows,
        [
            ("1", 7_000_000, 1_470_000),
            ("5", 1_231_860, 258_691),
            ("40", 2_408_300, 505_743),
            ("43", 1_231_860, 258_691)
        ]
    );
    // Output 14 700,00 + 2 586,91; input 5 057,43 + 2 586,91; payable 9 642,57.
    assert_eq!(
        (sep.output_tax.minor, sep.input_tax.minor, sep.payable.minor),
        (1_728_691, 764_434, 964_257)
    );
    assert!(sep.unmapped.is_empty());
    // 25 October 2026 is a Sunday.
    assert_eq!(sep.due_on, "2026-10-26");
    assert_eq!(sep.pack, "cz-2026@2026.1");
    assert_eq!(sep.pack_review, "draft");

    // August: Acme 33 600 + Studio Brno 17 892 out; Kvasnička 9 660 + monitor 2 079 in.
    let aug = core.vat_return("2026-08-01", "2026-08-31").unwrap();
    assert_eq!(aug.payable.minor, 3_975_300);
    assert_eq!(aug.due_on, "2026-09-25");
}

#[test]
fn rates_rounding_and_deadlines_come_from_the_rule_pack() {
    let core = core();
    let invoices = core.invoices().unwrap();
    let line = &invoices
        .iter()
        .find(|i| i.number.as_deref() == Some("2026-114"))
        .unwrap()
        .lines[0];
    assert_eq!(
        (line.vat_code.as_str(), line.vat_rate_percent.as_str()),
        ("OUT21", "21")
    );
    let dph = core
        .proposals()
        .unwrap()
        .into_iter()
        .find(|p| p.id == "p-dph-september")
        .unwrap();
    assert_eq!(dph.due_on.as_deref(), Some("2026-10-26"));

    let pack = core.rule_pack();
    assert_eq!(pack.provenance, "cz-2026@2026.1");
    assert_eq!(pack.holidays, 13);
    let reduced = pack
        .values
        .iter()
        .find(|v| v.key == "vat.rate.reduced")
        .unwrap();
    assert_eq!(reduced.value, "12");
    assert!(reduced.citation.starts_with("Zákon č. 235/2004 Sb."));
    assert!(pack.values.iter().all(|v| v.url.starts_with("https://")));
    assert!(!pack.omitted.is_empty());
}
