//! DPFO § 7 worksheet and the scenario engine (WP-22).
//!
//! The worksheet takes a year's § 7 income and expenses, by actual
//! expenses or a flat rate, to the income tax and the two insurances that
//! follow from the same profit. The scenario engine enumerates the levers
//! the facts allow and computes each combination with the worksheet, so
//! every figure an advisor may quote comes from here. Rates, caps,
//! roundings and thresholds come from the rule pack; what the pack doesn't
//! hold is stated as an assumption, never guessed.

use serde::{Deserialize, Serialize};
use skyla_money::{Money, MoneyError, Rate, RoundingMode};
use skyla_rules::{Pack, RulesError};

/// Errors from the worksheet.
#[derive(Debug, thiserror::Error)]
pub enum IncomeError {
    /// A pack value is missing or of the wrong kind.
    #[error(transparent)]
    Rules(#[from] RulesError),
    /// Arithmetic left the representable range.
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// The flat-rate group of an activity (§ 7 odst. 7 ZDP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlatRate {
    /// Craft trades and agriculture (80 %).
    Craft,
    /// Other trades (60 %).
    Trade,
    /// Other self-employment, e.g. liberal professions (40 %).
    Liberal,
}

impl FlatRate {
    fn key(self) -> &'static str {
        match self {
            Self::Craft => "craft",
            Self::Trade => "trade",
            Self::Liberal => "liberal",
        }
    }
}

/// How expenses are claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "method", content = "group")]
pub enum Expenses {
    /// What was actually spent (daňová evidence).
    Actual,
    /// A percentage of income, capped.
    FlatRate(FlatRate),
}

/// A year's § 7 figures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section7 {
    /// The date pack values are taken on (normally 31 December).
    pub on: String,
    /// § 7 income.
    pub income: Money,
    /// Tax-deductible expenses actually incurred.
    pub actual_expenses: Money,
}

/// One insurance computed from the profit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insurance {
    /// The share of the profit it's assessed on.
    pub share_percent: Rate,
    /// The annual assessment base.
    pub assessment_base: Money,
    /// The rate.
    pub rate_percent: Rate,
    /// The annual insurance.
    pub amount: Money,
}

/// The worksheet, every step kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worksheet {
    /// How expenses were claimed.
    pub expenses_method: Expenses,
    /// § 7 income.
    pub income: Money,
    /// Expenses claimed.
    pub expenses: Money,
    /// The flat-rate percentage, when used.
    pub flat_rate_percent: Option<Rate>,
    /// The flat-rate cap, when used.
    pub flat_rate_cap: Option<Money>,
    /// The cap limited the flat-rate expenses.
    pub capped: bool,
    /// Income minus expenses (negative for a loss).
    pub profit: Money,
    /// The tax base, rounded down by the pack's step.
    pub tax_base: Money,
    /// The rate applied.
    pub tax_rate_percent: Rate,
    /// Tax before credits.
    pub tax_before_credits: Money,
    /// The taxpayer credit used (no more than the tax).
    pub taxpayer_credit: Money,
    /// Tax after credits, rounded up by the pack's step.
    pub tax: Money,
    /// Social insurance (pension and state employment policy).
    pub social: Insurance,
    /// Health insurance.
    pub health: Insurance,
    /// Tax and both insurances.
    pub total: Money,
    /// What the computation assumed because the pack or the facts didn't say.
    pub assumptions: Vec<String>,
}

fn of_percent(amount: Money, percent: Rate, mode: RoundingMode) -> Result<Money, MoneyError> {
    let rate = percent
        .checked_div(Rate::ONE_HUNDRED)
        .ok_or(MoneyError::Overflow)?;
    amount.mul_rate(rate, mode)
}

/// Rounds to a multiple of `step` (an amount from the pack), down or up.
fn to_step(amount: Money, step: Money, up: bool) -> Result<Money, MoneyError> {
    let s = step.minor();
    if s <= 0 {
        return Ok(amount);
    }
    let m = amount.minor();
    let mut steps = m.div_euclid(s);
    if up && m.rem_euclid(s) != 0 {
        steps = steps.checked_add(1).ok_or(MoneyError::Overflow)?;
    }
    Ok(Money::new(
        steps.checked_mul(s).ok_or(MoneyError::Overflow)?,
        amount.currency(),
    ))
}

fn insurance(pack: &Pack, on: &str, profit: Money, kind: &str) -> Result<Insurance, IncomeError> {
    let share = pack.percent(&format!("insurance.{kind}.share"), on)?;
    let rate = pack.percent(&format!("insurance.{kind}.rate"), on)?;
    let step = pack.amount(&format!("insurance.{kind}.round_up_to"), on)?;
    let positive = if profit.is_negative() {
        Money::zero(profit.currency())
    } else {
        profit
    };
    let base = to_step(
        of_percent(positive, share, RoundingMode::AwayFromZero)?,
        step,
        true,
    )?;
    let amount = to_step(
        of_percent(base, rate, RoundingMode::AwayFromZero)?,
        step,
        true,
    )?;
    Ok(Insurance {
        share_percent: share,
        assessment_base: base,
        rate_percent: rate,
        amount,
    })
}

