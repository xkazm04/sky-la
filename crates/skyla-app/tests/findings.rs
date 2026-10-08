//! WP-28 acceptance: the detectors reproduce the canvas's findings from the
//! golden books (subcontracting up 42 %), each citing its entries, and
//! "explain this" cites only entries behind the figure.

#![allow(clippy::unwrap_used)]

use serde_json::{Value, json};
use skyla_advisor::Fake;
use skyla_app::Core;
use skyla_app::demo::EXPLAIN_518_TRANSCRIPT;
use skyla_app::dto::ExplainTargetDto;

fn find<'a>(all: &'a [skyla_app::dto::FindingDto], id: &str) -> &'a skyla_app::dto::FindingDto {
    all.iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no finding {id}"))
}

#[test]
fn subcontracting_rose_42_percent_and_cites_its_six_invoices() {
    let core = Core::demo().unwrap();
    let all = core.financial_findings().unwrap();
    let f = find(&all, "subcontracting");
    // 74 700 → 106 100 Kč: (106 100 − 74 700) / 74 700 = 42,03 %.
    assert_eq!(f.title, "Subcontracting rose 42 % on the previous quarter");
    assert_eq!(
        f.figures.iter().map(|(_, m)| m.minor).collect::<Vec<_>>(),
        [7_470_000, 10_610_000, 3_140_000]
    );
    assert_eq!(f.percents, [("Change".to_owned(), "42".to_owned())]);
    // The six received invoices of the three subcontractors.
    let journal = core.journal("2026-04-01", "2026-09-30").unwrap();
    let memos: Vec<String> = f
        .cites
        .iter()
        .map(|id| journal.iter().find(|e| e.id == *id).unwrap().memo.clone())
        .collect();
    assert_eq!(memos.len(), 6);
    assert!(
        memos.iter().all(|m| m.starts_with("Přijatá faktura")),
        "{memos:?}"
    );
}

#[test]
fn each_detector_finds_what_the_books_hold() {
    let core = Core::demo().unwrap();
    let all = core.financial_findings().unwrap();
    // Hourly rates: 28 000 / 56 h = 500 → 46 000 / 80 h = 575 (+15 %).
    let kv = find(&all, "rate:Kvasnička Dev s.r.o.");
    assert_eq!(
        kv.figures.iter().map(|(_, m)| m.minor).collect::<Vec<_>>(),
        [50_000, 57_500]
    );
    assert_eq!(kv.percents[0].1, "15");
    // 31 200 / 52 = 600 → 38 500 / 55 = 700: +16,7 % → 17 %.
    assert_eq!(find(&all, "rate:Marek Horák").percents[0].1, "17");
    // Margin: Northwind billed 140 000 + 70 000, 46 000 subcontracted → 78 %.
    let nw = find(&all, "margin:Northwind Traders s.r.o.");
    assert_eq!(nw.figures[2].1.minor, 16_400_000);
    assert_eq!(nw.percents[0].1, "78");
    // The duplicated Figma charge and its refund.
    let dup = all
        .iter()
        .find(|f| f.id.starts_with("duplicate:figma"))
        .unwrap();
    assert_eq!(dup.title, "Figma charged twice, and refunded");
    // Subscriptions 13 500 → 14 700: +8,9 % → 9 %.
    assert_eq!(find(&all, "subscriptions").percents[0].1, "9");
    // Late payers, from paid dates against due dates.
    assert!(all.iter().any(|f| f.detector == "late_payer"));
    // Plenty of cash: no runway finding.
    assert!(!all.iter().any(|f| f.detector == "runway"));
    // Everything cites entries except the runway.
    assert!(all.iter().all(|f| !f.cites.is_empty()));
}

