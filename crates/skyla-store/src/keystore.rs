use std::collections::HashMap;
use std::sync::Mutex;

use crate::{DataKey, StoreError};

/// Where an unlocked [`DataKey`] may be cached between launches, so the user
/// doesn't type the passphrase every time. The OS keychain backend (macOS
/// Keychain, Windows Credential Manager, Secret Service) is [`OsKeyStore`],
/// behind the `os-keychain` feature. Tests and headless sessions use [`MemoryKeyStore`].
pub trait KeyStore: Send + Sync {
    /// The cached key for `account`, if any.
    fn load(&self, account: &str) -> Result<Option<DataKey>, StoreError>;
    /// Caches `key` for `account`, replacing any previous one.
    fn save(&self, account: &str, key: &DataKey) -> Result<(), StoreError>;
    /// Removes the cached key. Removing a missing key is not an error.
    fn forget(&self, account: &str) -> Result<(), StoreError>;
}

/// An in-process [`KeyStore`]. Keys are wiped when removed or dropped.
#[derive(Debug, Default)]
pub struct MemoryKeyStore {
    keys: Mutex<HashMap<String, DataKey>>,
}

impl MemoryKeyStore {
    fn keys(&self) -> std::sync::MutexGuard<'_, HashMap<String, DataKey>> {
        // A poisoned lock only means another thread panicked mid-insert; the map is still usable.
        self.keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl KeyStore for MemoryKeyStore {
    fn load(&self, account: &str) -> Result<Option<DataKey>, StoreError> {
        Ok(self.keys().get(account).cloned())
    }

    fn save(&self, account: &str, key: &DataKey) -> Result<(), StoreError> {
        self.keys().insert(account.to_owned(), key.clone());
        Ok(())
    }

    fn forget(&self, account: &str) -> Result<(), StoreError> {
        self.keys().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_forgets() {
        let store = MemoryKeyStore::default();
        let key = DataKey::generate().unwrap();
        assert!(store.load("books").unwrap().is_none());
        store.save("books", &key).unwrap();
        assert_eq!(store.load("books").unwrap(), Some(key));
        store.forget("books").unwrap();
        store.forget("books").unwrap();
        assert!(store.load("books").unwrap().is_none());
    }
}

/// The OS keychain: macOS Keychain, Windows Credential Manager, or the
/// Secret Service on Linux (through keyring 4). Each entity's data key is a
/// secret under the `sky-la` service, named by the entity's account.
#[cfg(feature = "os-keychain")]
#[derive(Debug, Clone)]
pub struct OsKeyStore {
    service: String,
}

#[cfg(feature = "os-keychain")]
impl Default for OsKeyStore {
    fn default() -> Self {
        Self {
            service: "sky-la".into(),
        }
    }
}

#[cfg(feature = "os-keychain")]
impl OsKeyStore {
    /// Whether the platform's store is available (e.g. a Secret Service is running).
    pub fn available() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(&self.service, account).map_err(|e| StoreError::Keychain(e.to_string()))
    }
}

#[cfg(feature = "os-keychain")]
impl KeyStore for OsKeyStore {
    fn load(&self, account: &str) -> Result<Option<DataKey>, StoreError> {
        match self.entry(account)?.get_secret() {
            Ok(bytes) => {
                let bytes = zeroize::Zeroizing::new(bytes);
                let key: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                    StoreError::Keychain("the cached key has the wrong length".into())
                })?;
                Ok(Some(DataKey::from_bytes(key)))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(StoreError::Keychain(e.to_string())),
        }
    }

    fn save(&self, account: &str, key: &DataKey) -> Result<(), StoreError> {
        self.entry(account)?
            .set_secret(key.as_bytes())
            .map_err(|e| StoreError::Keychain(e.to_string()))
    }

    fn forget(&self, account: &str) -> Result<(), StoreError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(StoreError::Keychain(e.to_string())),
        }
    }
}
