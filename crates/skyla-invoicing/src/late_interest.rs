//! Statutory late-payment interest (úrok z prodlení) and the recovery cost.
//!
//! The annual rate is the ČNB repo rate valid on the first day of the
//! calendar half-year in which the delay began, plus the pack's margin, and
//! stays fixed for the whole delay (nařízení vlády č. 351/2013 Sb., § 2).
//! Interest accrues daily on what is still owed: from the day after the due
//! date through the day of payment, actual days over 365. Each period
//! between payments is rounded to the haléř, half up, so the printed lines
//! add up to the total. The repo history is reference data the caller
//! supplies (imported, or fetched when the user opts in).

use serde::{Deserialize, Serialize};
use skyla_money::{Money, Rate, RoundingMode};
use skyla_rules::{Pack, date};

use crate::InvoicingError;

/// A ČNB repo rate (annual percent) in force from a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRate {
    /// `YYYY-MM-DD`.
    pub effective_from: String,
    /// Annual percent, e.g. `3.50`.
    pub rate: Rate,
}

/// One stretch of the delay at a constant principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterestPeriod {
    /// First day of interest.
    pub from: String,
    /// Last day of interest (inclusive).
    pub to: String,
    /// Days, `from` and `to` included.
    pub days: i64,
    /// What was owed during the period.
    pub principal: Money,
    /// Interest for the period, rounded to the haléř.
    pub interest: Money,
}

/// Interest on one overdue receivable up to a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LateInterest {
    /// The first day of delay (the day after the due date).
    pub delay_from: String,
    /// The half-year start whose repo rate applies.
    pub rate_date: String,
    /// The repo rate on `rate_date`.
    pub repo_rate: Rate,
    /// The pack's margin in percentage points.
    pub margin: Rate,
    /// Annual rate: repo + margin.
    pub annual_rate: Rate,
    /// The periods, in order.
    pub periods: Vec<InterestPeriod>,
    /// Sum of the periods.
    pub total: Money,
    /// The statutory minimum recovery cost a business debtor owes per receivable.
    pub recovery_cost: Money,
}

/// The repo rate in force on `on`: the latest one effective on or before it.
pub fn repo_rate_on<'a>(rates: &'a [RepoRate], on: &str) -> Option<&'a RepoRate> {
    rates
        .iter()
        .filter(|r| r.effective_from.as_str() <= on)
        .max_by(|a, b| a.effective_from.cmp(&b.effective_from))
}

/// Interest on `principal` due on `due_date`, counted through `until`.
/// `reductions` are dated payments and credits against it, in any order.
/// `None` when nothing was overdue by `until`.
pub fn late_interest(
    pack: &Pack,
    repo: &[RepoRate],
    principal: Money,
    due_date: &str,
    until: &str,
    reductions: &[(String, Money)],
) -> Result<Option<LateInterest>, InvoicingError> {
    let bad = |d: &str| InvoicingError::Invalid(vec![format!("{d:?} isn't a YYYY-MM-DD date")]);
    let due = date::parse(due_date).ok_or_else(|| bad(due_date))?;
    let end = date::parse(until).ok_or_else(|| bad(until))?;
    let currency = principal.currency();
    let mut sorted: Vec<(i64, Money)> = Vec::new();
    for (d, amount) in reductions {
        sorted.push((date::parse(d).ok_or_else(|| bad(d))?, *amount));
    }
    sorted.sort_by_key(|(d, _)| *d);

    // Paid on time doesn't accrue.
    let mut owed = principal;
    for (_, amount) in sorted.iter().filter(|(d, _)| *d <= due) {
        owed = owed.checked_sub(*amount)?;
    }
    let start = due + 1;
    if end < start || owed.minor() <= 0 {
        return Ok(None);
    }

    let delay_from = date::format(start);
    let (year, month, _) = date::from_days(start);
    let rate_date = format!("{year:04}-{}-01", if month <= 6 { "01" } else { "07" });
    let repo_rate = repo_rate_on(repo, &rate_date)
        .ok_or_else(|| {
            InvoicingError::Invalid(vec![format!(
                "no ČNB repo rate for {rate_date}; import the ČNB rate history"
            )])
        })?
        .rate;
    let margin = pack.percent("late_interest.margin_pp", &rate_date)?;
    let annual_rate = repo_rate + margin;
    let recovery_cost = pack.amount("late_interest.recovery_cost", &delay_from)?;

    let mut periods = Vec::new();
    let mut from = start;
    let mut accrue = |from: i64, to: i64, owed: Money| -> Result<(), InvoicingError> {
        if to < from || owed.minor() <= 0 {
            return Ok(());
        }
        let days = to - from + 1;
        let factor = annual_rate * Rate::from(days) / Rate::from(36_500);
        periods.push(InterestPeriod {
            from: date::format(from),
            to: date::format(to),
            days,
            principal: owed,
            interest: owed.mul_rate(factor, RoundingMode::HalfUp)?,
        });
        Ok(())
    };
    for (d, amount) in sorted.iter().filter(|(d, _)| *d > due && *d <= end) {
        accrue(from, *d, owed)?;
        owed = owed.checked_sub(*amount)?;
        from = (*d + 1).max(from);
    }
    accrue(from, end, owed)?;
    let total = Money::sum(currency, periods.iter().map(|p| p.interest))?;
    Ok(Some(LateInterest {
        delay_from,
        rate_date,
        repo_rate,
        margin,
        annual_rate,
        periods,
        total,
        recovery_cost,
    }))
}
