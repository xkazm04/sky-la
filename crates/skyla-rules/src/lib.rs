//! Rule-pack loader: versioned, effective-dated, cited statutory data per jurisdiction.
//!
//! A pack (`rules/<cc>/<year>/pack.toml`) holds every rate, threshold,
//! deadline and form mapping the engine needs, each with the provision it
//! comes from. Logic asks the pack for a value *on a date*; it never
//! hard-codes one. [`Pack::from_toml`] validates the whole pack and reports
//! every problem at once, so a bad pack is refused before anything uses it.

#![deny(clippy::float_arithmetic)]

mod calendar;
pub mod date;
pub mod golden;
pub mod refdata;
mod update;

pub use calendar::{Deadline, Due, Frequency, Obligation};
pub use update::{UpdateError, verify_pack_update};

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use skyla_money::{Currency, Money, Rate, RoundingMode, parse_amount_cs};

/// The CZ pack for 2026, compiled in so the app always has one.
pub const CZ_2026: &str = include_str!("../../../rules/cz/2026/pack.toml");

/// Errors from loading or querying a pack.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RulesError {
    /// The TOML doesn't parse.
    #[error("rule pack is not valid TOML: {0}")]
    Format(String),
    /// The pack is inconsistent; every problem is listed.
    #[error("invalid rule pack:\n  {}", .0.join("\n  "))]
    Invalid(Vec<String>),
    /// No value with this key is effective on the date.
    #[error("rule pack {pack} has no value {key} effective on {date}")]
    Missing {
        /// Pack id.
        pack: String,
        /// Value key.
        key: String,
        /// Date asked for.
        date: String,
    },
    /// The value exists but isn't of the kind asked for.
    #[error("rule pack value {key} is a {actual}, not a {expected}")]
    WrongKind {
        /// Value key.
        key: String,
        /// Its kind.
        actual: String,
        /// The kind asked for.
        expected: &'static str,
    },
    /// No VAT code with this name.
    #[error("rule pack {pack} has no VAT code {code}")]
    UnknownVatCode {
        /// Pack id.
        pack: String,
        /// The code.
        code: String,
    },
}

/// How far a pack has been checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    /// Compiled from the cited provisions, not yet independently verified.
    Draft,
    /// Verified line by line against the official texts.
    Reviewed,
}

/// Pack metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackInfo {
    /// Stable id, e.g. `cz-2026`.
    pub id: String,
    /// Pack version, recorded in every output that used it.
    pub version: String,
    /// ISO 3166 country code.
    pub jurisdiction: String,
    /// First day the pack applies to.
    pub valid_from: String,
    /// Last day it applies to.
    pub valid_to: String,
    /// Review status.
    pub review: Review,
    /// Currency of its amounts.
    pub currency: String,
    /// One line about its scope.
    pub summary: String,
    /// What the pack deliberately leaves out, and why.
    #[serde(default)]
    pub omitted: Vec<String>,
}

/// A legal source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Act {
    /// Short id used in citations.
    pub id: String,
    /// Full name.
    pub name: String,
    /// Where to read it.
    pub url: String,
}

/// A citation: an act and the provision within it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cite {
    /// An [`Act`] id.
    pub act: String,
    /// The provision, e.g. `§ 47 odst. 1 písm. a)`.
    pub section: String,
}

/// What a value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A percentage such as `21`.
    Percent,
    /// An amount in the pack currency, written `2000000.00`.
    Amount,
    /// A number of days.
    Days,
    /// A rounding mode: `half_up`, `half_even`, `toward_zero`, `away_from_zero`.
    Rounding,
    /// `true` or `false`.
    Flag,
}

impl Kind {
    /// The kind's name in the pack, e.g. `percent`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::Amount => "amount",
            Self::Days => "days",
            Self::Rounding => "rounding",
            Self::Flag => "flag",
        }
    }
}

/// One effective-dated, cited value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Value {
    /// Dotted key, e.g. `vat.rate.standard`.
    pub key: String,
    /// What it is.
    pub kind: Kind,
    /// The value as written.
    pub value: String,
    /// First day it applies.
    pub effective_from: String,
    /// Last day it applies; open-ended when absent.
    pub effective_to: Option<String>,
    /// Where it comes from.
    pub cite: Cite,
    /// Context for people.
    pub note: Option<String>,
}

