use std::collections::HashMap;
use std::sync::Mutex;

use crate::{DataKey, StoreError};

/// Where an unlocked [`DataKey`] may be cached between launches, so the user
/// doesn't type the passphrase every time. The OS keychain backend (macOS
/// Keychain, Windows Credential Manager, Secret Service) is wired up with the
/// unlock UX in WP-30. Tests and headless sessions use [`MemoryKeyStore`].
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
