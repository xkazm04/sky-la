//! SWIFT MT940 customer statements.
//!
//! Fields start with `:NN:` or `:NNa:` and continue on following lines. A
//! file may hold several statements (each from `:20:`), with or without the
//! SWIFT `{1:…}{4:` envelope. `:86:` follows its `:61:` and is read in the
//! two layouts Czech banks use: `?20`–`?29` subfields, or `/VS/…/KS/…` tags
//! and free text.

use skyla_money::{Currency, Money};

use crate::model::{Account, BankError, BankLine, Format, Statement, malformed};
use crate::text::{amount, non_empty, symbols_in, yymmdd};

const F: Format = Format::Mt940;

/// True when the text has the fields of an MT940 statement.
pub(crate) fn looks_like(text: &str) -> bool {
    text.contains(":20:")
        && text.contains(":25:")
        && (text.contains(":60F:") || text.contains(":60M:"))
}

struct Field {
    tag: String,
    value: String,
    line: usize,
}

fn fields(text: &str) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let tagged = line
            .strip_prefix(':')
            .and_then(|rest| rest.split_once(':'))
            .filter(|(tag, _)| {
                (2..=3).contains(&tag.len())
                    && tag.chars().take(2).all(|c| c.is_ascii_digit())
                    && tag.chars().nth(2).is_none_or(|c| c.is_ascii_uppercase())
            });
        if let Some((tag, value)) = tagged {
            out.push(Field {
                tag: tag.to_owned(),
                value: value.to_owned(),
                line: i + 1,
            });
        } else if line.trim() == "-"
            || line.trim() == "-}"
            || line.starts_with('{') && !line.contains(":20:")
        {
            // End of a message, or the SWIFT envelope.
            out.push(Field {
                tag: "-".into(),
                value: String::new(),
                line: i + 1,
            });
        } else if let Some(last) = out.last_mut().filter(|f| f.tag != "-") {
            last.value.push('\n');
            last.value.push_str(line);
        }
    }
    out
}

/// `C260901CZK1000,00` → (date, signed amount).
fn balance(value: &str, line: usize) -> Result<(String, Money), BankError> {
    let bad = || malformed(F, Some(line), format!("unreadable balance {value:?}"));
    let mark = value.get(0..1).ok_or_else(bad)?;
    let date = yymmdd(value.get(1..7).ok_or_else(bad)?).ok_or_else(bad)?;
    let currency = Currency::from_code(value.get(7..10).ok_or_else(bad)?).map_err(|_| bad())?;
    let amt = amount(value.get(10..).ok_or_else(bad)?.trim(), ',', currency).ok_or_else(bad)?;
    let amt = match mark {
        "C" => amt,
        "D" => amt.checked_neg().map_err(|_| bad())?,
        _ => return Err(bad()),
    };
    Ok((date, amt))
}

struct Sixty1 {
    date: String,
    amount: Money,
    reversal: bool,
    customer_ref: Option<String>,
    bank_ref: Option<String>,
    details: Option<String>,
}

fn line_61(value: &str, currency: Currency, line: usize) -> Result<Sixty1, BankError> {
    let bad = |why: &str| malformed(F, Some(line), format!("{why} in :61: {value:?}"));
    let (first, details) = value.split_once('\n').unwrap_or((value, ""));
    let chars: Vec<char> = first.chars().collect();
    let take = |from: usize, to: usize| -> String {
        chars
            .get(from..to)
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    };
    let date = yymmdd(&take(0, 6)).ok_or_else(|| bad("no value date"))?;
    let mut at = 6;
    // An optional entry date (MMDD).
    if chars
        .get(6..10)
        .is_some_and(|c| c.iter().all(char::is_ascii_digit))
    {
        at = 10;
    }
    let (sign, reversal, mark_len) = match (chars.get(at), chars.get(at + 1)) {
        (Some('R'), Some('C')) => (-1, true, 2),
        (Some('R'), Some('D')) => (1, true, 2),
        (Some('C'), _) => (1, false, 1),
        (Some('D'), _) => (-1, false, 1),
        _ => return Err(bad("no debit/credit mark")),
    };
    at += mark_len;
    // An optional funds code: one letter before the amount.
    if chars.get(at).is_some_and(|c| c.is_ascii_alphabetic()) {
        at += 1;
    }
    let start = at;
    while chars
        .get(at)
        .is_some_and(|c| c.is_ascii_digit() || *c == ',')
    {
        at += 1;
    }
    let amt = amount(&take(start, at), ',', currency).ok_or_else(|| bad("no amount"))?;
    let amt = if sign < 0 {
        amt.checked_neg()
            .map_err(|_| bad("an amount out of range"))?
    } else {
        amt
    };
    // Transaction type (4), then the customer reference, `//`, the bank's.
    let rest = take(at.saturating_add(4).min(chars.len()), chars.len());
    let (customer, bank) = match rest.split_once("//") {
        Some((c, b)) => (c, Some(b)),
        None => (rest.as_str(), None),
    };
    Ok(Sixty1 {
        date,
        amount: amt,
        reversal,
        customer_ref: non_empty(customer).filter(|c| c != "NONREF"),
        bank_ref: bank.and_then(non_empty),
        details: non_empty(details),
    })
}

