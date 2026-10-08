//! Improvement wave 15: a payment reminder is drafted by the core, sent by
//! the user from their own mail, and recorded once.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;

#[test]
fn the_due_reminder_is_recorded_once_and_leaves_the_queue() {
    let core = Core::demo().unwrap();
    let queue = core.dunning_queue("2026-10-07").unwrap();
    let notice = queue.iter().find(|n| n.number == "2026-102").unwrap();
    assert_eq!((notice.step, notice.tone.as_str()), (1, "friendly"));
    assert!(notice.body_cs.contains("2026-102"));
    assert!(core.reminders_sent(notice.document_id).unwrap().is_empty());

    let wrong = core.record_reminder(notice.document_id, 2).unwrap_err();
    assert!(wrong.to_string().contains("isn't due"), "{wrong}");

    let after = core.record_reminder(notice.document_id, 1).unwrap();
    assert!(after.iter().all(|n| n.document_id != notice.document_id));
    let sent = core.reminders_sent(notice.document_id).unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!((sent[0].step, sent[0].sent_on.as_str()), (1, "2026-10-07"));
    assert!(core.record_reminder(notice.document_id, 1).is_err());
}
