use crate::Currency;

/// Everything that can go wrong with money. No operation in this crate panics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    /// The two operands are in different currencies.
    #[error("currency mismatch: {left} vs {right}")]
    CurrencyMismatch {
        /// Left operand's currency.
        left: Currency,
        /// Right operand's currency.
        right: Currency,
    },
    /// The result does not fit in an `i64` count of minor units.
    #[error("amount overflow")]
    Overflow,
    /// The ISO 4217 code isn't in the supported table.
    #[error("unknown currency code {0:?}")]
    UnknownCurrency(String),
    /// The text is not an amount in the expected format.
    #[error("invalid amount {input:?}: {reason}")]
    InvalidAmount {
        /// The rejected input.
        input: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A rate or percentage is outside its valid range.
    #[error("invalid rate: {0}")]
    InvalidRate(&'static str),
    /// Allocation needs at least one part with a non-zero weight.
    #[error("allocation weights must contain at least one non-zero weight")]
    EmptyAllocation,
}
