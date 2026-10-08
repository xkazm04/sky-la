//! ABO / GPC, the fixed-width export of Czech banks: a `074` header per
//! statement and a `075` record per line, 128 characters each, usually in
//! Windows-1250. Amounts are in haléře without separators.
//!
//! Layout of `075` (1-based columns): account 4–19, counter-account 20–35,
//! document number 36–48, amount 49–60, posting code 61 (1 debit, 2 credit,
//! 4 debit reversal, 5 credit reversal), VS 62–71, `00` + counter bank
//! 72–77 + KS 78–81, SS 82–91, value date 92–97, counter-account name
//! 98–117, `0` 118, currency 119–122 (ISO numeric), booking date 123–128.
//! Some banks store account numbers in an internal digit order; they are
//! kept as written and flagged.

use skyla_money::{Currency, Money};

use crate::model::{Account, BankError, BankLine, Format, Statement, malformed};
use crate::text::{ddmmyy, non_empty, symbol};

const F: Format = Format::Gpc;

/// True when the first record is a `074` header.
pub(crate) fn looks_like(text: &str) -> bool {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.starts_with("074") && l.trim_end().chars().count() >= 100)
}

struct Record(Vec<char>);

impl Record {
    /// 1-based inclusive columns.
    fn get(&self, from: usize, to: usize) -> String {
        self.0
            .get(from.saturating_sub(1)..to.min(self.0.len()))
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    }
}

fn haler(text: &str, sign: &str, currency: Currency, line: usize) -> Result<Money, BankError> {
    let digits = text.trim();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 15 {
        return Err(malformed(
            F,
            Some(line),
            format!("unreadable amount {text:?}"),
        ));
    }
    let minor: i64 = digits
        .parse()
        .map_err(|_| malformed(F, Some(line), format!("unreadable amount {text:?}")))?;
    let minor = if sign == "-" { -minor } else { minor };
    // GPC amounts are in hundredths whatever the currency.
    let scale = 2_u32.saturating_sub(u32::from(currency.minor_units()));
    Ok(Money::new(minor / 10_i64.pow(scale), currency))
}

/// `000019` + `2000145399` → `19-2000145399`.
fn account(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 16 {
        return non_empty(text);
    }
    let prefix = digits.get(0..6)?.trim_start_matches('0');
    let number = digits.get(6..16)?.trim_start_matches('0');
    if number.is_empty() {
        return None;
    }
    Some(if prefix.is_empty() {
        number.to_owned()
    } else {
        format!("{prefix}-{number}")
    })
}

fn currency_numeric(code: &str) -> Option<Currency> {
    let alpha = match code.trim().trim_start_matches('0') {
        "203" => "CZK",
        "978" => "EUR",
        "840" => "USD",
        "826" => "GBP",
        "985" => "PLN",
        "756" => "CHF",
        _ => return None,
    };
    Currency::from_code(alpha).ok()
}

/// Reads every statement in an ABO/GPC file.
pub fn parse_gpc(text: &str) -> Result<Vec<Statement>, BankError> {
    let mut out: Vec<Statement> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim_end_matches(['\r', '\u{1a}']);
        if line.trim().is_empty() {
            continue;
        }
        let r = Record(line.chars().collect());
        match r.get(1, 3).as_str() {
            "074" => {
                // Currency comes with the lines; CZK until one says otherwise.
                let czk = Currency::CZK;
                let opening = haler(&r.get(46, 59), &r.get(60, 60), czk, n)?;
                let closing = haler(&r.get(61, 74), &r.get(75, 75), czk, n)?;
                out.push(Statement {
                    format: F,
                    account: Account {
                        iban: None,
                        number: account(&r.get(4, 19)),
                        bank_code: None,
                    },
                    number: non_empty(&r.get(106, 108))
                        .map(|s| s.trim_start_matches('0').to_owned()),
                    from: ddmmyy(&r.get(40, 45)),
                    to: ddmmyy(&r.get(109, 114)),
                    opening: Some(opening),
                    closing: Some(closing),
                    lines: Vec::new(),
                    warnings: Vec::new(),
                });
            }
            "075" => {
                let s = out
                    .last_mut()
                    .ok_or_else(|| malformed(F, Some(n), "a 075 record before any 074 header"))?;
                let currency = currency_numeric(&r.get(119, 122)).unwrap_or(Currency::CZK);
                let raw_amount = haler(&r.get(49, 60), "+", currency, n)?;
                let (sign, reversal) = match r.get(61, 61).as_str() {
                    "1" => (-1, false),
                    "2" => (1, false),
                    "4" => (1, true),
                    "5" => (-1, true),
                    other => {
                        return Err(malformed(
                            F,
                            Some(n),
                            format!("unknown posting code {other:?}"),
                        ));
                    }
                };
                let amount = if sign < 0 {
                    raw_amount
                        .checked_neg()
                        .map_err(|_| malformed(F, Some(n), "amount out of range"))?
                } else {
                    raw_amount
                };
                let ks_field = r.get(72, 81);
                let bank = ks_field
                    .get(2..6)
                    .and_then(|b| non_empty(b).filter(|b| b != "0000"));
                let counter = account(&r.get(20, 35)).map(|a| match &bank {
                    Some(b) => format!("{a}/{b}"),
                    None => a,
                });
                let value_date = ddmmyy(&r.get(92, 97));
                let booking_date = ddmmyy(&r.get(123, 128))
                    .or_else(|| value_date.clone())
                    .ok_or_else(|| malformed(F, Some(n), "a line without a date"))?;
                s.lines.push(BankLine {
                    sequence: u32::try_from(s.lines.len() + 1).unwrap_or(u32::MAX),
                    booking_date,
                    value_date,
                    amount,
                    reversal,
                    counterparty_name: non_empty(&r.get(98, 117)),
                    counterparty_account: counter,
                    vs: symbol(&r.get(62, 71)),
                    ks: ks_field.get(6..10).and_then(symbol),
                    ss: symbol(&r.get(82, 91)),
                    message: None,
                    bank_ref: non_empty(&r.get(36, 48)).filter(|d| d.chars().any(|c| c != '0')),
                });
            }
            // Message records some banks add after a 075.
            "076" | "078" | "079" => {
                if let Some(last) = out.last_mut().and_then(|s| s.lines.last_mut()) {
                    let text = r.get(4, 128);
                    if let Some(t) = non_empty(&text) {
                        last.message = Some(match last.message.take() {
                            Some(m) => format!("{m} {t}"),
                            None => t,
                        });
                    }
                }
            }
            other => {
                return Err(malformed(
                    F,
                    Some(n),
                    format!("unknown record type {other:?}"),
                ));
            }
        }
    }
    if out.is_empty() {
        return Err(malformed(F, None, "no 074 header"));
    }
    // A header's balances take the currency of its lines.
    for s in &mut out {
        if let Some(c) = s.lines.first().map(|l| l.amount.currency())
            && c != Currency::CZK
        {
            s.opening = s.opening.map(|m| Money::new(m.minor(), c));
            s.closing = s.closing.map(|m| Money::new(m.minor(), c));
        }
        let first = s.lines.first().map(|l| l.amount.currency());
        if s.lines.iter().any(|l| Some(l.amount.currency()) != first) {
            return Err(malformed(
                F,
                None,
                "lines in more than one currency in one statement",
            ));
        }
    }
    Ok(out)
}
