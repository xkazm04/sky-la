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
    assert!(review.explanation.unwrap().contains("197 584 Kč"));
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
        .replace("197 584 Kč", "197 585 Kč");
    answer["explanation"] = json!(text);
    core.replace_provider(Box::new(Fake::new().with(
        "tax.scenarios",
        &with_answer(TAX_REVIEW_TRANSCRIPT, &answer),
    )));
    let before = core.proposals().unwrap().len();
    let advice = core.run_tax_advisor(Some(&review()), true).unwrap();
    assert_eq!(advice.status, "rejected");
    assert!(
        advice.problems[0].starts_with("197 585 Kč"),
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

struct Case {
    name: &'static str,
    income: &'static str,
    expenses: &'static str,
    group: Option<&'static str>,
    purchase: Option<(&'static str, &'static str)>,
    /// What the engine must put lowest (checked independently of the answer).
    lowest: &'static str,
    /// The engine must ask for facts.
    asks: bool,
    /// The engine must say a lever couldn't be evaluated beyond paušální daň.
    unevaluated_purchase: bool,
}

const fn case(
    name: &'static str,
    income: &'static str,
    expenses: &'static str,
    group: Option<&'static str>,
    purchase: Option<(&'static str, &'static str)>,
    lowest: &'static str,
) -> Case {
    Case {
        name,
        income,
        expenses,
        group,
        purchase,
        lowest,
        asks: group.is_none(),
        unevaluated_purchase: false,
    }
}

/// Thirty Czech situations: trades, crafts and liberal professions, low and
/// high costs, the caps, a loss, purchases below and above the asset
/// threshold, and an unknown flat-rate group.
const CASES: &[Case] = &[
    case(
        "web designer, low costs",
        "1 571 000",
        "383 200",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "web designer, high costs",
        "1 571 000",
        "1 100 000",
        Some("trade"),
        None,
        "actual",
    ),
    case(
        "translator, small income",
        "320 000",
        "40 000",
        Some("liberal"),
        None,
        "flat_rate.liberal",
    ),
    case(
        "translator, costs at 40 %",
        "600 000",
        "240 000",
        Some("liberal"),
        None,
        "actual",
    ),
    case(
        "joiner, low costs",
        "900 000",
        "200 000",
        Some("craft"),
        None,
        "flat_rate.craft",
    ),
    case(
        "joiner, high costs",
        "900 000",
        "850 000",
        Some("craft"),
        None,
        "actual",
    ),
    case(
        "plumber at the craft cap",
        "2 400 000",
        "500 000",
        Some("craft"),
        None,
        "flat_rate.craft",
    ),
    case(
        "consultant above the trade cap",
        "2 500 000",
        "300 000",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "consultant, actual beats capped flat",
        "3 000 000",
        "1 500 000",
        Some("trade"),
        None,
        "actual",
    ),
    case(
        "architect above the liberal cap",
        "2 400 000",
        "200 000",
        Some("liberal"),
        None,
        "flat_rate.liberal",
    ),
    case(
        "loss year",
        "200 000",
        "320 000",
        Some("trade"),
        None,
        "actual",
    ),
    case(
        "break-even",
        "400 000",
        "400 000",
        Some("trade"),
        None,
        "actual",
    ),
    case(
        "tiny side income",
        "60 000",
        "5 000",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "credit absorbs the tax",
        "250 000",
        "20 000",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "photographer",
        "780 000",
        "310 000",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "programmer",
        "1 980 000",
        "150 000",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "baker",
        "1 200 000",
        "1 050 000",
        Some("craft"),
        None,
        "actual",
    ),
    case(
        "hairdresser",
        "640 000",
        "180 000",
        Some("craft"),
        None,
        "flat_rate.craft",
    ),
    case(
        "tax adviser",
        "1 100 000",
        "120 000",
        Some("liberal"),
        None,
        "flat_rate.liberal",
    ),
    case(
        "doctor with a practice",
        "2 000 000",
        "900 000",
        Some("liberal"),
        None,
        "actual",
    ),
    case(
        "unknown group, low costs",
        "1 000 000",
        "100 000",
        None,
        None,
        "actual",
    ),
    case(
        "unknown group, high costs",
        "1 000 000",
        "700 000",
        None,
        None,
        "actual",
    ),
    case(
        "laptop under the threshold, flat",
        "1 571 000",
        "383 200",
        Some("trade"),
        Some(("Laptop", "60 000")),
        "flat_rate.trade+purchase.this_year",
    ),
    case(
        "laptop under the threshold, actual",
        "1 571 000",
        "1 100 000",
        Some("trade"),
        Some(("Laptop", "60 000")),
        "actual+purchase.this_year",
    ),
    case(
        "printer at exactly the threshold",
        "900 000",
        "500 000",
        Some("trade"),
        Some(("Printer", "80 000")),
        "actual+purchase.this_year",
    ),
    Case {
        unevaluated_purchase: true,
        ..case(
            "car above the threshold",
            "1 571 000",
            "383 200",
            Some("trade"),
            Some(("Car", "450 000")),
            "flat_rate.trade",
        )
    },
    Case {
        unevaluated_purchase: true,
        ..case(
            "machine above the threshold, craft",
            "1 800 000",
            "600 000",
            Some("craft"),
            Some(("CNC machine", "250 000")),
            "flat_rate.craft",
        )
    },
    case(
        "tools for a joiner",
        "900 000",
        "700 000",
        Some("craft"),
        Some(("Tools", "40 000")),
        "actual+purchase.this_year",
    ),
    case(
        "decimals in the projection",
        "1 234 567,89",
        "345 678,90",
        Some("trade"),
        None,
        "flat_rate.trade",
    ),
    case(
        "no costs at all",
        "500 000",
        "0",
        Some("liberal"),
        None,
        "flat_rate.liberal",
    ),
];

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
        let projection = TaxProjectionDto {
            income: c.income.into(),
            expenses: c.expenses.into(),
            flat_rate: c.group.map(Into::into),
            purchase_description: c.purchase.map_or(String::new(), |p| p.0.into()),
            purchase_price: c.purchase.map_or(String::new(), |p| p.1.into()),
        };
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