/// What `:86:` says: message, name, account, and the text to find symbols in.
fn info_86(value: &str) -> (Option<String>, Option<String>, Option<String>, String) {
    let flat = value.replace('\n', "");
    if flat.contains("?20") || flat.contains("?32") {
        let mut sub: Vec<(String, String)> = Vec::new();
        for part in flat.split('?').skip(1) {
            let (code, text) = (part.get(0..2).unwrap_or(""), part.get(2..).unwrap_or(""));
            sub.push((code.to_owned(), text.to_owned()));
        }
        let pick = |codes: &[&str]| -> Option<String> {
            let joined: String = sub
                .iter()
                .filter(|(c, _)| codes.contains(&c.as_str()))
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("");
            non_empty(&joined)
        };
        let message = pick(&["20", "21", "22", "23", "24", "25", "26", "27", "28", "29"]);
        let name = pick(&["32", "33"]);
        let account = match (pick(&["31"]), pick(&["30"])) {
            (Some(a), Some(b)) => Some(format!("{a}/{}", b.trim())),
            (a, _) => a,
        };
        // Each subfield on its own, so `?21VS2026097` reads as `VS2026097`.
        let symbols = sub
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        return (message, name, account, symbols);
    }
    let mut message = Vec::new();
    let mut name = None;
    let mut account = None;
    // `/TAG/value` pairs, as some banks write them.
    if flat.starts_with('/') {
        let parts: Vec<&str> = flat.split('/').collect();
        let mut i = 1;
        while i + 1 < parts.len() {
            let (tag, val) = (
                parts.get(i).copied().unwrap_or(""),
                parts.get(i + 1).copied().unwrap_or(""),
            );
            match tag {
                "NAME" | "BENM" | "ORDP" => name = non_empty(val),
                "ACC" | "ACCW" => account = non_empty(val),
                "REMI" | "MSG" | "TXT" => message.push(val.trim().to_owned()),
                _ => {}
            }
            i += 2;
        }
    }
    if message.is_empty() && name.is_none() {
        message.push(value.replace('\n', " ").trim().to_owned());
    }
    let symbols = flat.replace('/', " ");
    (non_empty(&message.join(" ")), name, account, symbols)
}

fn account_25(value: &str) -> Account {
    let v = value.trim();
    let compact: String = v.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() >= 15
        && compact.chars().take(2).all(|c| c.is_ascii_uppercase())
        && compact.chars().skip(2).all(|c| c.is_ascii_alphanumeric())
    {
        return Account {
            iban: Some(compact),
            ..Account::default()
        };
    }
    if let Some((a, b)) = compact.split_once('/') {
        let (number, bank) = if a.len() == 4 && b.len() > 4 {
            (b, a)
        } else {
            (a, b)
        };
        return Account {
            iban: None,
            number: non_empty(number),
            bank_code: non_empty(bank),
        };
    }
    Account {
        number: non_empty(&compact),
        ..Account::default()
    }
}

/// Reads every statement in an MT940 file.
pub fn parse_mt940(text: &str) -> Result<Vec<Statement>, BankError> {
    let all = fields(text);
    let mut out = Vec::new();
    let mut current: Option<Statement> = None;
    let mut currency: Option<Currency> = None;
    for f in &all {
        match f.tag.as_str() {
            "20" => {
                if let Some(s) = current.take() {
                    out.push(finish(s));
                }
                currency = None;
                current = Some(Statement {
                    format: F,
                    account: Account::default(),
                    number: None,
                    from: None,
                    to: None,
                    opening: None,
                    closing: None,
                    lines: Vec::new(),
                    warnings: Vec::new(),
                });
            }
            "-" => {
                if let Some(s) = current.take() {
                    out.push(finish(s));
                }
            }
            tag => {
                let Some(s) = current.as_mut() else {
                    continue; // envelope fields before :20:
                };
                match tag {
                    "25" => s.account = account_25(&f.value),
                    "28" | "28C" => s.number = non_empty(&f.value),
                    "60F" | "60M" => {
                        let (date, amt) = balance(&f.value, f.line)?;
                        currency = Some(amt.currency());
                        if s.opening.is_none() {
                            s.opening = Some(amt);
                            s.from = Some(date);
                        }
                    }
                    "62F" | "62M" => {
                        let (date, amt) = balance(&f.value, f.line)?;
                        s.closing = Some(amt);
                        s.to = Some(date);
                    }
                    "61" => {
                        let ccy = currency.ok_or_else(|| {
                            malformed(F, Some(f.line), ":61: before the opening balance :60F:")
                        })?;
                        let l = line_61(&f.value, ccy, f.line)?;
                        s.lines.push(BankLine {
                            sequence: u32::try_from(s.lines.len() + 1).unwrap_or(u32::MAX),
                            booking_date: l.date.clone(),
                            value_date: Some(l.date),
                            amount: l.amount,
                            reversal: l.reversal,
                            counterparty_name: None,
                            counterparty_account: None,
                            vs: None,
                            ks: None,
                            ss: None,
                            message: l.details,
                            bank_ref: l.bank_ref.or(l.customer_ref),
                        });
                    }
                    "86" => {
                        let Some(last) = s.lines.last_mut() else {
                            continue; // statement-level information
                        };
                        let (message, name, account, symbols) = info_86(&f.value);
                        let (vs, ks, ss) = symbols_in(&symbols);
                        last.message = message.or(last.message.take());
                        last.counterparty_name = name;
                        last.counterparty_account = account;
                        last.vs = vs;
                        last.ks = ks;
                        last.ss = ss;
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(s) = current.take() {
        out.push(finish(s));
    }
    if out.is_empty() {
        return Err(malformed(F, None, "no statement (:20:) in the file"));
    }
    for s in &out {
        if s.opening.is_none() {
            return Err(malformed(
                F,
                None,
                "a statement without an opening balance (:60F:)",
            ));
        }
    }
    Ok(out)
}

fn finish(mut s: Statement) -> Statement {
    if s.closing.is_none() {
        s.warnings
            .push("no closing balance (:62F:); the statement can't be tied out".into());
    }
    s
}
