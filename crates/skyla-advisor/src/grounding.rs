//! The numeric-grounding validator (WP-27): every figure in an advisor's
//! prose must be one the engine produced. A model that does arithmetic of
//! its own, rounds differently, or misremembers a rate is caught here, and
//! the run is rejected rather than shown.
//!
//! Figures are read the Czech way (`942 600 Kč`, `1 571 000,00`, `29,2 %`).
//! Dates, years, section and row references (`§ 7`, `ř. 40`) and small
//! counts are not figures. Everything else must match an allowed value:
//! an amount in minor units, or a percentage.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// The values a run may quote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowed {
    /// Amounts in minor units (absolute values).
    amounts: BTreeSet<i64>,
    /// Percentages, in hundredths of a percent (`29,2 %` → 2920).
    percents: BTreeSet<i64>,
}

impl Allowed {
    /// Nothing allowed yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows an amount (sign ignored: "203 342 Kč less" quotes a difference).
    pub fn amount(&mut self, minor: i64) {
        self.amounts.insert(minor.abs());
    }

    /// Allows a percentage given as text, e.g. `29.2` or `60`.
    pub fn percent(&mut self, text: &str) {
        if let Some(h) = hundredths(&text.replace('.', ",")) {
            self.percents.insert(h);
        }
    }

    /// How many values are allowed.
    pub fn len(&self) -> usize {
        self.amounts.len() + self.percents.len()
    }

    /// Nothing allowed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A figure in the prose that no engine value backs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ungrounded {
    /// As written.
    pub text: String,
    /// A few words around it.
    pub context: String,
}

/// The check's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grounding {
    /// Figures that matched an engine value.
    pub grounded: usize,
    /// Figures that didn't.
    pub ungrounded: Vec<Ungrounded>,
}

impl Grounding {
    /// Every figure is backed.
    pub fn passes(&self) -> bool {
        self.ungrounded.is_empty()
    }
}

#[allow(clippy::expect_used)] // Constant patterns, exercised by the tests.
static IGNORED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        // ISO dates and day-month(-year) dates, in digits or with Czech or English month names.
        r"\d{4}-\d{2}-\d{2}",
        r"|\d{1,2}\.\s?\d{1,2}\.\s?(?:\d{4})?",
        r"|\d{1,2}\.?\s(?:ledna|února|března|dubna|května|června|července|srpna|září|října|listopadu|prosince|January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\b(?:\s\d{4})?",
        // Section, paragraph, row and letter references.
        r"|§\s?\d+[a-z]*(?:\s(?:odst\.|písm\.)\s?\d*[a-z]?\)?)*",
        r"|(?:odst\.|ř\.|řádek|row|Q)\s?\d+",
        // Account numbers in the chart.
        r"|(?i:accounts?|účet|účtu|účtem|účty)\s\d{3,6}(?:\s?(?:and|a|,)\s?\d{3,6})*",
        // Years.
        r"|\b(?:19|20)\d{2}\b",
    ))
    .expect("ignored pattern")
});

#[allow(clippy::expect_used)]
static FIGURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?P<num>-?\d{1,3}(?:[ \u{a0}\u{202f}]\d{3})+(?:,\d+)?|-?\d+(?:,\d+)?)(?P<unit>\s?(?:%|Kč|CZK|haléř\w*|tis\.|thousand))?",
    )
    .expect("figure pattern")
});

/// `1 571 000,50` → hundredths (157 100 050); `None` for more than two decimals.
fn hundredths(text: &str) -> Option<i64> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '\u{202f}')
        .collect();
    let negative = clean.starts_with('-');
    let body = clean.trim_start_matches('-');
    let (whole, frac) = body.split_once(',').unwrap_or((body, ""));
    if frac.len() > 2 || whole.is_empty() {
        return None;
    }
    let w: i64 = whole.parse().ok()?;
    let f: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<2}").parse().ok()?
    };
    let v = w.checked_mul(100)?.checked_add(f)?;
    Some(if negative { -v } else { v })
}