#[test]
fn the_runway_detector_warns_when_cash_is_short() {
    use skyla_app::core_findings::{Books, DetectorSettings, Posting, runway};
    let b = Books {
        expenses: (0..3)
            .map(|i| Posting {
                entry: i,
                date: format!("2026-0{}-15", 7 + i),
                account: "518".into(),
                minor: 5_000_000,
                vendor: None,
                hours_hundredths: None,
                client: None,
                memo: "x".into(),
            })
            .collect(),
        sales: Vec::new(),
        cash_minor: 12_000_000,
    };
    let f = runway(
        &b,
        ("2026-07-01", "2026-09-30"),
        DetectorSettings::default(),
    )
    .unwrap();
    // 120 000 Kč cash / 50 000 Kč a month = 2,4 months.
    assert_eq!(f.title, "Cash covers 2,4 months of expenses");
    let rich = Books {
        cash_minor: 40_000_000,
        ..b
    };
    assert!(
        runway(
            &rich,
            ("2026-07-01", "2026-09-30"),
            DetectorSettings::default()
        )
        .is_none()
    );
}

#[test]
fn actionable_findings_wait_in_the_inbox() {
    let core = Core::demo().unwrap();
    let inbox = core.proposals().unwrap();
    let f = inbox
        .iter()
        .find(|p| p.id == "finding-subcontracting")
        .unwrap();
    assert!(f.reasons.iter().any(|r| r.starts_with("Entries: #")));
    assert!(
        inbox
            .iter()
            .any(|p| p.id.starts_with("finding-duplicate:figma"))
    );
    assert!(
        !inbox.iter().any(|p| p.id.starts_with("finding-rate:")),
        "the rest stay on the Advisors screen"
    );
}

fn target() -> ExplainTargetDto {
    ExplainTargetDto {
        kind: "account".into(),
        account: Some("518".into()),
        from: Some("2026-07-01".into()),
        to: Some("2026-09-30".into()),
        entry: None,
    }
}

#[test]
fn explain_this_cites_entry_ids_behind_the_figure() {
    let core = Core::demo().unwrap();
    assert_eq!(
        core.explain(&target(), false).unwrap().status,
        "needs_confirmation"
    );
    let e = core.explain(&target(), true).unwrap();
    assert_eq!(e.status, "accepted", "{:?}", e.problems);
    assert_eq!(e.cites, [34, 35, 36, 39, 40, 41, 42, 43]);
    let behind: Vec<i64> = e.entries.iter().map(|x| x.id).collect();
    assert!(e.cites.iter().all(|c| behind.contains(c)));
    assert!(e.text.unwrap().contains("120 800 Kč"));
    assert!(e.grounded >= 10);
    // The register has it, with the memos' supplier names pseudonymised.
    let run = core.egress_register().unwrap()[0].clone();
    let payload = core.egress_payload(&run.id).unwrap().text;
    assert!(
        !payload.contains("Kvasnička") && !payload.contains("Pixelfarm"),
        "{payload}"
    );
}

fn answer_with(f: impl Fn(&mut Value)) -> String {
    EXPLAIN_518_TRANSCRIPT
        .lines()
        .map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap();
            if v["type"] == "result" {
                f(&mut v["structured_output"]);
            }
            v.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_explanation_citing_other_entries_or_figures_is_rejected() {
    let core = Core::demo().unwrap();
    // A citation from outside the figure.
    core.replace_provider(Box::new(Fake::new().with(
        "explain.figure",
        &answer_with(|a| {
            a["cites"] = json!([34, 3]);
        }),
    )));
    let e = core.explain(&target(), true).unwrap();
    assert_eq!(e.status, "rejected");
    assert!(
        e.problems.iter().any(|p| p.contains("#3,")),
        "{:?}",
        e.problems
    );
    // A figure it computed itself (the three subcontractors' sum).
    core.replace_provider(Box::new(Fake::new().with(
        "explain.figure",
        &answer_with(|a| {
            let t = a["explanation"].as_str().unwrap().replace(
                "Most of it",
                "Subcontractors billed 106 100 Kč in all. Most of it",
            );
            a["explanation"] = json!(t);
        }),
    )));
    let e = core.explain(&target(), true).unwrap();
    assert_eq!(e.status, "rejected");
    assert!(e.problems[0].starts_with("106 100 Kč"), "{:?}", e.problems);
    // An explanation that cites nothing.
    core.replace_provider(Box::new(Fake::new().with(
        "explain.figure",
        &answer_with(|a| {
            a["cites"] = json!([]);
        }),
    )));
    assert_eq!(core.explain(&target(), true).unwrap().status, "rejected");
}
