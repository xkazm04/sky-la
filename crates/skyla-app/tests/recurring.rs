//! Improvement wave 10: recurring invoices in real books. A template made
//! from the editor's draft catches up on what's due, runs again when the
//! books open (never twice for an occurrence), can issue automatically,
//! and can be paused.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::{ClientDto, EntitySetupDto, InvoiceDraftDto, RecurringDraftDto};
use skyla_app::session::Gate;

const PASS: &str = "a long passphrase for the books";

fn books() -> (tempfile::TempDir, Gate, Core) {
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
            PASS,
        )
        .unwrap();
    (dir, gate, core)
}

fn recurring(auto_issue: bool) -> RecurringDraftDto {
    let mut draft: InvoiceDraftDto =
        serde_json::from_value(skyla_app::recordings::scripted_draft()).unwrap();
    draft.client = String::new();
    draft.new_client = Some(ClientDto {
        name: "Lesní ateliér s.r.o.".into(),
        ico: Some("26965313".into()),
        dic: Some("CZ26965313".into()),
        address: Some("Jasmínová 12, 106 00 Praha 10".into()),
    });
    draft.lines.truncate(1);
    draft.lines[0].description = "Správa webu · {month}".into();
    RecurringDraftDto {
        name: "Správa webu".into(),
        draft,
        frequency: "monthly".into(),
        interval: 1,
        start: "2026-09-07".into(),
        auto_issue,
    }
}

#[test]
fn a_template_catches_up_and_never_runs_twice() {
    let (_dir, gate, core) = books();
    let templates = core.create_recurring(&recurring(false)).unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].next.as_deref(), Some("2026-11-07"));
    let drafts: Vec<_> = core
        .invoices()
        .unwrap()
        .into_iter()
        .filter(|i| i.status == "draft")
        .collect();
    assert_eq!(drafts.len(), 2, "September and October");
    assert!(
        drafts
            .iter()
            .any(|d| d.lines[0].description == "Správa webu · září 2026")
    );
    drop(core);
    let core = gate.unlock(PASS, false).unwrap();
    assert_eq!(
        core.invoices()
            .unwrap()
            .iter()
            .filter(|i| i.status == "draft")
            .count(),
        2,
        "reopening makes nothing twice"
    );
}

#[test]
fn auto_issue_numbers_each_occurrence_and_a_paused_template_waits() {
    let (_dir, _gate, core) = books();
    let templates = core.create_recurring(&recurring(true)).unwrap();
    let mut numbers: Vec<_> = core
        .invoices()
        .unwrap()
        .into_iter()
        .filter_map(|i| i.number)
        .collect();
    numbers.sort();
    assert_eq!(
        numbers,
        ["2026-001", "2026-002"],
        "September's first, then October's"
    );
    let paused = core.set_recurring_active(templates[0].id, false).unwrap();
    assert!(!paused[0].active);
}

#[test]
fn bad_schedules_are_refused_with_the_drafts_problems() {
    let (_dir, _gate, core) = books();
    let mut r = recurring(false);
    r.frequency = "daily".into();
    r.interval = 0;
    r.draft.lines[0].unit_price = "abc".into();
    let refused = core.create_recurring(&r).unwrap_err().to_string();
    for p in ["daily", "1 to 12", "unit price"] {
        assert!(refused.contains(p), "{p}: {refused}");
    }
    assert!(core.recurring_templates().unwrap().is_empty());
}

#[test]
fn the_demo_creates_templates_but_doesnt_run_them() {
    let core = Core::demo().unwrap();
    let before = core.invoices().unwrap().len();
    let mut r = recurring(false);
    r.draft.client = "Northwind Traders s.r.o.".into();
    r.draft.new_client = None;
    let templates = core.create_recurring(&r).unwrap();
    assert!(templates.iter().any(|t| t.name == "Správa webu"));
    assert_eq!(core.invoices().unwrap().len(), before);
}
