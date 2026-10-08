//! Amounts, dates, encodings and payment symbols, shared by the parsers.

use skyla_money::{Currency, Money};

/// Parses a decimal amount (`1234.50`, `1 234,50`, `-12`) into minor units of
/// `currency`, exactly. `decimal` is the decimal separator.
pub(crate) fn amount(text: &str, decimal: char, currency: Currency) -> Option<Money> {
    let t = text.trim();
    let (negative, rest) = match t.strip_prefix('-').or_else(|| t.strip_prefix('\u{2212}')) {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    // Grouping marks (spaces, apostrophes, or the separator that isn't the
    // decimal one) only count in groups of three digits: `84,700.00` is
    // 84 700, but `12.5` with a decimal comma is refused, not read as 125.
    let group = if decimal == ',' { '.' } else { ',' };
    let (whole_raw, frac) = match rest.split_once(decimal) {
        Some((w, f)) => (w, f),
        None => (rest, ""),
    };
    let marks = |c: char| matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'') || c == group;
    let chunks: Vec<&str> = whole_raw.split(marks).collect();
    let grouped = chunks.len() > 1;
    if grouped
        && (chunks.first().is_none_or(|c| c.is_empty() || c.len() > 3)
            || chunks.iter().skip(1).any(|c| c.len() != 3))
    {
        return None;
    }
    let whole_owned: String = chunks.concat();
    let whole = whole_owned.as_str();
    let units = u32::from(currency.minor_units());
    if whole.is_empty() && frac.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > units as usize
        || whole.len() > 15
    {
        return None;
    }
    let mut minor: i64 = 0;
    for c in whole.chars() {
        minor = minor
            .checked_mul(10)?
            .checked_add(i64::from(c.to_digit(10)?))?;
    }
    let mut frac_digits = 0u32;
    for c in frac.chars() {
        minor = minor
            .checked_mul(10)?
            .checked_add(i64::from(c.to_digit(10)?))?;
        frac_digits += 1;
    }
    for _ in frac_digits..units {
        minor = minor.checked_mul(10)?;
    }
    Some(Money::new(
        if negative {
            minor.checked_neg()?
        } else {
            minor
        },
        currency,
    ))
}

/// A real calendar date, `YYYY-MM-DD`.
pub(crate) fn iso(y: i64, m: i64, d: i64) -> Option<String> {
    let s = format!("{y:04}-{m:02}-{d:02}");
    skyla_rules::date::parse(&s).map(|_| s)
}

/// `YYMMDD` with a pivot: years below 70 are 20xx.
pub(crate) fn yymmdd(text: &str) -> Option<String> {
    let (y, m, d) = (
        num(text.get(0..2)?)?,
        num(text.get(2..4)?)?,
        num(text.get(4..6)?)?,
    );
    iso(if y < 70 { 2000 + y } else { 1900 + y }, m, d)
}

/// `DDMMYY` with the same pivot.
pub(crate) fn ddmmyy(text: &str) -> Option<String> {
    let (d, m, y) = (
        num(text.get(0..2)?)?,
        num(text.get(2..4)?)?,
        num(text.get(4..6)?)?,
    );
    iso(if y < 70 { 2000 + y } else { 1900 + y }, m, d)
}

