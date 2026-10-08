//! Improvement wave 14: a draft can be changed until it's issued. The
//! editor gets it back as typed, and changes are checked like a new draft.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::InvoiceDraftDto;

fn scripted() -> InvoiceDraftDto {
    serde_json::from_value(skyla_app::recordings::scripted_draft()).unwrap()
}

#[test]
fn a_draft_comes_back_as_typed_and_changes_are_checked() {
    let core = Core::demo().unwrap();
    let typed = scripted();
    let saved = core.create_invoice_draft(&typed).unwrap();
    let back = core.invoice_draft(saved.id).unwrap();
    assert_eq!(back, typed, "the editor sees what was typed");

    let mut changed = back.clone();
    changed.lines[0].quantity = "10,5".into();
    changed.lines.remove(1);
    changed.due_days = 30;
    let updated = core.update_invoice_draft(saved.id, &changed).unwrap();
    assert_eq!(updated.id, saved.id);
    assert_eq!(updated.status, "draft");
    // 10,5 h at 1 450,00 is 15 225,00 before VAT.
    assert_eq!(updated.base.minor, 1_522_500);
    assert_eq!(updated.lines.len(), 1);
    assert_eq!(core.invoice_draft(saved.id).unwrap(), changed);

    let mut bad = changed.clone();
    bad.lines[0].unit_price = "abc".into();
    bad.client = "Nobody".into();
    let refused = core
        .update_invoice_draft(saved.id, &bad)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("isn't a known customer"), "{refused}");
    assert!(refused.contains("unit price"), "{refused}");
    assert_eq!(
        core.invoice_draft(saved.id).unwrap(),
        changed,
        "a refused change leaves the draft as it was"
    );
}

#[test]
fn an_issued_invoice_cant_be_edited() {
    let core = Core::demo().unwrap();
    let saved = core.create_invoice_draft(&scripted()).unwrap();
    core.issue_invoice(saved.id, "2026-10-07").unwrap();
    let read = core.invoice_draft(saved.id).unwrap_err().to_string();
    assert!(read.contains("credit note"), "{read}");
    assert!(core.update_invoice_draft(saved.id, &scripted()).is_err());
}
