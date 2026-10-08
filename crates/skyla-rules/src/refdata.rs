//! Public reference data the user imports, or fetches when they opt in:
//! the ČNB's daily exchange rates and the history of its repo rate. This
//! isn't pack data (it changes daily); every set records where it came from.

use serde::{Deserialize, Serialize};
use skyla_money::Rate;

use crate::date;

/// Why a reference file was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct RefDataError {
    /// 1-based line.
    pub line: usize,
    /// What's wrong.
    pub message: String,
}

fn err(line: usize, message: impl Into<String>) -> RefDataError {
    RefDataError {
        line,
        message: message.into(),
    }
}

fn decimal(text: &str) -> Option<Rate> {
    let t = text.trim().replace(',', ".");
    if t.is_empty()
        || !t.chars().all(|c| c.is_ascii_digit() || c == '.')
        || t.matches('.').count() > 1
    {
        return None;
    }
    t.parse().ok()
}

/// `07.10.2026` → `2026-10-07`.
fn dmy(text: &str) -> Option<String> {
    let mut parts = text.trim().split('.');
    let (d, m, y) = (parts.next()?, parts.next()?, parts.next()?);
    let s = format!("{y:0>4}-{m:0>2}-{d:0>2}");
    date::parse(&s).map(|_| s)
}

/// One currency's rate on a day: `amount` units cost `rate` CZK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxRate {
    /// ISO 4217 code.
    pub code: String,
    /// Units the rate is quoted for (100 for JPY).
    pub amount: u32,
    /// CZK for `amount` units.
    pub rate: Rate,
}

impl FxRate {
    /// CZK for one unit.
    pub fn per_unit(&self) -> Rate {
        self.rate / Rate::from(self.amount.max(1))
    }
}

/// The ČNB's rates for one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxDay {
    /// The day they're valid for.
    pub date: String,
    /// The ČNB's sequence number for the year.
    pub number: u32,
    /// The rates.
    pub rates: Vec<FxRate>,
}

/// Parses the ČNB's daily file (`denni_kurz.txt`):
///
/// ```text
/// 07.10.2026 #194
/// země|měna|množství|kód|kurz
/// EMU|euro|1|EUR|25,140
/// ```
pub fn parse_cnb_daily(text: &str) -> Result<FxDay, RefDataError> {
    let mut lines = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let (_, head) = lines.next().ok_or_else(|| err(1, "the file is empty"))?;
    let (day, number) = head
        .split_once('#')
        .ok_or_else(|| err(1, "expected `DD.MM.YYYY #N` on the first line"))?;
    let date = dmy(day).ok_or_else(|| err(1, format!("{:?} isn't a date", day.trim())))?;
    let number = number
        .trim()
        .parse()
        .map_err(|_| err(1, "the sequence number isn't a number"))?;
    let (_, columns) = lines.next().ok_or_else(|| err(2, "no column header"))?;
    if columns.split('|').count() != 5 {
        return Err(err(2, "expected five columns: země|měna|množství|kód|kurz"));
    }
    let mut rates = Vec::new();
    for (i, line) in lines {
        let n = i + 1;
        let cells: Vec<&str> = line.split('|').collect();
        let [_, _, amount, code, rate] = cells.as_slice() else {
            return Err(err(n, "expected five columns"));
        };
        let code = code.trim();
        if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(err(n, format!("{code:?} isn't a currency code")));
        }
        let amount: u32 = amount
            .trim()
            .parse()
            .map_err(|_| err(n, "the amount isn't a whole number"))?;
        let rate = decimal(rate)
            .filter(|r| *r > Rate::ZERO)
            .ok_or_else(|| err(n, format!("{:?} isn't a rate", rate.trim())))?;
        if amount == 0 {
            return Err(err(n, "an amount of zero"));
        }
        rates.push(FxRate {
            code: code.to_owned(),
            amount,
            rate,
        });
    }
    if rates.is_empty() {
        return Err(err(3, "no rates in the file"));
    }
    Ok(FxDay {
        date,
        number,
        rates,
    })
}

