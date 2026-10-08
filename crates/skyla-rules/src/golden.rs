//! Golden tests for a rule pack (WP-34): what the pack must answer, written
//! by hand from the cited texts, ideally by someone other than the pack's
//! author. `golden.toml` sits beside `pack.toml`; `skyla-pack check` and the
//! crate's tests run it. Every value key, obligation and holiday must be
//! covered by at least one case, so a value can't enter a pack unchecked.
//!
//! ```toml
//! [[value]]                      # the value in force on a day
//! key = "vat.rate.standard"
//! on = "2026-06-30"
//! expect = "21"
//!
//! [[value]]                      # a known disagreement awaiting a fix:
//! key = "some.key"               # reported as open, not failed, and
//! on = "2026-06-30"              # failed once the pack agrees, so the
//! expect = "55"                  # note gets removed
//! open = "why, with the source"
//!
//! [[working_day]]                # whether a day is a working day
//! date = "2026-04-03"
//! expect = false                 # Velký pátek
//!
//! [[deadline]]                   # when an obligation's period falls due
//! obligation = "vat.return.monthly"
//! period = "2026-03"
//! due = "2026-04-27"             # the 25th is a Saturday
//!
//! [[vat_code]]                   # a VAT code's rate on a day
//! code = "OUT12"
//! on = "2026-06-30"
//! expect = "12"
//! ```

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::{Kind, Pack, RulesError, parse_amount, parse_rounding};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenFile {
    #[serde(default, rename = "value")]
    values: Vec<ValueCase>,
    #[serde(default, rename = "working_day")]
    working_days: Vec<WorkingDayCase>,
    #[serde(default, rename = "deadline")]
    deadlines: Vec<DeadlineCase>,
    #[serde(default, rename = "vat_code")]
    vat_codes: Vec<VatCodeCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueCase {
    key: String,
    on: String,
    expect: String,
    /// A known disagreement with the pack, awaiting a fix: reported, not failed.
    open: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkingDayCase {
    date: String,
    expect: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeadlineCase {
    obligation: String,
    period: String,
    due: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VatCodeCase {
    code: String,
    on: String,
    expect: String,
}

/// What a golden run found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoldenReport {
    /// Cases run.
    pub cases: usize,
    /// Cases whose answer differed, described.
    pub failures: Vec<String>,
    /// Value keys, obligations and holidays no case covers.
    pub uncovered: Vec<String>,
    /// Cases marked `open`: where the reviewer expects the pack to change.
    pub open: Vec<String>,
}

impl GoldenReport {
    /// Every case passed and everything is covered.
    pub fn passed(&self) -> bool {
        self.failures.is_empty() && self.uncovered.is_empty()
    }
}

/// Whether `written` and `expect` are the same value of `kind`.
fn same(kind: Kind, written: &str, expect: &str, pack: &Pack) -> bool {
    match kind {
        Kind::Percent => match (
            written.parse::<crate::Rate>(),
            expect.parse::<crate::Rate>(),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        },
        Kind::Amount => match skyla_money::Currency::from_code(&pack.info.currency) {
            Ok(c) => {
                parse_amount(written, c).is_some()
                    && parse_amount(written, c) == parse_amount(expect, c)
            }
            Err(_) => false,
        },
        Kind::Rounding => {
            parse_rounding(written).is_some() && parse_rounding(written) == parse_rounding(expect)
        }
        Kind::Days | Kind::Flag => written.trim() == expect.trim(),
    }
}

/// Runs `golden_toml` against `pack`.
pub fn run(pack: &Pack, golden_toml: &str) -> Result<GoldenReport, RulesError> {
    let file: GoldenFile =
        toml::from_str(golden_toml).map_err(|e| RulesError::Format(e.to_string()))?;
    let mut report = GoldenReport::default();

    let mut keys = BTreeSet::new();
    for c in &file.values {
        report.cases += 1;
        keys.insert(c.key.as_str());
        let answer = pack.value(&c.key, &c.on);
        let agrees = matches!(&answer, Ok(v) if same(v.kind, &v.value, &c.expect, pack));
        match (&answer, agrees, &c.open) {
            (Ok(_), true, None) => {}
            (Ok(_), true, Some(_)) => report.failures.push(format!(
                "value {} on {}: marked open, but the pack now agrees; remove `open`",
                c.key, c.on
            )),
            (Ok(v), false, Some(why)) => report.open.push(format!(
                "value {} on {}: the pack says {}, the reviewer expects {} ({why})",
                c.key, c.on, v.value, c.expect
            )),
            (Ok(v), false, None) => report.failures.push(format!(
                "value {} on {}: the pack says {}, expected {}",
                c.key, c.on, v.value, c.expect
            )),
            (Err(e), _, _) => report
                .failures
                .push(format!("value {} on {}: {e}", c.key, c.on)),
        }
    }

    let mut holidays = BTreeSet::new();
    for c in &file.working_days {
        report.cases += 1;
        if let Some(h) = pack.holiday(&c.date) {
            holidays.insert(h.name.as_str());
        }
        let working = pack.is_working_day(&c.date);
        if working != c.expect {
            report.failures.push(format!(
                "{}: the pack says {}a working day, expected {}",
                c.date,
                if working { "" } else { "not " },
                if c.expect { "one" } else { "a day off" }
            ));
        }
    }

    let mut obligations = BTreeSet::new();
    for c in &file.deadlines {
        report.cases += 1;
        obligations.insert(c.obligation.as_str());
        let Some(o) = pack.obligations.iter().find(|o| o.id == c.obligation) else {
            report
                .failures
                .push(format!("deadline {}: no such obligation", c.obligation));
            continue;
        };
        let facts: Vec<&str> = o.applies_to.iter().map(String::as_str).collect();
        let year: i64 = c.period.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0);
        let mut found = None;
        for y in [year, year + 1] {
            if let Ok(list) = pack.calendar(y, &facts)
                && let Some(d) = list
                    .into_iter()
                    .find(|d| d.obligation == c.obligation && d.period == c.period)
            {
                found = Some(d);
                break;
            }
        }
        match found {
            Some(d) if d.due == c.due => {}
            Some(d) => report.failures.push(format!(
                "deadline {} for {}: the pack says {} (nominally {}), expected {}",
                c.obligation, c.period, d.due, d.nominal, c.due
            )),
            None => report.failures.push(format!(
                "deadline {} for {}: the pack's calendar has no such period",
                c.obligation, c.period
            )),
        }
    }

    let mut codes = BTreeSet::new();
    for c in &file.vat_codes {
        report.cases += 1;
        codes.insert(c.code.as_str());
        match (
            pack.vat_rate(&c.code, &c.on),
            c.expect.parse::<crate::Rate>(),
        ) {
            (Ok(rate), Ok(expect)) if rate == expect => {}
            (Ok(rate), _) => report.failures.push(format!(
                "VAT code {} on {}: the pack says {rate} %, expected {}",
                c.code, c.on, c.expect
            )),
            (Err(e), _) => report
                .failures
                .push(format!("VAT code {} on {}: {e}", c.code, c.on)),
        }
    }

    for key in pack.keys() {
        if !keys.contains(key) {
            report.uncovered.push(format!("value {key}"));
        }
    }
    for o in &pack.obligations {
        if !obligations.contains(o.id.as_str()) {
            report.uncovered.push(format!("obligation {}", o.id));
        }
    }
    for h in &pack.holidays {
        if !holidays.contains(h.name.as_str()) {
            report.uncovered.push(format!("holiday {}", h.name));
        }
    }
    for v in &pack.vat_codes {
        if !codes.contains(v.code.as_str()) {
            report.uncovered.push(format!("VAT code {}", v.code));
        }
    }
    Ok(report)
}
