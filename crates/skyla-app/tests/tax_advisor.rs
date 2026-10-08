//! WP-27 acceptance: the tax advisor's answers are checked against the
//! engine. A run with an injected wrong number is rejected; the eval set
//! (30 Czech cases) passes on the Fake provider, and every case's answer is
//! rejected again once any one of its figures is changed.

#![allow(clippy::unwrap_used)]

use serde_json::{Value, json};
use skyla_advisor::Fake;
use skyla_app::Core;
use skyla_app::demo::{TAX_BOOKS_TRANSCRIPT, TAX_REVIEW_TRANSCRIPT};
use skyla_app::dto::{TaxProjectionDto, TaxScenariosDto};
use skyla_app::evals::TAX_CASES as CASES;

fn review() -> TaxProjectionDto {
    TaxProjectionDto {
        income: "1 571 000".into(),
        expenses: "383 200".into(),
        flat_rate: Some("trade".into()),
        purchase_description: "Laptop".into(),
        purchase_price: "60 000".into(),
    }
}

/// A transcript whose final answer is `answer`, in the recorded format.
fn with_answer(template: &str, answer: &Value) -> String {
    template
        .lines()
        .map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap();
            if v["type"] == "result" {
                v["structured_output"] = answer.clone();
                v["result"] = answer["explanation"].clone();
            }
            v.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn answer_of(template: &str) -> Value {
    template
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["type"] == "result")
        .unwrap()["structured_output"]
        .clone()
}

#[test]
fn the_demo_answers_are_accepted_and_filed_for_review() {
    let core = Core::demo().unwrap();
    let before = core.proposals().unwrap().len();
    let runs = core.egress_register().unwrap().len();
    let ask = core.run_tax_advisor(None, false).unwrap();
    assert_eq!(
        ask.status, "needs_confirmation",
        "the default policy asks first"
    );
    assert_eq!(core.egress_register().unwrap().len(), runs, "nothing ran");

    let books = core.run_tax_advisor(None, true).unwrap();
    assert_eq!(books.status, "accepted", "{:?}", books.problems);
    assert!(books.grounded >= 10, "{}", books.grounded);
    assert_eq!(books.recommendation.as_deref(), Some("flat_rate.trade"));
    let review = core.run_tax_advisor(Some(&review()), true).unwrap();
    assert_eq!(review.status, "accepted", "{:?}", review.problems);
    assert!(review.explanation.unwrap().contains("206 759 Kč"));
    assert_eq!(core.proposals().unwrap().len(), before + 2);
    let register = core.egress_register().unwrap();
    assert_eq!(register.len(), runs + 2, "every run is recorded");
    assert!(register.iter().all(|r| r.intact));
}

#[test]
fn an_injected_wrong_number_is_rejected() {
    let core = Core::demo().unwrap();
    let mut answer = answer_of(TAX_REVIEW_TRANSCRIPT);
    let text = answer["explanation"]
        .as_str()
        .unwrap()
        .replace("206 759 Kč", "206 760 Kč");
    answer["explanation"] = json!(text);
    core.replace_provider(Box::new(Fake::new().with(
        "tax.scenarios",
        &with_answer(TAX_REVIEW_TRANSCRIPT, &answer),
    )));
    let before = core.proposals().unwrap().len();
    let advice = core.run_tax_advisor(Some(&review()), true).unwrap();
    assert_eq!(advice.status, "rejected");
    assert!(
        advice.problems[0].starts_with("206 760 Kč"),
        "{:?}",
        advice.problems
    );
    assert!(
        advice.explanation.is_none(),
        "a rejected answer isn't shown"
    );
    assert_eq!(core.proposals().unwrap().len(), before, "nor filed");
    let last = &core.egress_register().unwrap()[0];
    assert!(last.outcome.starts_with("Rejected"), "{}", last.outcome);
}

#[test]
fn an_unknown_recommendation_or_a_blocked_task_never_shows() {
    let core = Core::demo().unwrap();
    let mut answer = answer_of(TAX_BOOKS_TRANSCRIPT);
    answer["recommendation"] = json!("pausalni_dan");
    core.replace_provider(Box::new(
        Fake::new().with("tax.scenarios", &with_answer(TAX_BOOKS_TRANSCRIPT, &answer)),
    ));
    let advice = core.run_tax_advisor(None, true).unwrap();
    assert_eq!(advice.status, "rejected");
    assert!(advice.problems[0].contains("isn't one of the engine's scenarios"));
    core.set_egress_policy("tax.scenarios", "never").unwrap();
    let runs = core.egress_register().unwrap().len();
    assert_eq!(core.run_tax_advisor(None, true).unwrap().status, "blocked");
    assert_eq!(core.egress_register().unwrap().len(), runs);
}

