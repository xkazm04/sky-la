//! What never leaves the machine: IBANs, Czech account numbers, personal
//! ID numbers (rodné číslo) and card numbers. The patterns are deliberately
//! wide: a string that merely looks like one is withheld too, because a
//! false alarm costs a word and a miss costs the user's privacy.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A kind of identifier the gate withholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Withheld {
    /// An IBAN, with or without spaces.
    Iban,
    /// A domestic account number, `prefix-number/bank`.
    BankAccount,
    /// A Czech or Slovak personal ID number.
    PersonalId,
    /// A payment card number.
    CardNumber,
}

impl Withheld {
    /// The placeholder put where it was.
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Iban => "[IBAN withheld]",
            Self::BankAccount => "[account number withheld]",
            Self::PersonalId => "[personal ID withheld]",
            Self::CardNumber => "[card number withheld]",
        }
    }

    /// For people.
    pub fn label(self) -> &'static str {
        match self {
            Self::Iban => "IBAN",
            Self::BankAccount => "Account number",
            Self::PersonalId => "Personal ID number",
            Self::CardNumber => "Card number",
        }
    }
}

// Boundaries are about digits, not words: `RČ855120/1234` or
// `ŽCZ65…` must not slip past because a letter is glued on. Numeric
// identifiers need a non-digit (or the edge) on both sides; IBANs need
// nothing, since their country code is letters anyway.
#[allow(clippy::expect_used)] // The patterns are constants, checked by the tests.
static IBAN: LazyLock<Regex> = LazyLock::new(|| {
    // Country and check digits, then the BBAN: in groups of four with
    // single spaces (the printed form), or run together in either case.
    Regex::new(r"[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]{4}){2,7}(?:[ ]?[A-Z0-9]{1,3})?|(?i:[a-z]{2}\d{2}[a-z0-9]{10,30})")
        .expect("IBAN pattern")
});
#[allow(clippy::expect_used)]
static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<pre>^|[^0-9-])(?P<v>(?:\d{1,6}-)?\d{2,10}/\d{4})(?P<post>$|[^0-9])")
        .expect("account pattern")
});
#[allow(clippy::expect_used)]
static PERSONAL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<pre>^|[^0-9])(?P<v>(?P<yy>\d{2})(?P<mm>\d{2})(?P<dd>\d{2})[ ]?/?[ ]?\d{3,4})(?P<post>$|[^0-9])")
        .expect("personal ID pattern")
});
#[allow(clippy::expect_used)]
static CARD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<pre>^|[^0-9])(?P<v>\d(?:[ -]?\d){12,30})(?P<post>$|[^0-9])")
        .expect("card pattern")
});

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                if d * 2 > 9 { d * 2 - 9 } else { d * 2 }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Any 13–19-digit window passes Luhn: a card number inside a longer run.
fn holds_a_card(digits: &[u32]) -> bool {
    (13..=19.min(digits.len())).any(|len| digits.windows(len).any(luhn))
}

/// A date-shaped first half: the month may carry +50 (women) or +20/+70
/// (numbers issued since 2004).
fn plausible_birth(month: u32, day: u32) -> bool {
    let m = match month {
        1..=12 => month,
        21..=32 => month - 20,
        51..=62 => month - 50,
        71..=82 => month - 70,
        _ => return false,
    };
    (1..=12).contains(&m) && (1..=31).contains(&day)
}

/// Replaces `v` in every match `decide` accepts, keeping the context, and
/// repeats until nothing changes (context characters can't be shared by
/// two matches in one pass). Placeholders hold no digits, so this ends.
fn bounded(
    re: &Regex,
    text: &str,
    kind: Withheld,
    found: &mut Vec<Withheld>,
    decide: impl Fn(&regex::Captures<'_>) -> bool,
) -> String {
    let mut current = text.to_owned();
    loop {
        let mut changed = false;
        let next = re
            .replace_all(&current, |c: &regex::Captures<'_>| {
                let part = |n: &str| c.name(n).map_or("", |m| m.as_str());
                if decide(c) {
                    changed = true;
                    found.push(kind);
                    format!("{}{}{}", part("pre"), kind.placeholder(), part("post"))
                } else {
                    c.get(0).map_or(String::new(), |m| m.as_str().to_owned())
                }
            })
            .into_owned();
        if !changed {
            return current;
        }
        current = next;
    }
}

/// `text` with everything withheld, and what was withheld.
pub fn redact(text: &str) -> (String, Vec<Withheld>) {
    let mut found = Vec::new();
    let out = IBAN
        .replace_all(text, |_: &regex::Captures<'_>| {
            found.push(Withheld::Iban);
            Withheld::Iban.placeholder()
        })
        .into_owned();
    let out = bounded(&PERSONAL_ID, &out, Withheld::PersonalId, &mut found, |c| {
        let num = |n: &str| {
            c.name(n)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0)
        };
        plausible_birth(num("mm"), num("dd"))
    });
    let out = bounded(&ACCOUNT, &out, Withheld::BankAccount, &mut found, |_| true);
    let out = bounded(&CARD, &out, Withheld::CardNumber, &mut found, |c| {
        let digits: Vec<u32> = c
            .name("v")
            .map_or("", |m| m.as_str())
            .chars()
            .filter_map(|ch| ch.to_digit(10))
            .collect();
        holds_a_card(&digits)
    });
    (out, found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withholds_each_kind() {
        let (t, w) = redact(
            "Pay CZ65 0800 0000 1920 0014 5399 or 19-2000145399/0800; RČ 855120/1234, card 4111 1111 1111 1111.",
        );
        assert!(
            !t.contains("0800 0000")
                && !t.contains("2000145399")
                && !t.contains("855120")
                && !t.contains("4111"),
            "{t}"
        );
        let mut w = w;
        w.sort();
        assert_eq!(
            w,
            [
                Withheld::Iban,
                Withheld::BankAccount,
                Withheld::PersonalId,
                Withheld::CardNumber
            ]
        );
    }

    #[test]
    fn leaves_ordinary_figures_alone() {
        let text =
            "Profit 280 350,00 Kč on 2026-09-30, invoice 2026-114, VS 2026114, ř. 40, 12 318,60";
        assert_eq!(redact(text), (text.to_owned(), Vec::new()));
    }
}