/// Which part of a VAT code's postings feeds a form row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowPart {
    /// The tax base (lines on non-tax accounts).
    Base,
    /// All tax lines.
    Tax,
    /// Debit tax lines.
    TaxDebit,
    /// Credit tax lines.
    TaxCredit,
}

/// One row a VAT code feeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowMapping {
    /// Row as printed on the form.
    pub row: String,
    /// Which part.
    pub part: RowPart,
    /// Show credits as positive.
    pub credit_positive: bool,
}

/// A VAT code: its rate and where it lands on the return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VatCode {
    /// The posting's `vat_code`.
    pub code: String,
    /// For people.
    pub name: String,
    /// The key of its rate value.
    pub rate: String,
    /// Where the mapping comes from.
    pub cite: Cite,
    /// The rows it feeds.
    pub rows: Vec<RowMapping>,
    /// Outside VAT altogether (a supplier not registered for VAT): no tax,
    /// no return rows.
    #[serde(default)]
    pub outside_vat: bool,
    /// How sales under this code appear on an EN 16931 e-invoice; none for
    /// codes that never go on a sales document (purchases).
    #[serde(default)]
    pub einvoice: Option<EinvoiceTax>,
    /// Set for a supply to a VAT payer in another member state: the
    /// customer's VAT number is required and the supply goes into the
    /// souhrnné hlášení (EC Sales List).
    #[serde(default)]
    pub eu_supply: Option<EuSupply>,
    /// What the printed invoice must say under this code, in both languages.
    #[serde(default)]
    pub invoice_note: Option<InvoiceNote>,
}

/// An EU supply's place in the souhrnné hlášení.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EuSupply {
    /// The statement's supply code (kód plnění): `0` goods, `1` transfer of
    /// own assets, `2` triangular trade, `3` services.
    pub sh_code: String,
    /// Where the code comes from.
    pub cite: Cite,
}

/// The statement an invoice prints for a VAT code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvoiceNote {
    /// In Czech.
    pub cs: String,
    /// In English.
    pub en: String,
}

/// The EN 16931 view of a VAT code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EinvoiceTax {
    /// VAT category (UNTDID 5305 subset of EN 16931): `S`, `Z`, `E`, `AE`,
    /// `K`, `G`, `O`, `L` or `M`.
    pub category: String,
    /// Why no VAT is charged; required for every category except `S` and `Z`.
    #[serde(default)]
    pub exemption_reason: Option<String>,
}

/// A public holiday: a fixed `MM-DD` date or an offset from Easter Sunday.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holiday {
    /// Its name.
    pub name: String,
    /// `MM-DD` for fixed holidays.
    pub date: Option<String>,
    /// Days from Easter Sunday for movable ones.
    pub easter_offset: Option<i64>,
    /// Where it comes from.
    pub cite: Cite,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    pack: PackInfo,
    #[serde(rename = "act", default)]
    acts: Vec<Act>,
    #[serde(rename = "value", default)]
    values: Vec<Value>,
    #[serde(rename = "vat_code", default)]
    vat_codes: Vec<VatCode>,
    #[serde(rename = "holiday", default)]
    holidays: Vec<Holiday>,
    #[serde(rename = "obligation", default)]
    obligations: Vec<Obligation>,
}

/// A validated rule pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pack {
    /// Metadata.
    pub info: PackInfo,
    /// Legal sources by id.
    pub acts: BTreeMap<String, Act>,
    values: HashMap<String, Vec<Value>>,
    /// VAT codes in pack order.
    pub vat_codes: Vec<VatCode>,
    /// Public holidays.
    pub holidays: Vec<Holiday>,
    /// Recurring returns and payments, for the calendar.
    pub obligations: Vec<Obligation>,
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s.get(r).and_then(|p| p.parse::<u32>().ok());
    match (num(0..4), num(5..7), num(8..10)) {
        (Some(y), Some(m), Some(d)) => {
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let days = match m {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 if leap => 29,
                2 => 28,
                _ => return false,
            };
            (1..=days).contains(&d)
        }
        _ => false,
    }
}

fn parse_rounding(text: &str) -> Option<RoundingMode> {
    Some(match text {
        "half_up" => RoundingMode::HalfUp,
        "half_even" => RoundingMode::HalfEven,
        "toward_zero" => RoundingMode::TowardZero,
        "away_from_zero" => RoundingMode::AwayFromZero,
        _ => return None,
    })
}

