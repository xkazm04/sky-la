use std::fmt;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::{Currency, MoneyError, Rate, RoundingMode};

/// An amount as a whole number of the currency's minor units.
///
/// `Money::new(8_470_000, Currency::CZK)` is 84 700,00 Kč. Positive and negative
/// amounts are both valid; the ledger decides what a sign means.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

impl Money {
    /// An amount of `minor` minor units.
    pub const fn new(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    /// Zero in `currency`.
    pub const fn zero(currency: Currency) -> Self {
        Self::new(0, currency)
    }

    /// The amount in minor units.
    pub const fn minor(&self) -> i64 {
        self.minor
    }

    /// The currency.
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// True when the amount is exactly zero.
    pub const fn is_zero(&self) -> bool {
        self.minor == 0
    }

    /// True when the amount is below zero.
    pub const fn is_negative(&self) -> bool {
        self.minor < 0
    }

    fn same_currency(self, other: Self) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            })
        }
    }

    /// `self + other`. Fails on a currency mismatch or overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::new(minor, self.currency))
    }

    /// `self - other`. Fails on a currency mismatch or overflow.
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        let minor = self
            .minor
            .checked_sub(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::new(minor, self.currency))
    }

    /// `-self`. Fails only for the most negative representable amount.
    pub fn checked_neg(self) -> Result<Self, MoneyError> {
        let minor = self.minor.checked_neg().ok_or(MoneyError::Overflow)?;
        Ok(Self::new(minor, self.currency))
    }

    /// `self × n`, for whole quantities (e.g. 12 hours × hourly rate).
    pub fn checked_mul_int(self, n: i64) -> Result<Self, MoneyError> {
        let minor = self.minor.checked_mul(n).ok_or(MoneyError::Overflow)?;
        Ok(Self::new(minor, self.currency))
    }

    /// `self × rate`, rounded to whole minor units with `mode`.
    /// Use for percentages and fractional quantities in the same currency.
    pub fn mul_rate(self, rate: Rate, mode: RoundingMode) -> Result<Self, MoneyError> {
        let product = Decimal::from(self.minor)
            .checked_mul(rate)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::new(to_minor(mode.round(product))?, self.currency))
    }

    /// Converts into `to` at `rate` units of `to` per one major unit of `self`'s
    /// currency (the ČNB quotes CZK per EUR, so EUR → CZK uses e.g. `25.140`).
    pub fn convert(self, rate: Rate, to: Currency, mode: RoundingMode) -> Result<Self, MoneyError> {
        if rate.is_sign_negative() || rate.is_zero() {
            return Err(MoneyError::InvalidRate("an exchange rate must be positive"));
        }
        let value = Decimal::from(self.minor)
            .checked_mul(rate)
            .and_then(|v| v.checked_mul(Decimal::from(to.minor_per_major())))
            .and_then(|v| v.checked_div(Decimal::from(self.currency.minor_per_major())))
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::new(to_minor(mode.round(value))?, to))
    }

    /// Sums amounts that must all be in `currency`. An empty input sums to zero.
    pub fn sum<I: IntoIterator<Item = Self>>(
        currency: Currency,
        items: I,
    ) -> Result<Self, MoneyError> {
        items
            .into_iter()
            .try_fold(Self::zero(currency), Self::checked_add)
    }
}

/// Converts an already-rounded decimal to `i64` minor units.
fn to_minor(value: Decimal) -> Result<i64, MoneyError> {
    value.to_i64().ok_or(MoneyError::Overflow)
}