/// `D.M.YYYY` (with optional spaces), `YYYY-MM-DD` or `DD/MM/YYYY`.
pub(crate) fn any_date(text: &str) -> Option<String> {
    let t = text.trim();
    if t.contains('-') {
        // ISO, possibly with a time: `2026-10-07T12:00:00+02:00`.
        let t = t.split(['T', ' ']).next().unwrap_or(t);
        let (y, rest) = t.split_once('-')?;
        let (m, d) = rest.split_once('-')?;
        return iso(num(y)?, num(m)?, num(d)?);
    }
    // `7. 10. 2026`, `07.10.2026`, `07/10/2026`, possibly followed by a time.
    let t = t.split("  ").next().unwrap_or(t);
    let parts: Vec<&str> = t
        .split(['.', '/'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    match parts.as_slice() {
        [d, m, y] if y.len() == 4 => iso(num(y)?, num(m)?, num(d)?),
        _ => None,
    }
}

fn num(text: &str) -> Option<i64> {
    if text.is_empty() || text.len() > 4 || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Decodes a statement's bytes: UTF-8 (with or without a BOM) when valid,
/// otherwise Windows-1250, the Czech banks' legacy encoding.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1250
            .decode_without_bom_handling(bytes)
            .0
            .into_owned(),
    }
}

/// Digits only, leading zeros dropped; `None` when nothing is left.
pub(crate) fn symbol(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let trimmed = digits.trim_start_matches('0');
    (!trimmed.is_empty() && digits.len() <= 10).then(|| trimmed.to_owned())
}

/// Finds `VS`, `KS` and `SS` in free text: `VS:2026102`, `/VS/2026102`,
/// `VS 2026102`, `VS2026102`.
pub(crate) fn symbols_in(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    let upper = text.to_uppercase();
    let find = |tag: &str| -> Option<String> {
        let mut rest = upper.as_str();
        while let Some(i) = rest.find(tag) {
            let before_ok = rest
                .get(..i)
                .and_then(|b| b.chars().next_back())
                .is_none_or(|c| !c.is_ascii_alphanumeric());
            let after = rest.get(i + tag.len()..).unwrap_or("");
            let after = after.trim_start_matches([':', '/', ' ', '.', '=', '-']);
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            if before_ok && !digits.is_empty() {
                return symbol(&digits);
            }
            rest = rest.get(i + tag.len()..).unwrap_or("");
        }
        None
    };
    (find("VS"), find("KS"), find("SS"))
}

/// Trims and turns empty text into `None`.
pub(crate) fn non_empty(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amounts_exactly() {
        let czk = Currency::CZK;
        assert_eq!(
            amount("1234.50", '.', czk).map(|m| m.minor()),
            Some(123_450)
        );
        assert_eq!(
            amount("1 234,5", ',', czk).map(|m| m.minor()),
            Some(123_450)
        );
        assert_eq!(amount("-0,01", ',', czk).map(|m| m.minor()), Some(-1));
        assert_eq!(amount("12", ',', czk).map(|m| m.minor()), Some(1_200));
        assert_eq!(amount("1,234", ',', czk), None, "three decimals");
        assert_eq!(
            amount("84,700.00", '.', czk).map(|m| m.minor()),
            Some(8_470_000)
        );
        assert_eq!(
            amount("1.234,50", ',', czk).map(|m| m.minor()),
            Some(123_450)
        );
        assert_eq!(
            amount("12.5", ',', czk),
            None,
            "a stray dot isn't a grouping mark"
        );
        assert_eq!(amount("1 23,00", ',', czk), None, "groups are three digits");
        assert_eq!(amount("12a", '.', czk), None);
        assert_eq!(amount("", '.', czk), None);
        assert_eq!(amount("9999999999999999999", '.', czk), None, "overflow");
    }

    #[test]
    fn reads_dates_and_symbols() {
        assert_eq!(yymmdd("260931"), None, "no 31 September");
        assert_eq!(yymmdd("260930").as_deref(), Some("2026-09-30"));
        assert_eq!(ddmmyy("010126").as_deref(), Some("2026-01-01"));
        assert_eq!(any_date("7. 10. 2026").as_deref(), Some("2026-10-07"));
        assert_eq!(
            any_date("2026-10-07T12:00:00+02:00").as_deref(),
            Some("2026-10-07")
        );
        assert_eq!(
            symbols_in("Platba VS:2026102 KS 0308 /SS/77"),
            (
                Some("2026102".into()),
                Some("308".into()),
                Some("77".into())
            )
        );
        assert_eq!(
            symbols_in("CLASS 12 VSX"),
            (None, None, None),
            "SS inside a word"
        );
    }

    #[test]
    fn decodes_windows_1250() {
        // "Platba nájemné" with á = 0xE1, é = 0xE9 in Windows-1250.
        let bytes = b"Platba n\xe1jemn\xe9";
        assert_eq!(decode(bytes), "Platba nájemné");
        assert_eq!(decode("Žluťoučký".as_bytes()), "Žluťoučký");
    }
}
