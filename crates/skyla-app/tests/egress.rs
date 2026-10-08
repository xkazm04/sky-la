//! WP-26 acceptance (the core half): the demo's register is built by the
//! real gate, holds the exact bytes sent, and policies are per task.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;

#[test]
fn the_register_holds_exactly_what_the_gate_let_through() {
    let core = Core::demo().unwrap();
    let runs = core.egress_register().unwrap();
    assert_eq!(runs.len(), 3);
    assert!(runs.windows(2).all(|w| w[0].at >= w[1].at), "newest first");
    assert!(runs.iter().all(|r| r.intact));
    let bank = runs.iter().find(|r| r.id == "run-2026-10-04-02").unwrap();
    assert!(
        bank.redacted
            .iter()
            .any(|w| w.contains("counterpartyAccount")),
        "{:?}",
        bank.redacted
    );
    let lines = core.bank_statement().unwrap().lines;
    for r in &runs {
        let p = core.egress_payload(&r.id).unwrap();
        assert_eq!(p.bytes, r.bytes_sent);
        for l in &lines {
            if let Some(acct) = &l.counterparty_account {
                assert!(!p.text.contains(acct.as_str()), "{acct} in {}", r.id);
            }
            assert!(
                !p.text.contains(l.counterparty.as_str()),
                "{} in {}",
                l.counterparty,
                r.id
            );
        }
    }
    // A second core gives byte-identical payloads: replay is deterministic.
    let again = Core::demo().unwrap();
    for r in &runs {
        assert_eq!(
            again.egress_payload(&r.id).unwrap(),
            core.egress_payload(&r.id).unwrap()
        );
    }
}

#[test]
fn policies_are_per_task_and_default_to_ask() {
    let core = Core::demo().unwrap();
    assert!(core.egress_policies().iter().all(|p| p.policy == "ask"));
    let after = core.set_egress_policy("tax.scenarios", "never").unwrap();
    assert_eq!(
        after
            .iter()
            .find(|p| p.task == "tax.scenarios")
            .unwrap()
            .policy,
        "never"
    );
    assert!(
        core.set_egress_policy("tax.scenarios", "sometimes")
            .is_err()
    );
    assert!(core.set_egress_policy("nope", "always").is_err());
}
