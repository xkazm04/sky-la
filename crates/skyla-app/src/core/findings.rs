//! The financial advisor's detectors (WP-28): deterministic checks over
//! the books that produce candidate findings, each citing the entries it
//! rests on. The model may rank and explain them; it never computes them.
//!
//! Sensitivities are product heuristics, not statutory values, so they live
//! in [`DetectorSettings`] rather than the rule pack.

use std::collections::BTreeMap;

use serde::Serialize;

use super::Core;
use crate::dto::{FindingDto, MoneyDto};
use crate::error::CoreError;

/// How sensitive the detectors are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectorSettings {
    /// A change counts from this many percent…
    pub change_percent: i64,
    /// …and this many minor units.
    pub change_minor: i64,
    /// A vendor's hourly rate change counts from this many percent.
    pub rate_percent: i64,
    /// A payment this many days after its due date is late.
    pub late_days: i64,
    /// Fewer months of expenses covered by cash than this is short.
    pub runway_months: i64,
}

impl Default for DetectorSettings {
    fn default() -> Self {
        Self {
            change_percent: 25,
            change_minor: 500_000,
            rate_percent: 10,
            late_days: 7,
            runway_months: 6,
        }
    }
}

/// One expense or revenue posting, attributed where the books allow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Posting {
    /// The journal entry.
    pub entry: i64,
    /// Its date.
    pub date: String,
    /// The account.
    pub account: String,
    /// Signed functional amount (debit positive).
    pub minor: i64,
    /// The supplier, when a received invoice says who.
    pub vendor: Option<String>,
    /// Hours the supplier billed, when known (hundredths).
    pub hours_hundredths: Option<i64>,
    /// The customer the work was for, when known.
    pub client: Option<String>,
    /// The entry's memo.
    pub memo: String,
}

/// An issued invoice, for the revenue and payment detectors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sale {
    /// Its number.
    pub number: String,
    /// The customer.
    pub client: String,
    /// Issued.
    pub issued: String,
    /// Due.
    pub due: Option<String>,
    /// Paid in full, if it was.
    pub paid: Option<String>,
    /// Base (excluding VAT).
    pub base_minor: i64,
    /// Its ledger entry.
    pub entry: Option<i64>,
    /// Days overdue as of today, if open.
    pub overdue_days: Option<i64>,
}

/// What the detectors look at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Books {
    /// Expense postings (accounts 5xx).
    pub expenses: Vec<Posting>,
    /// Issued invoices.
    pub sales: Vec<Sale>,
    /// Cash and bank balance at the end.
    pub cash_minor: i64,
}

/// A detector's finding before it becomes a DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable id.
    pub id: String,
    /// Which detector.
    pub detector: &'static str,
    /// One line.
    pub title: String,
    /// Second line.
    pub detail: String,
    /// Figures, as (label, minor units or None for a plain number in `text`).
    pub figures: Vec<(String, i64)>,
    /// Percentages quoted (hundredths of a percent).
    pub percents: Vec<(String, i64)>,
    /// The entries it rests on.
    pub cites: Vec<i64>,
}

fn sum(p: &[&Posting]) -> i64 {
    p.iter().map(|x| x.minor).sum()
}

/// Change in hundredths of a percent, rounded half away from zero.
fn change_hundredths(before: i64, after: i64) -> Option<i64> {
    if before == 0 {
        return None;
    }
    let num = (after - before) * 10_000;
    let q = num / before;
    let r = num % before;
    Some(if 2 * r.abs() >= before.abs() {
        q + q.signum()
    } else {
        q
    })
}

/// Whole percent, half away from zero: `4203` → `42 %`.
fn whole_percent(h: i64) -> i64 {
    (h + 50 * h.signum()) / 100
}

fn pct_text(h: i64) -> String {
    format!("{} %", whole_percent(h))
}

fn crowns(minor: i64) -> String {
    skyla_money::Money::new(minor, skyla_money::Currency::CZK).format_cs()
}

fn in_range(date: &str, (from, to): (&str, &str)) -> bool {
    date >= from && date <= to
}