/// `2000000.00` → minor units.
fn parse_amount(text: &str, currency: Currency) -> Option<Money> {
    parse_amount_cs(&text.replace('.', ","), currency).ok()
}

impl Pack {
    /// Parses and validates a pack.
    pub fn from_toml(text: &str) -> Result<Self, RulesError> {
        let file: PackFile = toml::from_str(text).map_err(|e| RulesError::Format(e.to_string()))?;
        let mut problems = Vec::new();
        let info = file.pack;
        for (label, date) in [
            ("valid_from", &info.valid_from),
            ("valid_to", &info.valid_to),
        ] {
            if !is_date(date) {
                problems.push(format!("pack {label} {date:?} isn't a date"));
            }
        }
        if info.valid_from > info.valid_to {
            problems.push("pack valid_from is after valid_to".into());
        }
        let currency = match Currency::from_code(&info.currency) {
            Ok(c) => Some(c),
            Err(_) => {
                problems.push(format!("unknown pack currency {}", info.currency));
                None
            }
        };

        let mut acts = BTreeMap::new();
        for act in file.acts {
            if act.name.trim().is_empty() || !act.url.starts_with("https://") {
                problems.push(format!("act {}: needs a name and an https URL", act.id));
            }
            if acts.insert(act.id.clone(), act.clone()).is_some() {
                problems.push(format!("act {}: duplicate id", act.id));
            }
        }
        let check_cite = |what: &str, cite: &Cite, problems: &mut Vec<String>| {
            if !acts.contains_key(&cite.act) {
                problems.push(format!("{what}: cites unknown act {}", cite.act));
            }
            if cite.section.trim().is_empty() {
                problems.push(format!("{what}: citation has no section"));
            }
        };

        let mut values: HashMap<String, Vec<Value>> = HashMap::new();
        for v in file.values {
            let what = format!("value {}", v.key);
            check_cite(&what, &v.cite, &mut problems);
            if !is_date(&v.effective_from) {
                problems.push(format!(
                    "{what}: effective_from {:?} isn't a date",
                    v.effective_from
                ));
            }
            if let Some(to) = &v.effective_to {
                if !is_date(to) {
                    problems.push(format!("{what}: effective_to {to:?} isn't a date"));
                } else if to < &v.effective_from {
                    problems.push(format!("{what}: effective_to is before effective_from"));
                }
            }
            let parses = match v.kind {
                Kind::Percent => v.value.parse::<Rate>().is_ok_and(|r| !r.is_sign_negative()),
                Kind::Amount => currency.is_some_and(|c| parse_amount(&v.value, c).is_some()),
                Kind::Days => v.value.parse::<u16>().is_ok(),
                Kind::Rounding => parse_rounding(&v.value).is_some(),
                Kind::Flag => matches!(v.value.as_str(), "true" | "false"),
            };
            if !parses {
                problems.push(format!(
                    "{what}: {:?} isn't a valid {}",
                    v.value,
                    v.kind.name()
                ));
            }
            values.entry(v.key.clone()).or_default().push(v);
        }
        for (key, list) in &mut values {
            list.sort_by(|a, b| a.effective_from.cmp(&b.effective_from));
            if list.iter().any(|v| v.kind != list[0].kind) {
                problems.push(format!("value {key}: every period must have the same kind"));
            }
            for pair in list.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                match &a.effective_to {
                    Some(to) if to < &b.effective_from => {}
                    _ => problems.push(format!(
                        "value {key}: periods from {} and {} overlap",
                        a.effective_from, b.effective_from
                    )),
                }
            }
        }