/// The § 7 worksheet for one way of claiming expenses.
pub fn worksheet(
    pack: &Pack,
    facts: &Section7,
    method: Expenses,
) -> Result<Worksheet, IncomeError> {
    let on = facts.on.as_str();
    let zero = Money::zero(facts.income.currency());
    let mut assumptions = Vec::new();
    let (expenses, percent, cap, capped) = match method {
        Expenses::Actual => (facts.actual_expenses, None, None, false),
        Expenses::FlatRate(group) => {
            let percent =
                pack.percent(&format!("income_tax.flat_rate.{}.percent", group.key()), on)?;
            let cap = pack.amount(&format!("income_tax.flat_rate.{}.cap", group.key()), on)?;
            let positive = if facts.income.is_negative() {
                zero
            } else {
                facts.income
            };
            let raw = of_percent(positive, percent, RoundingMode::TowardZero)?;
            let capped = raw.minor() > cap.minor();
            (
                if capped { cap } else { raw },
                Some(percent),
                Some(cap),
                capped,
            )
        }
    };
    let profit = facts.income.checked_sub(expenses)?;
    let step = pack.amount("income_tax.base.round_down_to", on)?;
    let tax_base = if profit.is_negative() {
        zero
    } else {
        to_step(profit, step, false)?
    };
    let rate = pack.percent("income_tax.rate.base", on)?;
    // Above 36 × the average wage the rate is higher; the pack doesn't hold
    // that threshold for this year yet, so it can't be applied.
    if pack.amount("income_tax.solidarity_threshold", on).is_err() {
        assumptions.push(
            "The higher rate above the solidarity threshold isn't applied: the pack doesn't hold the threshold for this year.".to_owned(),
        );
    }
    let before = of_percent(tax_base, rate, RoundingMode::TowardZero)?;
    let credit_full = pack.amount("income_tax.credit.taxpayer", on)?;
    let credit = if credit_full.minor() > before.minor() {
        before
    } else {
        credit_full
    };
    let tax = to_step(
        before.checked_sub(credit)?,
        pack.amount("tax.round_up_to", on)?,
        true,
    )?;
    let social = insurance(pack, on, profit, "social")?;
    let health = insurance(pack, on, profit, "health")?;
    assumptions.push(
        "The minimum assessment bases aren't applied: the pack doesn't hold the 2026 average wage. If either base is below its minimum, that insurance is higher.".to_owned(),
    );
    assumptions.push(
        "Only § 7 income; no other income, deductions or credits besides the taxpayer's own."
            .to_owned(),
    );
    let total = tax.checked_add(social.amount)?.checked_add(health.amount)?;
    Ok(Worksheet {
        expenses_method: method,
        income: facts.income,
        expenses,
        flat_rate_percent: percent,
        flat_rate_cap: cap,
        capped,
        profit,
        tax_base,
        tax_rate_percent: rate,
        tax_before_credits: before,
        taxpayer_credit: credit,
        tax,
        social,
        health,
        total,
        assumptions,
    })
}

/// A purchase the user is planning, for the timing lever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedPurchase {
    /// What it is.
    pub description: String,
    /// Its price excluding claimable VAT.
    pub price: Money,
}

/// What the scenario engine needs to know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioFacts {
    /// The year's (projected) § 7 figures.
    pub section7: Section7,
    /// The activity's flat-rate group, when known.
    pub flat_rate: Option<FlatRate>,
    /// A purchase that could happen this year or next.
    pub planned_purchase: Option<PlannedPurchase>,
}

/// One combination of levers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable id, e.g. `flat_rate.trade+purchase.next_year`.
    pub id: String,
    /// The levers set, as (lever, choice).
    pub levers: Vec<(String, String)>,
    /// The computation.
    pub worksheet: Worksheet,
}

/// What a scenario changes against the baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    /// The scenario.
    pub id: String,
    /// Tax, scenario minus baseline.
    pub tax: Money,
    /// Social insurance, scenario minus baseline.
    pub social: Money,
    /// Health insurance, scenario minus baseline.
    pub health: Money,
    /// Everything, scenario minus baseline.
    pub total: Money,
    /// The pension assessment base, scenario minus baseline: lower now means
    /// a lower pension later.
    pub pension_base: Money,
}