/// Subcontracting: suppliers who bill hours, as a group, period on period.
pub fn subcontracting(
    b: &Books,
    prev: (&str, &str),
    cur: (&str, &str),
    s: DetectorSettings,
) -> Option<Finding> {
    let group = |r| -> Vec<&Posting> {
        b.expenses
            .iter()
            .filter(|p| p.hours_hundredths.is_some() && in_range(&p.date, r))
            .collect()
    };
    let (before, after) = (group(prev), group(cur));
    let (a, z) = (sum(&before), sum(&after));
    let change = change_hundredths(a, z)?;
    if change.abs() < s.change_percent * 100 || (z - a).abs() < s.change_minor {
        return None;
    }
    let vendors: std::collections::BTreeSet<&str> = before
        .iter()
        .chain(&after)
        .filter_map(|p| p.vendor.as_deref())
        .collect();
    let mut cites: Vec<i64> = before.iter().chain(&after).map(|p| p.entry).collect();
    cites.sort_unstable();
    Some(Finding {
        id: "subcontracting".into(),
        detector: "variance",
        title: format!(
            "Subcontracting {} {} on the previous quarter",
            if change > 0 { "rose" } else { "fell" },
            pct_text(change.abs())
        ),
        detail: format!(
            "{} → {} across {} suppliers who bill hours",
            crowns(a),
            crowns(z),
            vendors.len()
        ),
        figures: vec![
            ("Previous quarter".into(), a),
            ("This quarter".into(), z),
            ("Change".into(), z - a),
        ],
        percents: vec![("Change".into(), whole_percent(change) * 100)],
        cites,
    })
}

/// A supplier's billing in one period: amount, hours (hundredths), entries.
type Billed = (i64, i64, Vec<i64>);

/// Hourly rates per supplier, period on period.
pub fn vendor_rates(
    b: &Books,
    prev: (&str, &str),
    cur: (&str, &str),
    s: DetectorSettings,
) -> Vec<Finding> {
    let mut by: BTreeMap<&str, [Billed; 2]> = BTreeMap::new();
    for p in &b.expenses {
        let (Some(v), Some(h)) = (p.vendor.as_deref(), p.hours_hundredths) else {
            continue;
        };
        let slot = if in_range(&p.date, prev) {
            0
        } else if in_range(&p.date, cur) {
            1
        } else {
            continue;
        };
        let e = by.entry(v).or_default();
        e[slot].0 += p.minor;
        e[slot].1 += h;
        e[slot].2.push(p.entry);
    }
    let mut out = Vec::new();
    for (vendor, [(a_minor, a_h, a_e), (z_minor, z_h, z_e)]) in by {
        if a_h == 0 || z_h == 0 {
            continue;
        }
        // Rate per hour in minor units, rounded half up.
        let rate = |m: i64, h: i64| (m * 100 * 2 + h) / (h * 2);
        let (ra, rz) = (rate(a_minor, a_h), rate(z_minor, z_h));
        let Some(change) = change_hundredths(ra, rz) else {
            continue;
        };
        if change < s.rate_percent * 100 {
            continue;
        }
        let mut cites = a_e;
        cites.extend(z_e);
        out.push(Finding {
            id: format!("rate:{vendor}"),
            detector: "vendor_rate",
            title: format!("{vendor}: hourly rate up {}", pct_text(change)),
            detail: format!("{} per hour → {} per hour", crowns(ra), crowns(rz)),
            figures: vec![("Rate before".into(), ra), ("Rate now".into(), rz)],
            percents: vec![("Change".into(), whole_percent(change) * 100)],
            cites,
        });
    }
    out
}

