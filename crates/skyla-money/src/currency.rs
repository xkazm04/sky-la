use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::MoneyError;

/// An ISO 4217 currency with its number of minor-unit digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Currency {
    code: [u8; 3],
    minor_units: u8,
}

/// Supported currencies: code and minor-unit digits (ISO 4217).
const TABLE: &[(&str, u8)] = &[
    ("AUD", 2),
    ("BGN", 2),
    ("BHD", 3),
    ("BRL", 2),
    ("CAD", 2),
    ("CHF", 2),
    ("CNY", 2),
    ("CZK", 2),
    ("DKK", 2),
    ("EUR", 2),
    ("GBP", 2),
    ("HKD", 2),
    ("HUF", 2),
    ("ILS", 2),
    ("INR", 2),
    ("ISK", 0),
    ("JPY", 0),
    ("KRW", 0),
    ("KWD", 3),
    ("MXN", 2),
    ("NOK", 2),
    ("NZD", 2),
    ("OMR", 3),
    ("PLN", 2),
    ("RON", 2),
    ("RSD", 2),
    ("SEK", 2),
    ("SGD", 2),
    ("THB", 2),
    ("TRY", 2),
    ("UAH", 2),
    ("USD", 2),
    ("ZAR", 2),
];

impl Currency {
    /// Czech koruna, the functional currency of Czech entities.
    pub const CZK: Self = Self::known(*b"CZK", 2);
    /// Euro.
    pub const EUR: Self = Self::known(*b"EUR", 2);
    /// US dollar.
    pub const USD: Self = Self::known(*b"USD", 2);
    /// Pound sterling.
    pub const GBP: Self = Self::known(*b"GBP", 2);

    const fn known(code: [u8; 3], minor_units: u8) -> Self {
        Self { code, minor_units }
    }

    /// Looks up a currency by its ISO 4217 alphabetic code (case-sensitive, e.g. `"CZK"`).
    pub fn from_code(code: &str) -> Result<Self, MoneyError> {
        TABLE
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(c, mu)| {
                let b = c.as_bytes();
                Self::known([b[0], b[1], b[2]], *mu)
            })
            .ok_or_else(|| MoneyError::UnknownCurrency(code.to_owned()))
    }

    /// The three-letter code.
    pub fn code(&self) -> &str {
        // The table only holds ASCII codes, so this never fails.
        std::str::from_utf8(&self.code).unwrap_or("???")
    }

    /// Number of minor-unit digits (2 for CZK, 0 for JPY, 3 for KWD).
    pub const fn minor_units(&self) -> u8 {
        self.minor_units
    }

    /// `10^minor_units`: how many minor units make one major unit.
    pub const fn minor_per_major(&self) -> i64 {
        10_i64.pow(self.minor_units as u32)
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({})", self.code())
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::from_code(&code).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_codes_with_their_minor_units() {
        assert_eq!(Currency::from_code("CZK").unwrap(), Currency::CZK);
        assert_eq!(Currency::from_code("JPY").unwrap().minor_units(), 0);
        assert_eq!(Currency::from_code("KWD").unwrap().minor_per_major(), 1000);
        assert!(matches!(
            Currency::from_code("czk"),
            Err(MoneyError::UnknownCurrency(_))
        ));
        assert!(matches!(
            Currency::from_code("XXX"),
            Err(MoneyError::UnknownCurrency(_))
        ));
    }

    #[test]
    fn table_codes_are_unique_and_uppercase_ascii() {
        let mut codes: Vec<&str> = TABLE.iter().map(|(c, _)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), TABLE.len());
        assert!(
            codes
                .iter()
                .all(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()))
        );
    }

    #[test]
    fn serialises_as_the_code() {
        assert_eq!(serde_json::to_string(&Currency::EUR).unwrap(), "\"EUR\"");
        assert_eq!(
            serde_json::from_str::<Currency>("\"CZK\"").unwrap(),
            Currency::CZK
        );
        assert!(serde_json::from_str::<Currency>("\"ABC\"").is_err());
    }
}
