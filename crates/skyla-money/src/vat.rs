//! Splitting amounts into base and VAT.
//!
//! Both directions return a [`VatSplit`] where `base + vat == gross` holds
//! exactly. Rates come from the rule pack as percentages (`21`, `12`). The
//! rounding mode is the caller's, because it is a statutory choice.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Money, MoneyError, Rate, RoundingMode};

/// A base, its VAT and the gross total, all in one currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatSplit {
    /// Amount excluding VAT.
    pub base: Money,
    /// The VAT itself.
    pub vat: Money,
    /// `base + vat`.
    pub gross: Money,
}

fn check_rate(rate_percent: Rate) -> Result<(), MoneyError> {
    if rate_percent.is_sign_negative() {
        return Err(MoneyError::InvalidRate("a VAT rate cannot be negative"));
    }
    Ok(())
}

/// VAT on top of a base: `vat = round(base × rate / 100)`.
pub fn from_base(
    base: Money,
    rate_percent: Rate,
    mode: RoundingMode,
) -> Result<VatSplit, MoneyError> {
    check_rate(rate_percent)?;
    let fraction = rate_percent
        .checked_div(Decimal::ONE_HUNDRED)
        .ok_or(MoneyError::Overflow)?;
    let vat = base.mul_rate(fraction, mode)?;
    Ok(VatSplit {
        base,
        vat,
        gross: base.checked_add(vat)?,
    })
}

/// VAT contained in a gross amount: `vat = round(gross × rate / (100 + rate))`,
/// and `base = gross − vat` so the split is exact.
pub fn from_gross(
    gross: Money,
    rate_percent: Rate,
    mode: RoundingMode,
) -> Result<VatSplit, MoneyError> {
    check_rate(rate_percent)?;
    let denominator = Decimal::ONE_HUNDRED
        .checked_add(rate_percent)
        .ok_or(MoneyError::Overflow)?;
    let fraction = rate_percent
        .checked_div(denominator)
        .ok_or(MoneyError::Overflow)?;
    let vat = gross.mul_rate(fraction, mode)?;
    Ok(VatSplit {
        base: gross.checked_sub(vat)?,
        vat,
        gross,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Currency;
    use proptest::prelude::*;

    fn czk(minor: i64) -> Money {
        Money::new(minor, Currency::CZK)
    }
    const RATE_21: Rate = Rate::from_parts(21, 0, 0, false, 0);
    const RATE_12: Rate = Rate::from_parts(12, 0, 0, false, 0);

    #[test]
    fn splits_the_northwind_receipt() {
        // 84 700,00 gross at 21 % = 70 000,00 base + 14 700,00 VAT.
        let split = from_gross(czk(8_470_000), RATE_21, RoundingMode::HalfUp).unwrap();
        assert_eq!((split.base, split.vat), (czk(7_000_000), czk(1_470_000)));
    }

    #[test]
    fn computes_reverse_charge_vat_on_the_aws_line() {
        // 12 318,60 base at 21 % = 2 586,906 → 2 586,91.
        let split = from_base(czk(1_231_860), RATE_21, RoundingMode::HalfUp).unwrap();
        assert_eq!(split.vat, czk(258_691));
        assert_eq!(split.gross, czk(1_490_551));
    }

    #[test]
    fn handles_the_reduced_rate_and_zero() {
        let split = from_base(czk(10_000), RATE_12, RoundingMode::HalfEven).unwrap();
        assert_eq!(split.vat, czk(1_200));
        let zero = from_gross(czk(10_000), Rate::ZERO, RoundingMode::HalfEven).unwrap();
        assert_eq!((zero.base, zero.vat), (czk(10_000), czk(0)));
        assert!(matches!(
            from_base(czk(1), Rate::new(-1, 0), RoundingMode::HalfEven),
            Err(MoneyError::InvalidRate(_))
        ));
    }

    proptest! {
        #[test]
        fn splits_are_always_exact(minor in -10_000_000_000_000_i64..10_000_000_000_000, rate in 0_i64..30) {
            let rate = Rate::new(rate, 0);
            let g = from_gross(czk(minor), rate, RoundingMode::HalfUp).unwrap();
            prop_assert_eq!(g.base.checked_add(g.vat).unwrap(), g.gross);
            let b = from_base(czk(minor), rate, RoundingMode::HalfUp).unwrap();
            prop_assert_eq!(b.base.checked_add(b.vat).unwrap(), b.gross);
        }

        #[test]
        fn gross_to_base_and_back_is_within_one_minor_unit(minor in -10_000_000_000_000_i64..10_000_000_000_000, rate in 0_i64..30) {
            let rate = Rate::new(rate, 0);
            let split = from_gross(czk(minor), rate, RoundingMode::HalfUp).unwrap();
            let back = from_base(split.base, rate, RoundingMode::HalfUp).unwrap();
            prop_assert!((back.gross.minor() - minor).abs() <= 1);
        }
    }
}