/// Revenue less the subcontracting done for each client, this period.
pub fn client_margins(b: &Books, cur: (&str, &str)) -> Vec<Finding> {
    let mut revenue: BTreeMap<&str, (i64, Vec<i64>)> = BTreeMap::new();
    for s in b.sales.iter().filter(|s| in_range(&s.issued, cur)) {
        let e = revenue.entry(s.client.as_str()).or_default();
        e.0 += s.base_minor;
        e.1.extend(s.entry);
    }
    let mut costs: BTreeMap<&str, (i64, Vec<i64>)> = BTreeMap::new();
    for p in b.expenses.iter().filter(|p| in_range(&p.date, cur)) {
        if let Some(c) = p.client.as_deref() {
            let e = costs.entry(c).or_default();
            e.0 += p.minor;
            e.1.push(p.entry);
        }
    }
    let mut out = Vec::new();
    for (client, (rev, mut cites)) in revenue {
        let (cost, cost_entries) = costs.get(client).cloned().unwrap_or_default();
        if rev == 0 || cost == 0 {
            continue;
        }
        cites.extend(cost_entries);
        cites.sort_unstable();
        let margin = rev - cost;
        let share = change_hundredths(rev, margin).map_or(0, |c| 10_000 + c);
        out.push(Finding {
            id: format!("margin:{client}"),
            detector: "client_margin",
            title: format!("{client}: {} margin after subcontracting", pct_text(share)),
            detail: format!(
                "{} billed, {} subcontracted for them",
                crowns(rev),
                crowns(cost)
            ),
            figures: vec![
                ("Billed".into(), rev),
                ("Subcontracted".into(), cost),
                ("Margin".into(), margin),
            ],
            percents: vec![("Margin".into(), whole_percent(share) * 100)],
            cites,
        });
    }
    out
}

/// Customers who pay late, by paid or still-open invoices.
pub fn late_payers(b: &Books, s: DetectorSettings) -> Vec<Finding> {
    let mut by: BTreeMap<&str, Vec<(&Sale, i64)>> = BTreeMap::new();
    for sale in &b.sales {
        let late = match (&sale.due, &sale.paid, sale.overdue_days) {
            (Some(due), Some(paid), _) => day(paid) - day(due),
            (_, None, Some(d)) => d,
            _ => continue,
        };
        if late >= s.late_days {
            by.entry(sale.client.as_str())
                .or_default()
                .push((sale, late));
        }
    }
    by.into_iter()
        .map(|(client, list)| {
            let worst = list.iter().map(|(_, d)| *d).max().unwrap_or(0);
            let numbers: Vec<&str> = list.iter().map(|(s, _)| s.number.as_str()).collect();
            Finding {
                id: format!("late:{client}"),
                detector: "late_payer",
                title: format!("{client} pays late"),
                detail: format!(
                    "{} invoice(s) {} or more days past due: {}; the longest {worst} days",
                    list.len(),
                    s.late_days,
                    numbers.join(", ")
                ),
                figures: Vec::new(),
                percents: Vec::new(),
                cites: list.iter().filter_map(|(s, _)| s.entry).collect(),
            }
        })
        .collect()
}

/// `2026-09-03` → `3 Sep 2026`.
fn short_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    match skyla_rules::date::parse(iso).map(skyla_rules::date::from_days) {
        Some((y, m, d)) => format!(
            "{d} {} {y}",
            MONTHS[usize::try_from(m - 1).unwrap_or(0) % 12]
        ),
        None => iso.to_owned(),
    }
}

fn day(iso: &str) -> i64 {
    skyla_rules::date::parse(iso).unwrap_or(0)
}

