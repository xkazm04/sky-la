//! CSV exports, read through a column profile.
//!
//! Banks' CSV exports differ in delimiter, encoding, column names, date and
//! number formats, and in whether amounts are signed or split into debit
//! and credit. A profile names the columns; the header row is found by
//! looking for them, so preamble lines (account details) are skipped, and
//! `key;value` preamble pairs can give the opening and closing balance.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use skyla_money::{Currency, Money};

use crate::model::{Account, BankError, BankLine, Format, Statement, malformed};
use crate::text::{amount, any_date, non_empty, symbol, symbols_in};

const F: Format = Format::Csv;

/// How amounts are laid out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Amounts {
    /// One signed column.
    Signed(String),
    /// Separate columns for money out and money in (both positive).
    DebitCredit {
        /// Money out.
        debit: String,
        /// Money in.
        credit: String,
    },
}

/// A saved description of one bank's CSV export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CsvProfile {
    /// For people, e.g. "Fio banka".
    pub name: String,
    /// Field delimiter.
    pub delimiter: char,
    /// Decimal separator in amounts.
    pub decimal: char,
    /// The booking date column.
    pub date: String,
    /// Amounts.
    pub amounts: Amounts,
    /// A currency column; otherwise `default_currency`.
    pub currency: Option<String>,
    /// The currency when no column gives one.
    pub default_currency: String,
    /// Optional columns.
    pub value_date: Option<String>,
    /// The other party's name.
    pub counterparty_name: Option<String>,
    /// The other party's account number.
    pub counterparty_account: Option<String>,
    /// The other party's bank code, joined to the account as `number/bank`.
    pub counterparty_bank: Option<String>,
    /// Variable symbol.
    pub vs: Option<String>,
    /// Constant symbol.
    pub ks: Option<String>,
    /// Specific symbol.
    pub ss: Option<String>,
    /// Columns joined into the message, in order.
    pub message: Vec<String>,
    /// The bank's id for the line.
    pub bank_ref: Option<String>,
    /// Preamble keys (`key;value` lines before the header) for the balances.
    pub opening_key: Option<String>,
    /// See `opening_key`.
    pub closing_key: Option<String>,
}

impl CsvProfile {
    /// Fio banka's CSV export.
    pub fn fio() -> Self {
        let s = |t: &str| Some(t.to_owned());
        Self {
            name: "Fio banka".into(),
            delimiter: ';',
            decimal: ',',
            date: "Datum".into(),
            amounts: Amounts::Signed("Objem".into()),
            currency: s("Měna"),
            default_currency: "CZK".into(),
            value_date: None,
            counterparty_name: s("Název protiúčtu"),
            counterparty_account: s("Protiúčet"),
            counterparty_bank: s("Kód banky"),
            vs: s("VS"),
            ks: s("KS"),
            ss: s("SS"),
            message: vec!["Zpráva pro příjemce".into(), "Komentář".into()],
            bank_ref: s("ID pohybu"),
            opening_key: s("openingBalance"),
            closing_key: s("closingBalance"),
        }
    }

    fn required(&self) -> Vec<&str> {
        let mut r = vec![self.date.as_str()];
        match &self.amounts {
            Amounts::Signed(c) => r.push(c),
            Amounts::DebitCredit { debit, credit } => {
                r.push(debit);
                r.push(credit);
            }
        }
        r
    }
}

fn norm(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_lowercase()
}

