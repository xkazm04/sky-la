use rust_decimal::{Decimal, RoundingStrategy};

/// How a fractional amount is brought to a whole number of minor units.
/// There is deliberately no default: every call site names its rule, because
/// statutes differ (e.g. VAT on invoices vs. income tax rounded up to whole crowns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Midpoint to the even neighbour ("banker's rounding").
    HalfEven,
    /// Midpoint away from zero (commercial rounding).
    HalfUp,
    /// Truncate toward zero.
    TowardZero,
    /// Any fraction moves away from zero.
    AwayFromZero,
}

impl RoundingMode {
    pub(crate) fn strategy(self) -> RoundingStrategy {
        match self {
            Self::HalfEven => RoundingStrategy::MidpointNearestEven,
            Self::HalfUp => RoundingStrategy::MidpointAwayFromZero,
            Self::TowardZero => RoundingStrategy::ToZero,
            Self::AwayFromZero => RoundingStrategy::AwayFromZero,
        }
    }

    /// Rounds a decimal to a whole number with this mode.
    pub(crate) fn round(self, value: Decimal) -> Decimal {
        value.round_dp_with_strategy(0, self.strategy())
    }
}

#[cfg(test)]
mod tests {
    use super::RoundingMode::*;
    use rust_decimal::Decimal;

    fn r(mode: super::RoundingMode, value: &str) -> String {
        mode.round(value.parse::<Decimal>().unwrap()).to_string()
    }

    #[test]
    fn each_mode_handles_midpoints_and_signs() {
        let cases = [
            // value, HalfEven, HalfUp, TowardZero, AwayFromZero
            ("2.5", "2", "3", "2", "3"),
            ("3.5", "4", "4", "3", "4"),
            ("-2.5", "-2", "-3", "-2", "-3"),
            ("2.1", "2", "2", "2", "3"),
            ("-2.9", "-3", "-3", "-2", "-3"),
            ("7", "7", "7", "7", "7"),
        ];
        for (v, he, hu, tz, afz) in cases {
            assert_eq!(r(HalfEven, v), he, "HalfEven {v}");
            assert_eq!(r(HalfUp, v), hu, "HalfUp {v}");
            assert_eq!(r(TowardZero, v), tz, "TowardZero {v}");
            assert_eq!(r(AwayFromZero, v), afz, "AwayFromZero {v}");
        }
    }
}
