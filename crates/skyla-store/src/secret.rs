use std::fmt;

use data_encoding::{BASE32_NOPAD, HEXUPPER};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::StoreError;

/// Length of every key in bytes (256 bits).
pub(crate) const KEY_LEN: usize = 32;

/// Equality without an early exit, so comparing keys doesn't leak timing.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn random_bytes() -> Result<[u8; KEY_LEN], StoreError> {
    let mut bytes = [0_u8; KEY_LEN];
    getrandom::fill(&mut bytes).map_err(|_| StoreError::Random)?;
    Ok(bytes)
}

/// The key that encrypts the database. Wiped from memory on drop; never printed.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DataKey([u8; KEY_LEN]);

impl DataKey {
    /// A fresh random key from the operating system's generator.
    pub fn generate() -> Result<Self, StoreError> {
        random_bytes().map(Self)
    }

    /// Wraps existing key bytes (e.g. read back from the OS keychain).
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// The raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// The value for `PRAGMA key` / `PRAGMA rekey`: SQLCipher's raw-key form,
    /// which skips SQLCipher's own key derivation (ours already happened).
    pub(crate) fn sqlcipher_literal(&self) -> Zeroizing<String> {
        let mut hex = Zeroizing::new(HEXUPPER.encode(&self.0));
        let literal = Zeroizing::new(format!("\"x'{}'\"", hex.as_str()));
        hex.zeroize();
        literal
    }
}

impl PartialEq for DataKey {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&self.0, &other.0)
    }
}
impl Eq for DataKey {}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(…)")
    }
}

/// A printable second way to unwrap the [`DataKey`], shown once at setup.
///
/// Rendered as 52 base32 characters in groups of four (`ABCD-EFGH-…`).
/// Parsing ignores case, dashes and spaces, and accepts `0`, `1` and `8` for
/// the look-alikes `O`, `I` and `B`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RecoveryKey([u8; KEY_LEN]);

impl RecoveryKey {
    /// A fresh random recovery key.
    pub fn generate() -> Result<Self, StoreError> {
        random_bytes().map(Self)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// The form to show and print for the user.
    pub fn to_display(&self) -> Zeroizing<String> {
        let encoded = Zeroizing::new(BASE32_NOPAD.encode(&self.0));
        let groups: Vec<&str> = encoded
            .as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();
        Zeroizing::new(groups.join("-"))
    }

    /// Parses what the user typed back.
    pub fn parse(text: &str) -> Result<Self, StoreError> {
        let normalised: Zeroizing<String> = Zeroizing::new(
            text.chars()
                .filter(|c| !c.is_whitespace() && *c != '-')
                .map(|c| match c.to_ascii_uppercase() {
                    '0' => 'O',
                    '1' => 'I',
                    '8' => 'B',
                    other => other,
                })
                .collect(),
        );
        let decoded = Zeroizing::new(
            BASE32_NOPAD
                .decode(normalised.as_bytes())
                .map_err(|_| StoreError::InvalidRecoveryKey("unexpected characters"))?,
        );
        let bytes: [u8; KEY_LEN] = decoded
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::InvalidRecoveryKey("wrong length"))?;
        Ok(Self(bytes))
    }
}

impl PartialEq for RecoveryKey {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&self.0, &other.0)
    }
}
impl Eq for RecoveryKey {}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryKey(…)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_round_trips_through_its_display_form() {
        let key = RecoveryKey::generate().unwrap();
        let shown = key.to_display();
        assert_eq!(shown.len(), 52 + 12, "13 groups of 4 with 12 dashes");
        assert_eq!(RecoveryKey::parse(&shown).unwrap(), key);
        let sloppy = shown
            .to_lowercase()
            .replace('-', " ")
            .replace('o', "0")
            .replace('i', "1");
        assert_eq!(RecoveryKey::parse(&sloppy).unwrap(), key);
    }

    #[test]
    fn rejects_malformed_recovery_keys() {
        assert!(matches!(
            RecoveryKey::parse("ABCD-EFGH"),
            Err(StoreError::InvalidRecoveryKey(_))
        ));
        assert!(matches!(
            RecoveryKey::parse("!!!!"),
            Err(StoreError::InvalidRecoveryKey(_))
        ));
    }

    #[test]
    fn secrets_never_print() {
        let key = DataKey::generate().unwrap();
        assert_eq!(format!("{key:?}"), "DataKey(…)");
        assert_eq!(
            format!("{:?}", RecoveryKey::generate().unwrap()),
            "RecoveryKey(…)"
        );
        assert_ne!(
            DataKey::generate().unwrap(),
            key,
            "two random keys must differ"
        );
    }
}
