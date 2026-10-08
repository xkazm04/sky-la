//! Improvement wave 11: approving the inbox's postings. The user's approval
//! posts the entry through the kernel exactly as proposed; a proposal about
//! a bank line books that line; anything without an entry is refused.

#![allow(clippy::unwrap_used)]

use serde_json::json;
use skyla_app::Core;

fn ids(core: &Core) -> Vec<String> {
    core.proposals()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

fn posted(core: &Core) -> usize {
    core.journal("2000-01-01", "2100-12-31")
        .unwrap()
        .iter()
        .filter(|e| e.posted_seq.is_some())
        .count()
}

#[test]
fn approving_a_rule_proposal_books_its_bank_line() {
    let core = Core::demo().unwrap();
    let before = posted(&core);
    assert!(ids(&core).contains(&"p-google".to_owned()));
    let after = core.approve_proposals(&["p-google".into()]).unwrap();
    assert!(
        after.iter().all(|p| p.id != "p-google"),
        "it leaves the inbox"
    );
    assert_eq!(posted(&core), before + 1);

    let line = core
        .bank_statement()
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.id == "s1-2")
        .unwrap();
    assert_eq!(line.status, "booked");
    assert_eq!(
        line.booked_as.as_deref(),
        Some("Approved: Google Ireland · reverse-charge posting")
    );
    let entry = core
        .journal("2026-10-03", "2026-10-03")
        .unwrap()
        .into_iter()
        .find(|e| Some(e.id) == line.entry_id)
        .unwrap();
    assert_eq!(entry.source_kind, "rule");
    assert_eq!(entry.approved_by.as_deref(), Some("user"));
    assert!(entry.memo.starts_with("Google Workspace"));
    // Both sides of the reverse charge, with the pack's VAT.
    assert_eq!(
        entry
            .lines
            .iter()
            .filter(|l| l.vat_code.as_deref() == Some("RC21S"))
            .count(),
        3
    );
    assert!(core.integrity().unwrap().chain_intact);

    let again = core.approve_proposals(&["p-google".into()]).unwrap_err();
    assert!(again.to_string().contains("isn't in the inbox"), "{again}");
    assert_eq!(posted(&core), before + 1);
}

#[test]
fn approving_several_posts_each_and_a_refusal_names_the_item() {
    let core = Core::demo().unwrap();
    let before = posted(&core);
    let certain: Vec<String> = core
        .proposals()
        .unwrap()
        .into_iter()
        .filter(|p| p.kind == "posting" && p.confidence.as_deref() == Some("certain"))
        .map(|p| p.id)
        .collect();
    assert!(certain.len() >= 2, "{certain:?}");
    let after = core.approve_proposals(&certain).unwrap();
    assert_eq!(posted(&core), before + certain.len());
    assert!(after.iter().all(|p| !certain.contains(&p.id)));

    let advice = after.iter().find(|p| p.kind == "advice").unwrap();
    let refused = core
        .approve_proposals(std::slice::from_ref(&advice.id))
        .unwrap_err();
    assert!(
        refused.to_string().contains("no entry to post"),
        "{refused}"
    );
    assert!(core.approve_proposals(&[]).is_err());
    assert!(core.approve_proposals(&["nope".into()]).is_err());
    assert_eq!(posted(&core), before + certain.len());
}

#[test]
fn an_advisors_entry_posts_as_proposed_and_its_id_isnt_reused() {
    let core = Core::demo().unwrap();
    let filed = core
        .call_tool(
            "propose_entry",
            &json!({
                "date": "2026-09-30",
                "memo": "Accrued hosting for September",
                "reason": "The September invoice arrives in October.",
                "lines": [
                    { "account": "518", "amount_minor": 120_000 },
                    { "account": "321", "amount_minor": -120_000 }
                ]
            }),
        )
        .unwrap();
    let id = filed["proposal"].as_str().unwrap().to_owned();
    let before = posted(&core);
    let after = core.approve_proposals(std::slice::from_ref(&id)).unwrap();
    assert!(after.iter().all(|p| p.id != id));
    assert_eq!(posted(&core), before + 1);
    let entry = core
        .journal("2026-09-30", "2026-09-30")
        .unwrap()
        .into_iter()
        .find(|e| e.memo == "Accrued hosting for September")
        .unwrap();
    assert_eq!(entry.source_kind, "advisor");
    assert_eq!(entry.source_ref.as_deref(), Some(id.as_str()));
    assert_eq!(entry.approved_by.as_deref(), Some("user"));

    let next = core
        .call_tool(
            "ask_user",
            &json!({ "question": "Do you have children you claim?", "why": "The child bonus." }),
        )
        .unwrap();
    assert_ne!(next["proposal"].as_str().unwrap(), id);
}

#[test]
fn advice_can_be_dismissed_and_postings_cannot() {
    let core = Core::demo().unwrap();
    let advice: Vec<String> = core
        .proposals()
        .unwrap()
        .into_iter()
        .filter(|p| p.kind == "advice")
        .map(|p| p.id)
        .collect();
    assert!(advice.iter().any(|id| id.starts_with("finding-")));
    let after = core.dismiss_proposal(&advice[0]).unwrap();
    assert!(after.iter().all(|p| p.id != advice[0]));
    assert!(core.dismiss_proposal(&advice[0]).is_err(), "gone already");
    let posting = core.dismiss_proposal("p-alza").unwrap_err();
    assert!(posting.to_string().contains("only advice"), "{posting}");

    let asked = core
        .call_tool(
            "ask_user",
            &json!({ "question": "Is the car used privately?", "why": "It decides the VAT deduction." }),
        )
        .unwrap();
    let id = asked["proposal"].as_str().unwrap();
    let after = core.dismiss_proposal(id).unwrap();
    assert!(after.iter().all(|p| p.id != id));
}

#[test]
fn an_advisors_categorisation_is_approved_into_the_line_it_names() {
    let core = Core::demo().unwrap();
    let filed = core
        .call_tool(
            "propose_categorisation",
            &json!({ "line": "s1-3", "account": "491", "reason": "Owner drawings, like the last four." }),
        )
        .unwrap();
    let id = filed["proposal"].as_str().unwrap().to_owned();
    let item = core
        .proposals()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(item.kind, "posting");
    let entry = item.entry.unwrap();
    assert!(entry.balanced);
    assert_eq!(entry.lines[0].account, "491");
    assert!(entry.lines[0].debit.is_some(), "money out is a debit");

    let after = core.approve_proposals(std::slice::from_ref(&id)).unwrap();
    // The line is booked, so the demo's own proposal about it leaves too.
    assert!(after.iter().all(|p| p.id != id && p.id != "p-atm"));
    let line = core
        .bank_statement()
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.id == "s1-3")
        .unwrap();
    assert_eq!(line.status, "booked");
    let refused = core.call_tool(
        "propose_categorisation",
        &json!({ "line": "s1-3", "account": "518", "reason": "x" }),
    );
    assert!(refused.unwrap_err().to_string().contains("booked already"));
}
