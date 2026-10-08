//! WP-25 acceptance (the core half): no advisor tool can write or post.
//! Every tool is called, proposals included, and the ledger is unchanged:
//! the same entries, drafts, postings and hash-chain head.

#![allow(clippy::unwrap_used)]

use serde_json::{Value, json};
use skyla_app::Core;
use skyla_app::{ToolKind, tool_specs};

fn args_for(name: &str) -> Value {
    match name {
        "get_period_summary" | "get_vat_return" | "get_cash_basis" => {
            json!({ "from": "2026-07-01", "to": "2026-09-30" })
        }
        "get_rule_value" => json!({ "key": "vat.rate.standard", "on": "2026-09-30" }),
        "list_obligations" => json!({ "year": 2026 }),
        "run_scenario" => json!({ "projection": null }),
        "propose_entry" => json!({
            "date": "2026-09-30",
            "memo": "Accrued hosting for September",
            "reason": "The September invoice arrives in October.",
            "lines": [
                { "account": "518", "amount_minor": 120_000 },
                { "account": "321", "amount_minor": -120_000 }
            ]
        }),
        "propose_categorisation" => {
            json!({ "line": "s1-3", "account": "518", "reason": "Same counterparty as last month." })
        }
        "propose_finding" => {
            json!({ "title": "Subcontracting is 42 % of costs", "detail": "Up from 30 %.", "cites": ["pl:2026-Q3:518"] })
        }
        "ask_user" => {
            json!({ "question": "Do you have children you claim?", "why": "It decides the child bonus." })
        }
        other => panic!("no arguments for {other}"),
    }
}

/// Everything about the ledger a write would change.
fn fingerprint(core: &Core) -> Value {
    let integrity = core.integrity().unwrap();
    let journal = core.journal("2000-01-01", "2100-12-31").unwrap();
    json!({
        "head": integrity.head,
        "checked": integrity.entries_checked,
        "journal": serde_json::to_value(&journal).unwrap(),
        "invoices": serde_json::to_value(core.invoices().unwrap()).unwrap(),
        "bank": serde_json::to_value(core.bank_statement().unwrap()).unwrap(),
    })
}

#[test]
fn no_tool_can_write_or_post() {
    let core = Core::demo().unwrap();
    let before = fingerprint(&core);
    let inbox_before = core.proposals().unwrap().len();
    for spec in tool_specs() {
        let result = core.call_tool(spec.name, &args_for(spec.name));
        assert!(result.is_ok(), "{}: {:?}", spec.name, result.err());
    }
    assert_eq!(fingerprint(&core), before, "a tool changed the books");
    let proposals = tool_specs()
        .iter()
        .filter(|s| s.kind == ToolKind::Propose)
        .count();
    assert_eq!(
        core.proposals().unwrap().len(),
        inbox_before + proposals,
        "proposals wait in the inbox"
    );
}

#[test]
fn every_tool_reads_computes_or_proposes() {
    const WRITE_WORDS: [&str; 10] = [
        "post", "write", "delete", "update", "set", "create", "issue", "book", "import", "install",
    ];
    let specs = tool_specs();
    assert_eq!(specs.len(), 10);
    for s in &specs {
        assert!(
            !WRITE_WORDS.iter().any(|w| s.name.starts_with(w)),
            "{} sounds like a write",
            s.name
        );
        assert_eq!(s.input_schema["type"], "object", "{}", s.name);
    }
}

#[test]
fn a_bad_proposal_is_refused_by_the_kernel_and_files_nothing() {
    let core = Core::demo().unwrap();
    let n = core.proposals().unwrap().len();
    let unbalanced = json!({
        "date": "2026-09-30", "memo": "x", "reason": "y",
        "lines": [{ "account": "518", "amount_minor": 100 }, { "account": "321", "amount_minor": -99 }]
    });
    let err = core
        .call_tool("propose_entry", &unbalanced)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("the kernel refused it"), "{err}");
    let closed = json!({
        "date": "2025-12-31", "memo": "x", "reason": "y",
        "lines": [{ "account": "518", "amount_minor": 100 }, { "account": "321", "amount_minor": -100 }]
    });
    assert!(core.call_tool("propose_entry", &closed).is_err());
    assert!(
        core.call_tool("post_entry", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("there is no tool post_entry")
    );
    assert!(
        core.call_tool(
            "propose_finding",
            &json!({ "title": "t", "detail": "d", "cites": [] })
        )
        .is_err()
    );
    assert_eq!(core.proposals().unwrap().len(), n);
}