/// Reads a CSV export with `profile`.
pub fn parse_csv(text: &str, profile: &CsvProfile) -> Result<Vec<Statement>, BankError> {
    let delimiter = u8::try_from(profile.delimiter)
        .map_err(|_| malformed(F, None, "the profile's delimiter isn't a single byte"))?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let required: Vec<String> = profile.required().into_iter().map(norm).collect();
    let mut header: Option<HashMap<String, usize>> = None;
    let mut preamble: HashMap<String, String> = HashMap::new();
    let default_currency = Currency::from_code(&profile.default_currency).map_err(|_| {
        malformed(
            F,
            None,
            format!("unknown currency {}", profile.default_currency),
        )
    })?;
    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    let mut currency: Option<Currency> = None;
    for (i, record) in reader.records().enumerate() {
        let n = i + 1;
        let record = record.map_err(|e| malformed(F, Some(n), e.to_string()))?;
        let cells: Vec<&str> = record.iter().collect();
        let Some(cols) = &header else {
            let names: HashMap<String, usize> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| (norm(c), i))
                .collect();
            if required.iter().all(|r| names.contains_key(r)) {
                header = Some(names);
            } else if let [key, value, ..] = cells.as_slice() {
                preamble.insert(norm(key), value.trim().to_owned());
            }
            continue;
        };
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let cell = |name: &Option<String>| -> Option<String> {
            let idx = cols.get(&norm(name.as_deref()?))?;
            cells.get(*idx).and_then(|c| non_empty(c))
        };
        let get = |name: &str| cell(&Some(name.to_owned()));
        let line_currency = match cell(&profile.currency) {
            Some(c) => Currency::from_code(&c)
                .map_err(|_| malformed(F, Some(n), format!("unknown currency {c}")))?,
            None => default_currency,
        };
        if *currency.get_or_insert(line_currency) != line_currency {
            return Err(malformed(F, Some(n), "lines in more than one currency"));
        }
        let bad_amount = |v: &str| malformed(F, Some(n), format!("unreadable amount {v:?}"));
        let amt: Money = match &profile.amounts {
            Amounts::Signed(c) => {
                let v = get(c).ok_or_else(|| malformed(F, Some(n), format!("no {c}")))?;
                amount(&v, profile.decimal, line_currency).ok_or_else(|| bad_amount(&v))?
            }
            Amounts::DebitCredit { debit, credit } => {
                let read = |c: &str| -> Result<Money, BankError> {
                    match get(c) {
                        Some(v) => amount(&v, profile.decimal, line_currency)
                            .map(|m| Money::new(m.minor().abs(), line_currency))
                            .ok_or_else(|| bad_amount(&v)),
                        None => Ok(Money::zero(line_currency)),
                    }
                };
                read(credit)?
                    .checked_sub(read(debit)?)
                    .map_err(|_| bad_amount(""))?
            }
        };
        let date_text = get(&profile.date).ok_or_else(|| malformed(F, Some(n), "no date"))?;
        let booking_date = any_date(&date_text)
            .ok_or_else(|| malformed(F, Some(n), format!("unreadable date {date_text:?}")))?;
        let message: Vec<String> = profile.message.iter().filter_map(|c| get(c)).collect();
        let message = non_empty(&message.join(" "));
        let (vs_text, ks_text, ss_text) = symbols_in(message.as_deref().unwrap_or(""));
        let account =
            cell(&profile.counterparty_account).map(|a| match cell(&profile.counterparty_bank) {
                Some(b)
                    if !a.contains('/') && !a.starts_with(|c: char| c.is_ascii_alphabetic()) =>
                {
                    format!("{a}/{b}")
                }
                _ => a,
            });
        lines.push(BankLine {
            sequence: u32::try_from(lines.len() + 1).unwrap_or(u32::MAX),
            booking_date,
            value_date: cell(&profile.value_date).and_then(|d| any_date(&d)),
            amount: amt,
            reversal: false,
            counterparty_name: cell(&profile.counterparty_name),
            counterparty_account: account,
            vs: cell(&profile.vs).and_then(|v| symbol(&v)).or(vs_text),
            ks: cell(&profile.ks).and_then(|v| symbol(&v)).or(ks_text),
            ss: cell(&profile.ss).and_then(|v| symbol(&v)).or(ss_text),
            message,
            bank_ref: cell(&profile.bank_ref),
        });
    }
    if header.is_none() {
        return Err(malformed(
            F,
            None,
            format!(
                "no header row with the columns {} (profile {})",
                profile.required().join(", "),
                profile.name
            ),
        ));
    }
    let currency = currency.unwrap_or(default_currency);
    let balance = |key: &Option<String>| -> Option<Money> {
        let v = preamble.get(&norm(key.as_deref()?))?;
        amount(v, profile.decimal, currency).or_else(|| amount(v, '.', currency))
    };
    let (opening, closing) = (balance(&profile.opening_key), balance(&profile.closing_key));
    if closing.is_none() {
        warnings
            .push("the export gives no closing balance; the statement can't be tied out".into());
    }
    let dates = || lines.iter().map(|l: &BankLine| l.booking_date.clone());
    Ok(vec![Statement {
        format: F,
        account: Account {
            iban: preamble.get("iban").and_then(|v| non_empty(v)),
            number: preamble.get("accountid").and_then(|v| non_empty(v)),
            bank_code: preamble.get("bankid").and_then(|v| non_empty(v)),
        },
        number: None,
        from: preamble
            .get("datestart")
            .and_then(|d| any_date(d))
            .or_else(|| dates().min()),
        to: preamble
            .get("dateend")
            .and_then(|d| any_date(d))
            .or_else(|| dates().max()),
        opening,
        closing,
        lines,
        warnings,
    }])
}
