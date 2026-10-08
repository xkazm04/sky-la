//! User rules: visible, editable conditions on a line and what to do with it.

use serde::{Deserialize, Serialize};

use crate::normalise::{Normalised, canonical_account, canonical_name};

/// Money in or out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Money in.
    In,
    /// Money out.
    Out,
}

/// One condition; a rule needs all of its conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Condition {
    /// The other party's account (any spelling).
    CounterpartyAccount(String),
    /// The other party's name contains this (diacritics and case ignored).
    NameContains(String),
    /// The message contains this (diacritics and case ignored).
    MessageContains(String),
    /// Variable symbol.
    Vs(String),
    /// Constant symbol.
    Ks(String),
    /// In or out.
    Direction(Direction),
    /// The absolute amount in minor units, inclusive.
    AmountBetween {
        /// Lower bound.
        min: i64,
        /// Upper bound.
        max: i64,
    },
}

/// What a rule proposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Action {
    /// Book the line against an account (an expense, a fee), with an
    /// optional VAT code.
    Book {
        /// The account.
        account: String,
        /// VAT code from the pack, when the line carries VAT.
        vat_code: Option<String>,
        /// The entry's memo.
        memo: String,
    },
    /// Settle this customer's open invoices with the line.
    SettleCustomer {
        /// The customer's name as on their invoices.
        customer: String,
    },
}

/// A rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Stable id.
    pub id: u32,
    /// For people: "Bank fee", "Rent".
    pub name: String,
    /// All must hold.
    pub when: Vec<Condition>,
    /// What to propose.
    pub then: Action,
    /// Accept without asking when it applies (the user opts in per rule).
    pub auto_accept: bool,
}

impl Condition {
    fn holds(&self, l: &Normalised) -> bool {
        let fold = |s: &str| canonical_name(s);
        match self {
            Self::CounterpartyAccount(a) => canonical_account(a)
                .is_some_and(|a| l.counterparty_account.as_deref() == Some(a.as_str())),
            Self::NameContains(t) => !t.trim().is_empty() && l.counterparty_name.contains(&fold(t)),
            Self::MessageContains(t) => {
                !t.trim().is_empty()
                    && fold(l.line.message.as_deref().unwrap_or("")).contains(&fold(t))
            }
            Self::Vs(v) => l.line.vs.as_deref() == Some(v.trim_start_matches('0')),
            Self::Ks(k) => l.line.ks.as_deref() == Some(k.trim_start_matches('0')),
            Self::Direction(Direction::In) => l.line.amount.minor() > 0,
            Self::Direction(Direction::Out) => l.line.amount.minor() < 0,
            Self::AmountBetween { min, max } => {
                (*min..=*max).contains(&l.line.amount.minor().saturating_abs())
            }
        }
    }
}

impl Rule {
    /// True when every condition holds (a rule with none never applies).
    pub fn applies(&self, l: &Normalised) -> bool {
        !self.when.is_empty() && self.when.iter().all(|c| c.holds(l))
    }
}

/// The first rule that applies, in order.
pub fn first_rule<'a>(rules: &'a [Rule], l: &Normalised) -> Option<&'a Rule> {
    rules.iter().find(|r| r.applies(l))
}
