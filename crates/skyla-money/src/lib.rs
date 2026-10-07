//! Money in integer minor units, rates, rounding modes and exact allocation.
//!
//! Every amount is an `i64` count of the currency's minor unit (haléř for CZK,
//! cent for EUR). Arithmetic is checked and never panics. Conversions that need
//! fractions (rates, VAT, FX) use exact decimals and an explicit [`RoundingMode`].
//!
//! Money paths must never use floating point: `clippy::float_arithmetic` is denied here.

#![deny(clippy::float_arithmetic)]

mod allocate;
mod currency;
mod error;
mod format;
mod money;
mod rounding;
pub mod vat;

pub use allocate::allocate;
pub use currency::Currency;
pub use error::MoneyError;
pub use format::{format_amount_cs, parse_amount_cs};
pub use money::Money;
pub use rounding::RoundingMode;
/// Exact decimal used for rates, percentages and FX. Re-exported so callers
/// don't depend on `rust_decimal` directly.
pub use rust_decimal::Decimal as Rate;
