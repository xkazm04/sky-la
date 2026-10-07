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
fn invoices_take_amounts_from_entries_and_payments_from_settlements() {
    let invoices = core().invoices().unwrap();
    let get = |n: &str| invoices.iter().find(|i| i.number == n).unwrap();

    let brno = get("2026-102");
    assert_eq!(brno.status, "overdue");
    assert_eq!(brno.days_overdue, Some(13));
    assert_eq!(
        (brno.gross.minor, brno.paid.minor, brno.open.minor),
        (10_309_200, 5_000_000, 5_309_200)
    );

    let northwind = get("2026-114");
    assert_eq!(northwind.status, "paid");
    assert_eq!(northwind.paid_on.as_deref(), Some("2026-09-29"));
    assert_eq!(
        (northwind.base.minor, northwind.vat.minor),
        (7_000_000, 1_470_000)
    );
    assert_eq!(northwind.lines[0].base.minor, 7_000_000);

    let draft = get("2026-121");
    assert_eq!(draft.status, "draft");
    assert_eq!(
        (draft.base.minor, draft.vat.minor, draft.gross.minor),
        (8_000_000, 1_680_000, 9_680_000)
    );
    assert_eq!(get("2026-122").scheduled_for.as_deref(), Some("2026-11-01"));
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