/// Checks every figure in `text` against `allowed`. Bare whole numbers below
/// `small` (counts like "two scenarios" written as 2) aren't figures.
pub fn check(text: &str, allowed: &Allowed, small: i64) -> Grounding {
    // Blank by bytes, not characters: the offsets found below index `text`,
    // so a multibyte character (§, ř, ů) must not shift the ones after it.
    let blanked = IGNORED.replace_all(text, |c: &regex::Captures<'_>| " ".repeat(c[0].len()));
    let mut grounded = 0;
    let mut ungrounded = Vec::new();
    for c in FIGURE.captures_iter(&blanked) {
        let Some(num) = c.name("num") else { continue };
        let unit = c.name("unit").map(|u| u.as_str().trim()).unwrap_or("");
        let raw = num.as_str();
        let Some(h) = hundredths(raw) else {
            ungrounded.push(around(text, num.start(), num.end()));
            continue;
        };
        let has_decimals = raw.contains(',');
        if unit.is_empty() && !has_decimals && h.abs() < small * 100 {
            continue;
        }
        let ok = match unit {
            "%" => allowed.percents.contains(&h.abs()),
            "tis." | "thousand" => allowed.amounts.contains(&(h.abs() * 1000)),
            u if u.starts_with("hal") => allowed.amounts.contains(&(h.abs() / 100)),
            // Kč, CZK, or a bare figure: crowns.
            _ => allowed.amounts.contains(&h.abs()),
        };
        if ok {
            grounded += 1;
        } else {
            ungrounded.push(around(
                text,
                num.start(),
                c.get(0).map_or(num.end(), |m| m.end()),
            ));
        }
    }
    Grounding {
        grounded,
        ungrounded,
    }
}

fn around(text: &str, start: usize, end: usize) -> Ungrounded {
    let from = text[..start]
        .char_indices()
        .rev()
        .nth(24)
        .map_or(0, |(i, _)| i);
    let to = text[end..]
        .char_indices()
        .nth(24)
        .map_or(text.len(), |(i, _)| end + i);
    Ungrounded {
        text: text[start..end].to_owned(),
        context: text[from..to].trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> Allowed {
        let mut a = Allowed::new();
        for crowns in [
            1_571_000, 383_200, 942_600, 197_584, 400_926, 203_342, 30_840,
        ] {
            a.amount(crowns * 100);
        }
        a.percent("60");
        a.percent("29.2");
        a.percent("15");
        a
    }

    #[test]
    fn grounded_prose_passes() {
        let text = "Na rok 2026 (do 31. prosince) vychází paušál 60 % na 942 600 Kč proti skutečným 383 200 Kč. \
                    Daň a pojistné klesnou z 400 926 Kč na 197 584 Kč, tedy o 203 342 Kč; sazba 15 % a sleva 30 840 Kč \
                    podle § 35ba odst. 1 písm. a), ř. 40 a 2026-12-31. Sociální pojistné 29,2 %. Two scenarios, 2 levers. \
                    Account 518 and účet 602.";
        let g = check(text, &allowed(), 13);
        assert!(g.passes(), "{:?}", g.ungrounded);
        assert_eq!(g.grounded, 9);
    }

    #[test]
    fn a_wrong_or_invented_figure_is_caught() {
        let a = allowed();
        let g = check("Ušetříte 203 343 Kč.", &a, 13);
        assert_eq!(g.ungrounded[0].text, "203 343 Kč");
        assert!(!check("Sazba je 23 %.", &a, 13).passes());
        assert!(!check("Paušál dá 942 600,50 Kč.", &a, 13).passes());
        assert!(
            !check("About 942,600 CZK.", &a, 13).passes(),
            "English grouping isn't read as Czech"
        );
        assert!(
            !check("That is 14 % less.", &a, 13).passes(),
            "percentages always count"
        );
    }

    #[test]
    fn a_wrong_figure_after_a_czech_reference_is_reported_not_a_panic() {
        // "§ 35ba odst. 1 písm. a)" is ignored and has multibyte characters;
        // the offsets of the figure after it must still index the real text.
        let text =
            "Podle § 35ba odst. 1 písm. a) činí sleva 30 841 Kč, což je výhodné pro živnostníka.";
        let g = check(text, &allowed(), 13);
        assert_eq!(g.ungrounded.len(), 1, "{:?}", g.ungrounded);
        assert_eq!(g.ungrounded[0].text, "30 841 Kč");
        assert!(g.ungrounded[0].context.contains("sleva 30 841 Kč"));
    }
}