/// Recurring services (the same payee each month): growth, new ones and
/// charges that were duplicated.
pub fn subscriptions(
    b: &Books,
    prev: (&str, &str),
    cur: (&str, &str),
    s: DetectorSettings,
) -> Vec<Finding> {
    let key = |p: &Posting| {
        p.memo
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    };
    let recurring: Vec<&Posting> = b
        .expenses
        .iter()
        // Services that don't bill hours: known by the first word of the memo.
        .filter(|p| p.hours_hundredths.is_none() && p.account == "518")
        .collect();
    let mut out = Vec::new();
    // Duplicates: the same payee and amount twice within a few days.
    for (i, a) in recurring.iter().enumerate() {
        for z in &recurring[i + 1..] {
            if key(a) == key(z)
                && a.minor == z.minor
                && a.minor > 0
                && (day(&z.date) - day(&a.date)).abs() <= 3
            {
                // A refund names the service anywhere in its memo ("Storno … Figma").
                let refunded = recurring.iter().any(|r| {
                    r.minor == -a.minor
                        && day(&r.date) >= day(&z.date)
                        && r.memo.to_lowercase().contains(&key(a))
                });
                out.push(Finding {
                    id: format!("duplicate:{}:{}", key(a), z.entry),
                    detector: "subscription",
                    title: format!(
                        "{} charged twice{}",
                        a.memo.split_whitespace().next().unwrap_or(""),
                        if refunded { ", and refunded" } else { "" }
                    ),
                    detail: format!(
                        "{} on {} and {}",
                        crowns(a.minor),
                        short_date(&a.date),
                        short_date(&z.date)
                    ),
                    figures: vec![("Charge".into(), a.minor)],
                    percents: Vec::new(),
                    cites: vec![a.entry, z.entry],
                });
            }
        }
    }
    let total = |r| {
        recurring
            .iter()
            .filter(|p| in_range(&p.date, r))
            .map(|p| p.minor)
            .sum::<i64>()
    };
    let (a, z) = (total(prev), total(cur));
    if let Some(change) = change_hundredths(a, z)
        && change >= 500
    {
        let mut cites: Vec<i64> = recurring
            .iter()
            .filter(|p| in_range(&p.date, prev) || in_range(&p.date, cur))
            .map(|p| p.entry)
            .collect();
        cites.sort_unstable();
        out.push(Finding {
            id: "subscriptions".into(),
            detector: "subscription",
            title: format!(
                "Subscriptions cost {} more than the previous quarter",
                pct_text(change)
            ),
            detail: format!("{} → {}", crowns(a), crowns(z)),
            figures: vec![("Previous quarter".into(), a), ("This quarter".into(), z)],
            percents: vec![("Change".into(), whole_percent(change) * 100)],
            cites,
        });
    }
    let _ = s;
    out
}

/// Months of expenses the cash covers, at the recent monthly rate.
pub fn runway(b: &Books, cur: (&str, &str), s: DetectorSettings) -> Option<Finding> {
    let spent: i64 = b
        .expenses
        .iter()
        .filter(|p| in_range(&p.date, cur))
        .map(|p| p.minor)
        .sum();
    let months = 3;
    let monthly = spent / months;
    if monthly <= 0 {
        return None;
    }
    let covered_tenths = b.cash_minor * 10 / monthly;
    if covered_tenths >= s.runway_months * 10 {
        return None;
    }
    Some(Finding {
        id: "runway".into(),
        detector: "runway",
        title: format!(
            "Cash covers {},{} months of expenses",
            covered_tenths / 10,
            covered_tenths % 10
        ),
        detail: format!(
            "{} in the bank; {} spent a month on average this quarter",
            crowns(b.cash_minor),
            crowns(monthly)
        ),
        figures: vec![
            ("Cash".into(), b.cash_minor),
            ("Spent a month".into(), monthly),
        ],
        percents: Vec::new(),
        cites: Vec::new(),
    })
}

/// Every detector over `books` for the period `cur` against `prev`.
pub fn detect(
    b: &Books,
    prev: (&str, &str),
    cur: (&str, &str),
    s: DetectorSettings,
) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(subcontracting(b, prev, cur, s));
    out.extend(vendor_rates(b, prev, cur, s));
    out.extend(client_margins(b, cur));
    out.extend(late_payers(b, s));
    out.extend(subscriptions(b, prev, cur, s));
    out.extend(runway(b, cur, s));
    out
}

/// The last complete calendar quarter before `as_of`, and the one before it.
fn quarters(as_of: &str) -> ((String, String), (String, String)) {
    let y: i64 = as_of.get(..4).and_then(|v| v.parse().ok()).unwrap_or(2026);
    let m: i64 = as_of.get(5..7).and_then(|v| v.parse().ok()).unwrap_or(1);
    let q = (m - 1) / 3; // the current quarter, 0-based; the last complete is q - 1
    let (cy, cq) = if q == 0 { (y - 1, 3) } else { (y, q - 1) };
    let (py, pq) = if cq == 0 { (cy - 1, 3) } else { (cy, cq - 1) };
    let span = |y: i64, q: i64| {
        let start = skyla_rules::date::to_days(y, q * 3 + 1, 1);
        let end = if q == 3 {
            skyla_rules::date::to_days(y + 1, 1, 1)
        } else {
            skyla_rules::date::to_days(y, q * 3 + 4, 1)
        } - 1;
        (
            skyla_rules::date::format(start),
            skyla_rules::date::format(end),
        )
    };
    (span(py, pq), span(cy, cq))
}