// ── The eval set ───────────────────────────────────────────────────────────

/// What a careful model would write for a case: every figure quoted from
/// the scenarios exactly as the tool returned them.
fn scripted_answer(t: &TaxScenariosDto) -> Value {
    let crowns = |m: i64| skyla_money::Money::new(m, skyla_money::Currency::CZK).format_cs();
    let lowest = t.scenarios.iter().find(|s| s.id == t.lowest_total).unwrap();
    let base = t.scenarios.iter().find(|s| s.id == t.baseline).unwrap();
    let mut text = format!(
        "With {} income and {} actual expenses, the baseline ({}) comes to {} of tax and insurance.",
        crowns(t.income.minor),
        crowns(t.actual_expenses.minor),
        base.label,
        crowns(base.total.minor)
    );
    if lowest.id != base.id {
        text.push_str(&format!(
            " {} is the lowest at {}, a difference of {}, and the pension assessment base changes by {}.",
            lowest.label,
            crowns(lowest.total.minor),
            crowns(lowest.vs_baseline_total.minor),
            crowns(lowest.vs_baseline_pension_base.minor)
        ));
        if let Some(p) = &lowest.flat_rate_percent {
            text.push_str(&format!(
                " The flat rate of {p} % counts {} of expenses.",
                crowns(lowest.expenses.minor)
            ));
        }
    }
    text.push_str(" A scenario for your review, not tax advice.");
    json!({
        "recommendation": t.lowest_total,
        "explanation": text,
        "levers": ["expenses"],
        "questions": t.questions,
    })
}

#[test]
fn the_eval_set_passes_on_the_fake_provider() {
    assert!(CASES.len() >= 30);
    let core = Core::demo().unwrap();
    for c in CASES {
        let projection = c.projection();
        let engine = core.income_tax_scenarios(Some(&projection)).unwrap();
        // The engine's own verdict, independent of any answer.
        assert_eq!(engine.lowest_total, c.lowest, "{}: lowest", c.name);
        assert_eq!(
            !engine.questions.is_empty(),
            c.asks,
            "{}: questions",
            c.name
        );
        assert_eq!(
            engine.not_evaluated.len() > 1,
            c.unevaluated_purchase,
            "{}: not evaluated",
            c.name
        );

        let answer = scripted_answer(&engine);
        core.replace_provider(Box::new(Fake::new().with(
            "tax.scenarios",
            &with_answer(TAX_REVIEW_TRANSCRIPT, &answer),
        )));
        let advice = core.run_tax_advisor(Some(&projection), true).unwrap();
        assert_eq!(
            advice.status, "accepted",
            "{}: {:?}",
            c.name, advice.problems
        );
        assert_eq!(advice.questions.is_empty(), !c.asks, "{}", c.name);

        // Change any one figure by one crown: rejected.
        let text = answer["explanation"].as_str().unwrap();
        let first = engine
            .scenarios
            .iter()
            .find(|s| s.id == engine.baseline)
            .unwrap()
            .total
            .minor;
        let shown = skyla_money::Money::new(first, skyla_money::Currency::CZK).format_cs();
        let wrong = skyla_money::Money::new(first + 100, skyla_money::Currency::CZK).format_cs();
        let mut bad = answer.clone();
        bad["explanation"] = json!(text.replacen(&shown, &wrong, 1));
        core.replace_provider(Box::new(
            Fake::new().with("tax.scenarios", &with_answer(TAX_REVIEW_TRANSCRIPT, &bad)),
        ));
        let advice = core.run_tax_advisor(Some(&projection), true).unwrap();
        assert_eq!(
            advice.status, "rejected",
            "{}: a wrong figure got through",
            c.name
        );
    }
}

#[test]
fn the_live_harness_judges_and_costs_each_case() {
    use skyla_app::evals::run_tax;
    // The demo's recorded answer is for the review's projection (with a laptop),
    // so on the first case (no laptop) it names a scenario that doesn't exist.
    let core = Core::demo().unwrap();
    let report = run_tax(&core, "fake", Some(1)).unwrap();
    assert_eq!(report.cases.len(), 1);
    let c = &report.cases[0];
    assert!(!c.passed && c.status == "rejected", "{c:?}");
    assert!(
        c.reasons[0].contains("isn't one of the engine's scenarios"),
        "{:?}",
        c.reasons
    );
    // The provider's estimate from the transcript is summed.
    assert!(report.cost_estimate_usd > 0.0);
}