/// Every scenario the facts allow, against the baseline (actual expenses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioAnalysis {
    /// Actual expenses, the purchase (if any) this year.
    pub baseline: String,
    /// Every combination computed.
    pub scenarios: Vec<Scenario>,
    /// Each scenario against the baseline.
    pub differences: Vec<Difference>,
    /// The lowest total; ties keep the baseline.
    pub lowest_total: String,
    /// Facts the books can't hold that would change the analysis.
    pub questions: Vec<String>,
    /// Levers the engine couldn't evaluate, and why.
    pub not_evaluated: Vec<String>,
}

/// Computes every lever combination the facts allow.
pub fn scenarios(pack: &Pack, facts: &ScenarioFacts) -> Result<ScenarioAnalysis, IncomeError> {
    let on = facts.section7.on.as_str();
    let mut questions = Vec::new();
    let mut not_evaluated = vec![
        "Paušální daň: the pack doesn't hold the 2026 bands, so eligibility and its amount aren't assessed.".to_owned(),
    ];
    let mut methods = vec![Expenses::Actual];
    match facts.flat_rate {
        Some(g) => methods.push(Expenses::FlatRate(g)),
        None => {
            let pct = |g: FlatRate| -> Result<String, IncomeError> {
                let p = pack.percent(&format!("income_tax.flat_rate.{}.percent", g.key()), on)?;
                Ok(p.normalize().to_string().replace('.', ","))
            };
            questions.push(format!(
                "Which flat-rate group does your activity fall in (craft {} %, other trade {} %, other {} %)?",
                pct(FlatRate::Craft)?,
                pct(FlatRate::Trade)?,
                pct(FlatRate::Liberal)?
            ));
        }
    }
    // Timing: an expensed purchase lowers this year's actual expenses only if
    // bought this year; one above the asset threshold is depreciated instead.
    let mut timings: Vec<(&str, Money)> = vec![("", Money::zero(facts.section7.income.currency()))];
    if let Some(p) = &facts.planned_purchase {
        let threshold = pack.amount("assets.tangible.threshold", on)?;
        if p.price.minor() > threshold.minor() {
            not_evaluated.push(format!(
                "{}: above the {} asset threshold it's depreciated, and the pack doesn't hold the depreciation groups yet.",
                p.description,
                threshold.format_cs()
            ));
        } else {
            timings = vec![
                ("this_year", p.price),
                ("next_year", Money::zero(p.price.currency())),
            ];
        }
    }
    let mut out = Vec::new();
    for method in &methods {
        for (timing, extra) in &timings {
            let mut s7 = facts.section7.clone();
            s7.actual_expenses = s7.actual_expenses.checked_add(*extra)?;
            let worksheet = worksheet(pack, &s7, *method)?;
            let (lever, choice) = match method {
                Expenses::Actual => ("expenses", "actual".to_owned()),
                Expenses::FlatRate(g) => ("expenses", format!("flat_rate.{}", g.key())),
            };
            let mut levers = vec![(lever.to_owned(), choice.clone())];
            let mut id = choice;
            if !timing.is_empty() {
                levers.push(("purchase".to_owned(), (*timing).to_owned()));
                id = format!("{id}+purchase.{timing}");
            }
            out.push(Scenario {
                id,
                levers,
                worksheet,
            });
        }
    }
    let base = out.first().map(|s| s.worksheet.clone());
    let baseline = out.first().map(|s| s.id.clone()).unwrap_or_default();
    let mut differences = Vec::new();
    if let Some(b) = &base {
        for s in &out {
            let w = &s.worksheet;
            differences.push(Difference {
                id: s.id.clone(),
                tax: w.tax.checked_sub(b.tax)?,
                social: w.social.amount.checked_sub(b.social.amount)?,
                health: w.health.amount.checked_sub(b.health.amount)?,
                total: w.total.checked_sub(b.total)?,
                pension_base: w
                    .social
                    .assessment_base
                    .checked_sub(b.social.assessment_base)?,
            });
        }
    }
    let lowest_total = out
        .iter()
        .min_by_key(|s| s.worksheet.total.minor())
        .map(|s| s.id.clone())
        .unwrap_or_default();
    Ok(ScenarioAnalysis {
        baseline,
        scenarios: out,
        differences,
        lowest_total,
        questions,
        not_evaluated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_the_right_way() {
        let czk = |m| Money::new(m, skyla_money::Currency::CZK);
        assert_eq!(
            to_step(czk(62_845_099), czk(10_000), false).ok(),
            Some(czk(62_840_000))
        );
        assert_eq!(
            to_step(czk(8_017_650), czk(100), true).ok(),
            Some(czk(8_017_700))
        );
        assert_eq!(
            to_step(czk(8_017_600), czk(100), true).ok(),
            Some(czk(8_017_600))
        );
    }
}