/// Locale-neutral form for logs and errors: `-84700.00 CZK`.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mu = u32::from(self.currency.minor_units());
        let sign = if self.minor < 0 { "-" } else { "" };
        let abs = self.minor.unsigned_abs();
        if mu == 0 {
            return write!(f, "{sign}{abs} {}", self.currency);
        }
        let per = 10_u64.pow(mu);
        write!(
            f,
            "{sign}{}.{:0width$} {}",
            abs / per,
            abs % per,
            self.currency,
            width = mu as usize
        )
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn czk(minor: i64) -> Money {
        Money::new(minor, Currency::CZK)
    }

    #[test]
    fn adds_and_rejects_mixed_currencies() {
        assert_eq!(czk(150).checked_add(czk(-50)).unwrap(), czk(100));
        assert_eq!(
            czk(1).checked_add(Money::new(1, Currency::EUR)),
            Err(MoneyError::CurrencyMismatch {
                left: Currency::CZK,
                right: Currency::EUR
            })
        );
    }

    #[test]
    fn overflow_is_an_error_not_a_panic() {
        assert_eq!(czk(i64::MAX).checked_add(czk(1)), Err(MoneyError::Overflow));
        assert_eq!(czk(i64::MIN).checked_neg(), Err(MoneyError::Overflow));
        assert_eq!(czk(i64::MAX).checked_mul_int(2), Err(MoneyError::Overflow));
        assert_eq!(
            czk(i64::MAX).mul_rate(Rate::new(2, 0), RoundingMode::HalfEven),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn converts_eur_to_czk_at_the_cnb_rate() {
        // 490,00 € at 25,140 Kč/€ = 12 318,60 Kč (the AWS line on the design canvas).
        let eur = Money::new(49_000, Currency::EUR);
        let czk_amount = eur
            .convert(Rate::new(25_140, 3), Currency::CZK, RoundingMode::HalfUp)
            .unwrap();
        assert_eq!(czk_amount, czk(1_231_860));
        // 45,86 € at 25,140 = 1 152,9204 → 1 152,92 Kč.
        let google = Money::new(4_586, Currency::EUR);
        assert_eq!(
            google
                .convert(Rate::new(25_140, 3), Currency::CZK, RoundingMode::HalfUp)
                .unwrap(),
            czk(115_292)
        );
    }

    #[test]
    fn converts_between_different_minor_units() {
        // 1 000 JPY (0 minor digits) at 0,16 CZK per JPY = 160,00 CZK.
        let jpy = Money::new(1_000, Currency::from_code("JPY").unwrap());
        assert_eq!(
            jpy.convert(Rate::new(16, 2), Currency::CZK, RoundingMode::HalfEven)
                .unwrap(),
            czk(16_000)
        );
        assert!(matches!(
            jpy.convert(Rate::ZERO, Currency::CZK, RoundingMode::HalfEven),
            Err(MoneyError::InvalidRate(_))
        ));
    }

    #[test]
    fn displays_locale_neutrally() {
        assert_eq!(czk(-8_470_000).to_string(), "-84700.00 CZK");
        assert_eq!(czk(5).to_string(), "0.05 CZK");
        assert_eq!(
            Money::new(1_234, Currency::from_code("JPY").unwrap()).to_string(),
            "1234 JPY"
        );
        assert_eq!(czk(i64::MIN).to_string(), "-92233720368547758.08 CZK");
    }

    #[test]
    fn serialises_as_minor_units_and_code() {
        let json = serde_json::to_string(&czk(8_470_000)).unwrap();
        assert_eq!(json, r#"{"minor":8470000,"currency":"CZK"}"#);
        assert_eq!(
            serde_json::from_str::<Money>(&json).unwrap(),
            czk(8_470_000)
        );
    }

    #[test]
    fn sums_and_propagates_mismatches() {
        assert_eq!(
            Money::sum(Currency::CZK, [czk(1), czk(2), czk(3)]).unwrap(),
            czk(6)
        );
        assert_eq!(Money::sum(Currency::CZK, []).unwrap(), czk(0));
        assert!(Money::sum(Currency::CZK, [Money::new(1, Currency::EUR)]).is_err());
    }

    proptest! {
        #[test]
        fn add_then_sub_round_trips(a in -1_000_000_000_000_i64..1_000_000_000_000, b in -1_000_000_000_000_i64..1_000_000_000_000) {
            prop_assert_eq!(czk(a).checked_add(czk(b)).unwrap().checked_sub(czk(b)).unwrap(), czk(a));
        }

        #[test]
        fn checked_ops_never_panic(a: i64, b: i64, n: i64) {
            let _ = czk(a).checked_add(czk(b));
            let _ = czk(a).checked_sub(czk(b));
            let _ = czk(a).checked_neg();
            let _ = czk(a).checked_mul_int(n);
            let _ = czk(a).mul_rate(Rate::new(b, 4), RoundingMode::HalfEven);
        }
    }
}
