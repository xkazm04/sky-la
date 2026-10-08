//! The advisor eval set (WP-27, WP-29): Czech tax situations with the
//! engine's verdict for each, checked by hand. `tests/tax_advisor.rs` runs
//! them on the Fake provider in every CI run; `skyla-eval --live` runs them
//! on demand through the user's own Claude Code CLI and records the cost.

use serde::Serialize;

use crate::Core;
use crate::dto::{TaxAdviceDto, TaxProjectionDto, TaxScenariosDto};
use crate::error::CoreError;

/// One situation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    /// For people.
    pub name: &'static str,
    /// § 7 income, as typed.
    pub income: &'static str,
    /// Actual expenses, as typed.
    pub expenses: &'static str,
    /// Flat-rate group, when known.
    pub group: Option<&'static str>,
    /// A planned purchase and its price.
    pub purchase: Option<(&'static str, &'static str)>,
    /// What the engine must put lowest (checked independently of the answer).
    pub lowest: &'static str,
    /// The engine must ask for facts.
    pub asks: bool,
    /// The engine must say a lever couldn't be evaluated beyond paušální daň.
    pub unevaluated_purchase: bool,
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
pub const TAX_CASES: &[Case] = &[
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

impl Case {
    /// The projection the user would type.
    pub fn projection(&self) -> TaxProjectionDto {
        TaxProjectionDto {
            income: self.income.into(),
            expenses: self.expenses.into(),
            flat_rate: self.group.map(Into::into),
            purchase_description: self.purchase.map_or(String::new(), |p| p.0.into()),
            purchase_price: self.purchase.map_or(String::new(), |p| p.1.into()),
        }
    }
}

/// How one case went.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseResult {
    /// The case.
    pub name: String,
    /// It passed every check.
    pub passed: bool,
    /// The advisor's status.
    pub status: String,
    /// What it recommended.
    pub recommendation: Option<String>,
    /// What the engine puts lowest.
    pub lowest: String,
    /// Figures matched to the engine.
    pub grounded: u32,
    /// Why it failed.
    pub reasons: Vec<String>,
    /// The provider's cost estimate, as recorded in the register.
    pub cost_note: Option<String>,
}

/// A whole run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// The provider.
    pub provider: String,
    /// Every case.
    pub cases: Vec<CaseResult>,
    /// Cases passed.
    pub passed: usize,
    /// Estimated cost in USD, summed from the provider's estimates.
    pub cost_estimate_usd: f64,
}

/// Judges an answer: accepted (every figure grounded), the engine's lowest
/// scenario first, and questions asked exactly when the engine needs facts.
pub fn judge(case: &Case, engine: &TaxScenariosDto, advice: &TaxAdviceDto) -> Vec<String> {
    let mut reasons = Vec::new();
    if engine.lowest_total != case.lowest {
        reasons.push(format!(
            "the engine puts {} lowest, the case expects {}",
            engine.lowest_total, case.lowest
        ));
    }
    if advice.status != "accepted" {
        reasons.push(format!("{}: {}", advice.status, advice.problems.join("; ")));
    } else if advice.recommendation.as_deref() != Some(engine.lowest_total.as_str()) {
        reasons.push(format!(
            "recommended {:?}, the lowest is {}",
            advice.recommendation, engine.lowest_total
        ));
    }
    if case.asks && advice.status == "accepted" && advice.questions.is_empty() {
        reasons.push("asked nothing although the flat-rate group is unknown".into());
    }
    reasons
}

/// Runs every case on the core's current provider (the caller swaps it in)
/// with the tax task allowed to run, and collects the results.
pub fn run_tax(core: &Core, provider: &str, limit: Option<usize>) -> Result<Report, CoreError> {
    core.set_egress_policy("tax.scenarios", "always")?;
    let mut cases = Vec::new();
    let mut cost = 0.0_f64;
    for case in TAX_CASES.iter().take(limit.unwrap_or(usize::MAX)) {
        let projection = case.projection();
        let engine = core.income_tax_scenarios(Some(&projection))?;
        let advice = core.run_tax_advisor(Some(&projection), true)?;
        let reasons = judge(case, &engine, &advice);
        let cost_note = core
            .egress_register()?
            .into_iter()
            .find(|r| Some(&r.id) == advice.run_id.as_ref())
            .and_then(|r| r.cost_note);
        if let Some(c) = cost_note
            .as_deref()
            .and_then(|n| n.strip_prefix("about $"))
            .and_then(|n| n.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
        {
            cost += c;
        }
        cases.push(CaseResult {
            name: case.name.into(),
            passed: reasons.is_empty(),
            status: advice.status.clone(),
            recommendation: advice.recommendation.clone(),
            lowest: engine.lowest_total.clone(),
            grounded: advice.grounded,
            reasons,
            cost_note,
        });
    }
    let passed = cases.iter().filter(|c| c.passed).count();
    Ok(Report {
        provider: provider.into(),
        cases,
        passed,
        cost_estimate_usd: cost,
    })
}