impl Core {
    /// The books as the detectors see them.
    pub fn detector_books(&self) -> Result<Books, CoreError> {
        let year = self
            .domain
            .entity
            .as_of
            .get(..4)
            .unwrap_or("2026")
            .to_owned();
        let entries = skyla_ledger::list_posted(
            &self.db(),
            &format!("{}-01-01", year.parse::<i32>().unwrap_or(2026) - 1),
            &self.domain.entity.as_of,
        )?;
        let mut expenses = Vec::new();
        for e in &entries {
            let purchase = e
                .source_ref
                .as_deref()
                .and_then(|r| self.domain.purchases.iter().find(|p| p.reference == r));
            for l in &e.lines {
                if !l.account.starts_with('5') {
                    continue;
                }
                expenses.push(Posting {
                    entry: e.id,
                    date: e.date.clone(),
                    account: l.account.clone(),
                    minor: l.functional.minor(),
                    vendor: purchase.map(|p| p.supplier.clone()),
                    hours_hundredths: purchase
                        .and_then(|p| p.hours.as_deref())
                        .and_then(|h| h.parse::<i64>().ok())
                        .map(|h| h * 100),
                    // Invoices name customers by their legal name.
                    client: purchase.and_then(|p| p.client.as_deref()).map(|c| {
                        self.domain
                            .clients
                            .iter()
                            .find(|k| k.name == c)
                            .map_or_else(|| c.to_owned(), |k| k.legal_name.clone())
                    }),
                    memo: e.memo.clone(),
                });
            }
        }
        let sales = self
            .invoices()?
            .into_iter()
            .filter_map(|i| {
                Some(Sale {
                    number: i.number?,
                    client: i.client,
                    issued: i.issued_on?,
                    due: i.due_on,
                    paid: i.paid_on,
                    base_minor: i.base.minor,
                    entry: i.entry_id,
                    overdue_days: i.days_overdue,
                })
            })
            .collect();
        // Cash in hand and at the bank: the 21x accounts.
        let cash_minor = self
            .balance_sheet(&self.domain.entity.as_of)?
            .assets
            .iter()
            .filter(|l| l.code.starts_with("21") || l.code.starts_with("22"))
            .map(|l| l.amount.minor)
            .sum();
        Ok(Books {
            expenses,
            sales,
            cash_minor,
        })
    }

    /// The financial advisor's findings for the last complete quarter.
    pub fn financial_findings(&self) -> Result<Vec<FindingDto>, CoreError> {
        let (prev, cur) = quarters(&self.domain.entity.as_of);
        let books = self.detector_books()?;
        detect(
            &books,
            (&prev.0, &prev.1),
            (&cur.0, &cur.1),
            DetectorSettings::default(),
        )
        .into_iter()
        .map(|f| {
            Ok(FindingDto {
                id: f.id,
                detector: f.detector.into(),
                title: f.title,
                detail: f.detail,
                period: format!("{} – {}", cur.0, cur.1),
                figures: f
                    .figures
                    .into_iter()
                    .map(|(label, m)| {
                        Ok((
                            label,
                            MoneyDto::try_from(skyla_money::Money::new(m, self.currency))?,
                        ))
                    })
                    .collect::<Result<_, CoreError>>()?,
                percents: f
                    .percents
                    .into_iter()
                    .map(|(l, h)| (l, format!("{}", h / 100)))
                    .collect(),
                cites: f.cites,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_and_percentages() {
        assert_eq!(
            quarters("2026-10-07"),
            (
                ("2026-04-01".into(), "2026-06-30".into()),
                ("2026-07-01".into(), "2026-09-30".into())
            )
        );
        assert_eq!(
            quarters("2026-02-01").1,
            ("2025-10-01".into(), "2025-12-31".into())
        );
        // 74 700 → 106 100: +42,03 %.
        assert_eq!(change_hundredths(7_470_000, 10_610_000), Some(4203));
        assert_eq!(pct_text(4203), "42 %");
        assert_eq!(pct_text(1667), "17 %");
        assert_eq!(pct_text(-1250), "-13 %");
    }
}