        let mut codes = std::collections::HashSet::new();
        for code in &file.vat_codes {
            let what = format!("VAT code {}", code.code);
            check_cite(&what, &code.cite, &mut problems);
            if !codes.insert(code.code.clone()) {
                problems.push(format!("{what}: duplicate"));
            }
            match values.get(&code.rate) {
                Some(list) if list.iter().all(|v| v.kind == Kind::Percent) => {}
                _ => problems.push(format!(
                    "{what}: rate {} isn't a percent value in the pack",
                    code.rate
                )),
            }
            if let Some(e) = &code.einvoice {
                const CATEGORIES: [&str; 9] = ["S", "Z", "E", "AE", "K", "G", "O", "L", "M"];
                if !CATEGORIES.contains(&e.category.as_str()) {
                    problems.push(format!(
                        "{what}: e-invoice category {:?} isn't an EN 16931 VAT category",
                        e.category
                    ));
                }
                let exempt = !matches!(e.category.as_str(), "S" | "Z" | "L" | "M");
                if exempt && e.exemption_reason.as_deref().is_none_or(str::is_empty) {
                    problems.push(format!(
                        "{what}: e-invoice category {} needs an exemption reason",
                        e.category
                    ));
                }
                if code.outside_vat != (e.category == "O") {
                    problems.push(format!(
                        "{what}: category O is for codes outside VAT, and only for them"
                    ));
                }
            }
            match (code.outside_vat, code.rows.is_empty()) {
                (false, true) => problems.push(format!("{what}: maps to no rows")),
                (true, false) => {
                    problems.push(format!("{what}: is outside VAT but maps to return rows"));
                }
                _ => {}
            }
            if let Some(eu) = &code.eu_supply {
                check_cite(&format!("{what} (eu_supply)"), &eu.cite, &mut problems);
                if !matches!(eu.sh_code.as_str(), "0" | "1" | "2" | "3") {
                    problems.push(format!(
                        "{what}: souhrnné hlášení code {:?} isn't 0, 1, 2 or 3",
                        eu.sh_code
                    ));
                }
                if code.outside_vat {
                    problems.push(format!("{what}: an EU supply can't be outside VAT"));
                }
                // The customer accounts for the tax or the supply is exempt;
                // either way the invoice charges none and says why.
                match code.einvoice.as_ref().map(|e| e.category.as_str()) {
                    Some("K" | "AE") => {}
                    _ => problems.push(format!(
                        "{what}: an EU supply is e-invoice category K or AE"
                    )),
                }
                if code.invoice_note.is_none() {
                    problems.push(format!("{what}: an EU supply needs an invoice_note"));
                }
            }
            if let Some(note) = &code.invoice_note
                && (note.cs.trim().is_empty() || note.en.trim().is_empty())
            {
                problems.push(format!("{what}: invoice_note needs both languages"));
            }
        }

        for h in &file.holidays {
            let what = format!("holiday {}", h.name);
            check_cite(&what, &h.cite, &mut problems);
            match (&h.date, h.easter_offset) {
                (Some(md), None) if date::parse(&format!("2024-{md}")).is_some() => {}
                (None, Some(offset)) if (-60..=60).contains(&offset) => {}
                _ => problems.push(format!(
                    "{what}: needs either a valid MM-DD date or an Easter offset"
                )),
            }
        }

        let mut ids = std::collections::HashSet::new();
        for o in &file.obligations {
            check_cite(&format!("obligation {}", o.id), &o.cite, &mut problems);
            if !ids.insert(o.id.clone()) {
                problems.push(format!("obligation {}: duplicate", o.id));
            }
            problems.extend(calendar::check(o, |key| {
                values.get(key).and_then(|l| l.first()).map(|v| v.kind)
            }));
        }