/// The rate valid on `on`: the latest day on or before it (the ČNB
/// publishes on working days; a weekend uses Friday's rates).
pub fn fx_on<'a>(days: &'a [FxDay], code: &str, on: &str) -> Option<(&'a FxDay, &'a FxRate)> {
    days.iter()
        .filter(|d| d.date.as_str() <= on)
        .max_by(|a, b| a.date.cmp(&b.date))
        .and_then(|d| d.rates.iter().find(|r| r.code == code).map(|r| (d, r)))
}

/// A repo rate in force from a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoChange {
    /// `YYYY-MM-DD`.
    pub effective_from: String,
    /// Annual percent.
    pub rate: Rate,
}

/// Parses a repo-rate history: one change per line, a date (`YYYY-MM-DD`
/// or `DD.MM.YYYY`) and a percent, separated by `;`, `|`, a tab or a comma
/// followed by a space. Lines without a date (headers) are skipped.
pub fn parse_repo_history(text: &str) -> Result<Vec<RepoChange>, RefDataError> {
    let mut out: Vec<RepoChange> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line
            .split([';', '|', '\t'])
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        let [day, rate, ..] = cells.as_slice() else {
            continue;
        };
        let effective_from = if day.contains('-') {
            date::parse(day).map(|_| (*day).to_owned())
        } else {
            dmy(day)
        };
        let Some(effective_from) = effective_from else {
            if out.is_empty() {
                continue; // a header
            }
            return Err(err(n, format!("{day:?} isn't a date")));
        };
        let rate = decimal(rate.trim_end_matches('%'))
            .ok_or_else(|| err(n, format!("{rate:?} isn't a percent")))?;
        if rate > Rate::from(100) {
            return Err(err(n, "a repo rate above 100 %"));
        }
        out.push(RepoChange {
            effective_from,
            rate,
        });
    }
    if out.is_empty() {
        return Err(err(1, "no dated rates in the file"));
    }
    out.sort_by(|a, b| a.effective_from.cmp(&b.effective_from));
    if out
        .windows(2)
        .any(|w| w.first().map(|a| &a.effective_from) == w.get(1).map(|b| &b.effective_from))
    {
        return Err(err(1, "two rates for the same day"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAILY: &str = "07.10.2026 #194\nzemě|měna|množství|kód|kurz\nEMU|euro|1|EUR|25,140\nJaponsko|jen|100|JPY|15,987\nUSA|dolar|1|USD|21,455\n";

    #[test]
    fn reads_the_cnb_daily_file() {
        let day = parse_cnb_daily(DAILY).expect("parse");
        assert_eq!(
            (day.date.as_str(), day.number, day.rates.len()),
            ("2026-10-07", 194, 3)
        );
        let (_, jpy) = fx_on(std::slice::from_ref(&day), "JPY", "2026-10-10").expect("jpy");
        assert_eq!(jpy.per_unit(), "0.15987".parse::<Rate>().expect("rate"));
        assert!(
            fx_on(std::slice::from_ref(&day), "EUR", "2026-10-06").is_none(),
            "no rates before the day"
        );
        assert!(
            parse_cnb_daily("07.10.2026 #194\nzemě|měna|množství|kód|kurz\nEMU|euro|1|EU|25,140")
                .is_err()
        );
        assert!(
            parse_cnb_daily("07.10.2026 #194\nzemě|měna|množství|kód|kurz\nEMU|euro|1|EUR|-25,140")
                .is_err()
        );
    }

    #[test]
    fn reads_a_repo_history() {
        let h = parse_repo_history("platnost od;sazba\n02.05.2025;3,50\n2024-12-20;4.00 %\n")
            .expect("parse");
        assert_eq!(
            h.iter()
                .map(|c| c.effective_from.as_str())
                .collect::<Vec<_>>(),
            ["2024-12-20", "2025-05-02"]
        );
        assert_eq!(h[1].rate, "3.5".parse::<Rate>().expect("rate"));
        assert!(parse_repo_history("2025-05-02;3,50\n2025-05-02;3,25").is_err());
        assert!(parse_repo_history("2025-05-02;3,50\nnot a date;3,25").is_err());
    }
}
