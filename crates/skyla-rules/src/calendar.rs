//! The obligations calendar: the pack's `[[obligation]]` entries expanded
//! into dated deadlines for a year, each moved to the next working day when
//! the pack says deadlines move.

use serde::{Deserialize, Serialize};

use crate::{Cite, Kind, Pack, RulesError, date};

/// How often an obligation recurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Every calendar month.
    Monthly,
    /// Every calendar quarter.
    Quarterly,
    /// Every calendar year.
    Yearly,
}

/// When an obligation for a period falls due: a number of days after the
/// period ends (a `days` value in the pack), or a day of a later month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Due {
    /// A `days` value key, counted from the period's last day.
    pub days_after_period: Option<String>,
    /// Months after the period's last month (with `day`).
    pub month_offset: Option<u8>,
    /// Day of that month, 1–28.
    pub day: Option<u8>,
}

/// A recurring duty: a return to file, an advance to pay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    /// Stable id, e.g. `vat.return.monthly`.
    pub id: String,
    /// For people.
    pub name: String,
    /// `file`, `pay` or `file_and_pay`.
    pub action: String,
    /// The entity facts it needs, all of them (e.g. `osvc`, `vat_monthly`).
    pub applies_to: Vec<String>,
    /// How often.
    pub frequency: Frequency,
    /// When.
    pub due: Due,
    /// Where it comes from.
    pub cite: Cite,
    /// Anything worth knowing.
    #[serde(default)]
    pub note: Option<String>,
}

/// One dated deadline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deadline {
    /// The obligation's id.
    pub obligation: String,
    /// Its name.
    pub name: String,
    /// `file`, `pay` or `file_and_pay`.
    pub action: String,
    /// `2026-09`, `2026-Q3` or `2025`.
    pub period: String,
    /// The period's last day.
    pub period_end: String,
    /// The date the rule gives, before any shift.
    pub nominal: String,
    /// The deadline.
    pub due: String,
    /// Its citation.
    pub cite: Cite,
}

const ACTIONS: [&str; 3] = ["file", "pay", "file_and_pay"];

fn month_end(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    date::to_days(ny, nm, 1) - 1
}

pub(crate) fn check(o: &Obligation, values_of_kind: impl Fn(&str) -> Option<Kind>) -> Vec<String> {
    let what = format!("obligation {}", o.id);
    let mut problems = Vec::new();
    if !ACTIONS.contains(&o.action.as_str()) {
        problems.push(format!(
            "{what}: action {:?} isn't file, pay or file_and_pay",
            o.action
        ));
    }
    if o.applies_to.is_empty() {
        problems.push(format!("{what}: applies to nobody"));
    }
    match (&o.due.days_after_period, o.due.month_offset, o.due.day) {
        (Some(key), None, None) => {
            if values_of_kind(key) != Some(Kind::Days) {
                problems.push(format!("{what}: {key} isn't a days value in the pack"));
            }
        }
        (None, Some(offset), Some(day)) => {
            if offset > 24 || !(1..=28).contains(&day) {
                problems.push(format!("{what}: month offset 0–24 and day 1–28"));
            }
        }
        _ => problems.push(format!(
            "{what}: due needs either days_after_period or month_offset with day"
        )),
    }
    problems
}

impl Pack {
    /// Moves `due` to the next working day when the pack says deadlines move.
    pub fn shift_deadline(&self, due: &str) -> Result<String, RulesError> {
        let bad = || RulesError::Invalid(vec![format!("{due:?} isn't a date")]);
        let mut day = date::parse(due).ok_or_else(bad)?;
        if self.flag("deadline.shift_to_next_working_day", due)? {
            while !self.is_working_day(&date::format(day)) {
                day += 1;
            }
        }
        Ok(date::format(day))
    }

    /// Every deadline falling due in `year` for an entity with `facts`,
    /// sorted by date.
    pub fn calendar(&self, year: i64, facts: &[&str]) -> Result<Vec<Deadline>, RulesError> {
        let mut out = Vec::new();
        for o in &self.obligations {
            if !o.applies_to.iter().all(|f| facts.contains(&f.as_str())) {
                continue;
            }
            // Periods ending from two years before through the year itself
            // cover every deadline the year can hold.
            let mut periods: Vec<(String, i64, i64)> = Vec::new(); // label, end, last month
            for y in (year - 2)..=year {
                match o.frequency {
                    Frequency::Monthly => {
                        for m in 1..=12 {
                            periods.push((format!("{y:04}-{m:02}"), month_end(y, m), m));
                        }
                    }
                    Frequency::Quarterly => {
                        for q in 1..=4 {
                            periods.push((format!("{y:04}-Q{q}"), month_end(y, q * 3), q * 3));
                        }
                    }
                    Frequency::Yearly => periods.push((format!("{y:04}"), month_end(y, 12), 12)),
                }
            }
            for (period, end, last_month) in periods {
                let period_end = date::format(end);
                let nominal = match (&o.due.days_after_period, o.due.month_offset, o.due.day) {
                    (Some(key), _, _) => end + i64::from(self.days(key, &period_end)?),
                    (None, Some(offset), Some(day)) => {
                        let (y, _, _) = date::from_days(end);
                        let months = last_month - 1 + i64::from(offset);
                        date::to_days(
                            y + months.div_euclid(12),
                            months.rem_euclid(12) + 1,
                            i64::from(day),
                        )
                    }
                    _ => continue,
                };
                let nominal = date::format(nominal);
                let due = self.shift_deadline(&nominal)?;
                if due.get(..4) != Some(format!("{year:04}").as_str()) {
                    continue;
                }
                out.push(Deadline {
                    obligation: o.id.clone(),
                    name: o.name.clone(),
                    action: o.action.clone(),
                    period,
                    period_end,
                    nominal,
                    due,
                    cite: o.cite.clone(),
                });
            }
        }
        out.sort_by(|a, b| (&a.due, &a.obligation).cmp(&(&b.due, &b.obligation)));
        Ok(out)
    }
}