        if !problems.is_empty() {
            return Err(RulesError::Invalid(problems));
        }
        Ok(Self {
            info,
            acts,
            values,
            vat_codes: file.vat_codes,
            holidays: file.holidays,
            obligations: file.obligations,
        })
    }

    /// The CZ 2026 pack.
    pub fn cz_2026() -> Result<Self, RulesError> {
        Self::from_toml(CZ_2026)
    }

    /// `cz-2026@2026.1`: what outputs record as their provenance.
    pub fn provenance(&self) -> String {
        format!("{}@{}", self.info.id, self.info.version)
    }

    /// The value of `key` effective on `date` (`YYYY-MM-DD`), with its citation.
    pub fn value(&self, key: &str, date: &str) -> Result<&Value, RulesError> {
        self.values
            .get(key)
            .and_then(|list| {
                list.iter().find(|v| {
                    v.effective_from.as_str() <= date
                        && v.effective_to.as_deref().is_none_or(|to| date <= to)
                })
            })
            .ok_or_else(|| RulesError::Missing {
                pack: self.info.id.clone(),
                key: key.to_owned(),
                date: date.to_owned(),
            })
    }

    fn typed(&self, key: &str, date: &str, kind: Kind) -> Result<&Value, RulesError> {
        let v = self.value(key, date)?;
        if v.kind == kind {
            Ok(v)
        } else {
            Err(RulesError::WrongKind {
                key: key.to_owned(),
                actual: v.kind.name().to_owned(),
                expected: kind.name(),
            })
        }
    }

    /// A percentage, e.g. `21` for `vat.rate.standard`.
    pub fn percent(&self, key: &str, date: &str) -> Result<Rate, RulesError> {
        // Validated at load.
        Ok(self
            .typed(key, date, Kind::Percent)?
            .value
            .parse()
            .unwrap_or_default())
    }

    /// An amount in the pack currency.
    pub fn amount(&self, key: &str, date: &str) -> Result<Money, RulesError> {
        let v = self.typed(key, date, Kind::Amount)?;
        let currency = Currency::from_code(&self.info.currency).unwrap_or(Currency::CZK);
        Ok(parse_amount(&v.value, currency).unwrap_or(Money::zero(currency)))
    }

    /// A number of days.
    pub fn days(&self, key: &str, date: &str) -> Result<u16, RulesError> {
        Ok(self
            .typed(key, date, Kind::Days)?
            .value
            .parse()
            .unwrap_or_default())
    }

    /// A rounding mode.
    pub fn rounding(&self, key: &str, date: &str) -> Result<RoundingMode, RulesError> {
        Ok(
            parse_rounding(&self.typed(key, date, Kind::Rounding)?.value)
                .unwrap_or(RoundingMode::HalfUp),
        )
    }

    /// A flag.
    pub fn flag(&self, key: &str, date: &str) -> Result<bool, RulesError> {
        Ok(self.typed(key, date, Kind::Flag)?.value == "true")
    }

    /// A VAT code.
    pub fn vat_code(&self, code: &str) -> Result<&VatCode, RulesError> {
        self.vat_codes
            .iter()
            .find(|c| c.code == code)
            .ok_or_else(|| RulesError::UnknownVatCode {
                pack: self.info.id.clone(),
                code: code.to_owned(),
            })
    }

    /// The rate of a VAT code on a date.
    pub fn vat_rate(&self, code: &str, date: &str) -> Result<Rate, RulesError> {
        let rate_key = self.vat_code(code)?.rate.clone();
        self.percent(&rate_key, date)
    }

    /// A human-readable citation, e.g. `Zákon č. 235/2004 Sb., … § 47 odst. 1 písm. a)`.
    pub fn citation(&self, cite: &Cite) -> String {
        match self.acts.get(&cite.act) {
            Some(act) => format!("{}, {}", act.name, cite.section),
            None => cite.section.clone(),
        }
    }

    /// The holiday on `date`, if it is one.
    pub fn holiday(&self, iso: &str) -> Option<&Holiday> {
        let days = date::parse(iso)?;
        let (year, _, _) = date::from_days(days);
        let easter = date::easter(year);
        self.holidays
            .iter()
            .find(|h| match (&h.date, h.easter_offset) {
                (Some(md), _) => iso.get(5..) == Some(md.as_str()),
                (None, Some(offset)) => easter + offset == days,
                _ => false,
            })
    }

    /// True for a weekday that isn't a public holiday.
    pub fn is_working_day(&self, iso: &str) -> bool {
        date::parse(iso).is_some_and(|d| date::weekday(d) < 5) && self.holiday(iso).is_none()
    }

    /// A deadline `days_key` days after `period_end`, moved to the next
    /// working day when the pack says so. Returns `YYYY-MM-DD`.
    pub fn deadline_after(&self, days_key: &str, period_end: &str) -> Result<String, RulesError> {
        let bad = || RulesError::Invalid(vec![format!("{period_end:?} isn't a date")]);
        let end = date::parse(period_end).ok_or_else(bad)?;
        let days = i64::from(self.days(days_key, period_end)?);
        let mut due = date::format(end + days);
        if self.flag("deadline.shift_to_next_working_day", &due)? {
            while !self.is_working_day(&due) {
                due = date::format(date::parse(&due).ok_or_else(bad)? + 1);
            }
        }
        Ok(due)
    }

    /// Every value key, sorted.
    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.values.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }
}
