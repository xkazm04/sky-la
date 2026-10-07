//! Czech (`cs-CZ`) number formatting and strict parsing of amounts.
//!
//! Format: digit groups separated by a no-break space, comma as the decimal
//! separator, `-` for negatives: `-84 700,00`. Parsing accepts exactly that
//! shape (plus ordinary or narrow no-break spaces as group separators and `−`
//! U+2212 as the minus sign). It rejects anything ambiguous instead of
//! guessing; in particular `1.234` is refused because it could mean either a
//! thousand or one.

use crate::{Currency, Money, MoneyError};

const NBSP: char = '\u{00A0}';

/// Formats `minor` units of `currency` as a Czech number, without a symbol.
pub fn format_amount_cs(minor: i64, currency: Currency) -> String {
    let mu = u32::from(currency.minor_units());
    let abs = minor.unsigned_abs();
    let per = 10_u64.pow(mu);
    let digits = (abs / per).to_string();

    let mut out = String::with_capacity(digits.len() + 8);
    if minor < 0 {
        out.push('-');
    }
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(NBSP);
        }
        out.push(ch);
    }
    if mu > 0 {
        out.push(',');
        out.push_str(&format!("{:0width$}", abs % per, width = mu as usize));
    }
    out
}

impl Money {
    /// Czech display with a symbol: `84 700,00 Kč`, `490,00 €`, `12,00 USD`.
    pub fn format_cs(&self) -> String {
        let currency = self.currency();
        let symbol = match currency.code() {
            "CZK" => "Kč",
            "EUR" => "€",
            other => other,
        };
        format!(
            "{}{NBSP}{symbol}",
            format_amount_cs(self.minor(), self.currency())
        )
    }
}

fn invalid(input: &str, reason: &'static str) -> MoneyError {
    MoneyError::InvalidAmount {
        input: input.to_owned(),
        reason,
    }
}

/// Parses a Czech-formatted amount (`84 700,00`, `-6 000`, `1234,5`) into `currency`.
pub fn parse_amount_cs(input: &str, currency: Currency) -> Result<Money, MoneyError> {
    let text = input.trim();
    let (negative, rest) = match text.chars().next() {
        Some('-' | '−') => (
            true,
            &text[text.char_indices().nth(1).map_or(text.len(), |(i, _)| i)..],
        ),
        Some('+') => (false, &text[1..]),
        _ => (false, text),
    };
    if rest.contains('.') {
        return Err(invalid(input, "use a comma as the decimal separator"));
    }
    let (int_text, frac_text) = match rest.split_once(',') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };

    let groups: Vec<&str> = int_text.split([' ', NBSP, '\u{202F}']).collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid(
            input,
            "expected digits, optionally grouped by spaces",
        ));
    }
    if groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)) {
        return Err(invalid(input, "digit groups must have three digits"));
    }

    let mu = usize::from(currency.minor_units());
    let fraction = match frac_text {
        None => String::new(),
        Some(_) if mu == 0 => return Err(invalid(input, "this currency has no minor units")),
        Some(f) if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) => {
            return Err(invalid(input, "expected digits after the decimal comma"));
        }
        Some(f) if f.len() > mu => return Err(invalid(input, "too many decimal places")),
        Some(f) => f.to_owned(),
    };

    let digits = format!("{}{fraction:0<mu$}", groups.concat());
    let magnitude: i128 = digits.parse().map_err(|_| MoneyError::Overflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    let minor = i64::try_from(signed).map_err(|_| MoneyError::Overflow)?;
    Ok(Money::new(minor, currency))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn jpy() -> Currency {
        Currency::from_code("JPY").unwrap()
    }

    #[test]
    fn formats_like_the_czech_locale() {
        assert_eq!(format_amount_cs(8_470_000, Currency::CZK), "84\u{a0}700,00");
        assert_eq!(format_amount_cs(-600_000, Currency::CZK), "-6\u{a0}000,00");
        assert_eq!(format_amount_cs(5, Currency::CZK), "0,05");
        assert_eq!(format_amount_cs(1_234_567, jpy()), "1\u{a0}234\u{a0}567");
        assert_eq!(
            Money::new(8_470_000, Currency::CZK).format_cs(),
            "84\u{a0}700,00\u{a0}Kč"
        );
        assert_eq!(
            Money::new(49_000, Currency::EUR).format_cs(),
            "490,00\u{a0}€"
        );
    }

    #[test]
    fn parses_the_shapes_people_type() {
        let p = |s: &str| parse_amount_cs(s, Currency::CZK).map(|m| m.minor());
        assert_eq!(p("84 700,00"), Ok(8_470_000));
        assert_eq!(p("84\u{a0}700"), Ok(8_470_000));
        assert_eq!(p("−6 000,5"), Ok(-600_050));
        assert_eq!(p("+12,3"), Ok(1_230));
        assert_eq!(p("1234567,89"), Ok(123_456_789));
        assert_eq!(p("  42 "), Ok(4_200));
    }

    #[test]
    fn rejects_ambiguous_or_malformed_input() {
        let err = |s: &str| {
            matches!(
                parse_amount_cs(s, Currency::CZK),
                Err(MoneyError::InvalidAmount { .. })
            )
        };
        for bad in [
            "1.234",
            "12,345",
            "12 34",
            "1234 567",
            "",
            "-",
            ",5",
            "12,",
            "1e5",
            "12 Kč",
            "--1",
            "1 000 ,00",
        ] {
            assert!(err(bad), "should reject {bad:?}");
        }
        assert!(matches!(
            parse_amount_cs("1,5", jpy()),
            Err(MoneyError::InvalidAmount { .. })
        ));
        assert_eq!(
            parse_amount_cs("99999999999999999999", Currency::CZK),
            Err(MoneyError::Overflow)
        );
    }

    proptest! {
        #[test]
        fn format_then_parse_round_trips(minor: i64, which in 0_usize..3) {
            let currency = [Currency::CZK, jpy(), Currency::from_code("KWD").unwrap()][which];
            let text = format_amount_cs(minor, currency);
            prop_assert_eq!(parse_amount_cs(&text, currency).unwrap(), Money::new(minor, currency));
        }

        #[test]
        fn parsing_never_panics(input in "\\PC{0,24}") {
            let _ = parse_amount_cs(&input, Currency::CZK);
        }
    }
}
