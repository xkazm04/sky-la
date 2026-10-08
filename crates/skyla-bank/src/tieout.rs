//! Statement tie-out: the opening balance plus every line must equal the
//! closing balance, and each statement must start where the books (or the
//! previous statement) ended. A missing or doubled line fails loudly.

use serde::{Deserialize, Serialize};
use skyla_money::Money;

use crate::model::Statement;

/// A statement that ties out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TieOut {
    /// Opening balance.
    pub opening: Money,
    /// Money in.
    pub credits: Money,
    /// Money out (negative).
    pub debits: Money,
    /// Closing balance, equal to opening + credits + debits.
    pub closing: Money,
    /// Lines counted.
    pub lines: usize,
}

/// Why a statement doesn't tie out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TieOutError {
    /// The lines don't add up to the closing balance.
    #[error(
        "the statement doesn't tie out: opening {} + {} lines = {}, but the closing balance is {} (off by {}). A line is missing, doubled or misread; nothing was imported.",
        .opening.format_cs(), .lines, .computed.format_cs(), .closing.format_cs(), .difference.format_cs()
    )]
    Unbalanced {
        /// Opening.
        opening: Money,
        /// Lines summed.
        lines: usize,
        /// Opening + lines.
        computed: Money,
        /// What the bank says.
        closing: Money,
        /// `closing − computed`.
        difference: Money,
    },
    /// The statement doesn't start where the books end.
    #[error(
        "the statement opens at {} but the books show {} for the account on the day before; a statement in between is missing.",
        .opening.format_cs(), .expected.format_cs()
    )]
    Gap {
        /// The statement's opening.
        opening: Money,
        /// The books' balance before it.
        expected: Money,
    },
    /// The file has no balances to check against.
    #[error("the statement gives no opening or closing balance, so it can't be tied out")]
    NoBalances,
    /// Amounts in different currencies.
    #[error("lines in {line} on a {account} statement")]
    Currency {
        /// The statement's currency.
        account: String,
        /// The line's.
        line: String,
    },
}

/// Checks a statement against its own balances, and its opening against
/// `books` (the account's balance the day before it starts) when given.
pub fn tie_out(statement: &Statement, books: Option<Money>) -> Result<TieOut, TieOutError> {
    let (Some(opening), Some(closing)) = (statement.opening, statement.closing) else {
        return Err(TieOutError::NoBalances);
    };
    let currency = opening.currency();
    let zero = Money::zero(currency);
    let (mut credits, mut debits) = (zero, zero);
    for l in &statement.lines {
        if l.amount.currency() != currency {
            return Err(TieOutError::Currency {
                account: currency.code().to_owned(),
                line: l.amount.currency().code().to_owned(),
            });
        }
        // Sums of a statement stay far inside i64; an overflow would be a
        // corrupt file, reported as unbalanced.
        if l.amount.minor() >= 0 {
            credits = credits.checked_add(l.amount).unwrap_or(credits);
        } else {
            debits = debits.checked_add(l.amount).unwrap_or(debits);
        }
    }
    let computed = opening
        .checked_add(credits)
        .and_then(|m| m.checked_add(debits))
        .unwrap_or(opening);
    if computed != closing {
        return Err(TieOutError::Unbalanced {
            opening,
            lines: statement.lines.len(),
            computed,
            closing,
            difference: closing.checked_sub(computed).unwrap_or(zero),
        });
    }
    if let Some(expected) = books
        && expected != opening
    {
        return Err(TieOutError::Gap { opening, expected });
    }
    Ok(TieOut {
        opening,
        credits,
        debits,
        closing,
        lines: statement.lines.len(),
    })
}
