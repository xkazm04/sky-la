//! The periods the screens report on, from the books' date and VAT status
//! rather than the UI's assumptions (improvement wave 4): the last VAT
//! periods to file, the quarters that have ended, the year to date, and the
//! obligations year. The demo's date gives the demo's periods.

use super::Core;
use crate::dto::{PeriodChoiceDto, ReportingPeriodsDto};
use crate::error::CoreError;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn ymd(day: &str) -> Option<(i64, u32, u32)> {
    let d = skyla_rules::date::parse(day)?;
    let (y, m, dd) = skyla_rules::date::from_days(d);
    Some((y, u32::try_from(m).ok()?, u32::try_from(dd).ok()?))
}

fn month_end(y: i64, m: u32) -> String {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let first_next = skyla_rules::date::parse(&format!("{ny:04}-{nm:02}-01")).unwrap_or(0);
    skyla_rules::date::format(first_next - 1)
}

fn name(names: &[&'static str; 12], m: u32) -> &'static str {
    usize::try_from(m.saturating_sub(1))
        .ok()
        .and_then(|i| names.get(i).copied())
        .unwrap_or("")
}

fn month(y: i64, m: u32) -> PeriodChoiceDto {
    PeriodChoiceDto {
        id: format!("{y:04}-{m:02}"),
        label: format!("{} {y}", name(&MONTHS, m)),
        from: format!("{y:04}-{m:02}-01"),
        to: month_end(y, m),
    }
}

fn quarter(y: i64, q: u32) -> PeriodChoiceDto {
    PeriodChoiceDto {
        id: format!("{y:04}-Q{q}"),
        label: format!("Q{q} {y}"),
        from: format!("{y:04}-{:02}-01", q * 3 - 2),
        to: month_end(y, q * 3),
    }
}

/// A calendar period for people: `2026-09` → `September 2026`, `2026-Q3`
/// → `Q3 2026`; a year stays as it is.
pub(crate) fn label(period: &str) -> String {
    if let Some((y, q)) = period.split_once("-Q") {
        return format!("Q{q} {y}");
    }
    match period.split_once('-') {
        Some((y, m)) => m.parse::<u32>().map_or_else(
            |_| period.to_owned(),
            |m| format!("{} {y}", name(&MONTHS, m)),
        ),
        None => period.to_owned(),
    }
}

impl Core {
    /// What the screens report on, as of the books' date.
    pub fn reporting_periods(&self) -> Result<ReportingPeriodsDto, CoreError> {
        let today = self.domain.entity.as_of.clone();
        let (y, m, _) = ymd(&today)
            .ok_or_else(|| CoreError::BadRequest("the books' date isn't a date".into()))?;
        let books_from = skyla_ledger::list_periods(&self.db())?
            .into_iter()
            .map(|p| p.starts_on)
            .min()
            .unwrap_or_else(|| format!("{y:04}-01-01"));
        let overlaps = |p: &PeriodChoiceDto| p.to >= books_from && p.to < today;

        // The last three VAT periods that have ended, newest first.
        let mut vat = Vec::new();
        match self.domain.entity.vat_period.as_str() {
            "monthly" => {
                let (mut yy, mut mm) = (y, m);
                while vat.len() < 3 {
                    (yy, mm) = if mm == 1 { (yy - 1, 12) } else { (yy, mm - 1) };
                    let p = month(yy, mm);
                    if p.to < books_from {
                        break;
                    }
                    vat.push(p);
                }
            }
            "quarterly" => {
                let (mut yy, mut q) = (y, m.div_ceil(3));
                while vat.len() < 3 {
                    (yy, q) = if q == 1 { (yy - 1, 4) } else { (yy, q - 1) };
                    let p = quarter(yy, q);
                    if p.to < books_from {
                        break;
                    }
                    vat.push(p);
                }
            }
            _ => {}
        }

        // The last two quarters that have ended and overlap the books.
        let mut quarters = Vec::new();
        let (mut yy, mut q) = (y, m.div_ceil(3));
        while quarters.len() < 2 {
            (yy, q) = if q == 1 { (yy - 1, 4) } else { (yy, q - 1) };
            let p = quarter(yy, q);
            if !overlaps(&p) {
                break;
            }
            quarters.insert(0, p);
        }
        let last = quarters.last().cloned();
        // The year of the last ended quarter, from its start (or the books').
        let year_to_date = match &last {
            Some(lq) => {
                let year = &lq.to[..4];
                let from = std::cmp::max(format!("{year}-01-01"), books_from.clone());
                let (_, fm, _) = ymd(&from).unwrap_or((y, 1, 1));
                let (_, tm, _) = ymd(&lq.to).unwrap_or((y, 12, 31));
                PeriodChoiceDto {
                    id: format!("{year}-ytd"),
                    label: format!("{} – {} {year}", name(&SHORT, fm), name(&SHORT, tm)),
                    from,
                    to: lq.to.clone(),
                }
            }
            // No quarter has ended since the books began: the books so far.
            None => PeriodChoiceDto {
                id: format!("{y:04}-ytd"),
                label: format!("{} {y} so far", name(&SHORT, m)),
                from: books_from.clone(),
                to: today.clone(),
            },
        };
        let prior = if quarters.len() == 2 {
            Some(quarters[0].clone())
        } else {
            None
        };
        Ok(ReportingPeriodsDto {
            today,
            year: y,
            books_from,
            vat,
            quarters,
            last_quarter: last,
            prior_quarter: prior,
            year_to_date,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{month, quarter};

    #[test]
    fn months_and_quarters_end_on_their_last_day() {
        assert_eq!(month(2026, 2).to, "2026-02-28");
        assert_eq!(month(2028, 2).to, "2028-02-29");
        assert_eq!(month(2026, 12).to, "2026-12-31");
        assert_eq!(quarter(2026, 3).from, "2026-07-01");
        assert_eq!(quarter(2026, 3).to, "2026-09-30");
    }
}
