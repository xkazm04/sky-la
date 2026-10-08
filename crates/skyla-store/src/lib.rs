//! Encrypted storage: SQLCipher, migrations, single-writer actor, key management and backups.
//!
//! The database is encrypted with a random 256-bit [`DataKey`]. That key never
//! changes when the passphrase does: a [`Vault`] keeps it wrapped twice with
//! XChaCha20-Poly1305, once under a key derived from the passphrase (Argon2id)
//! and once under a printable [`RecoveryKey`]. Losing the passphrase therefore
//! doesn't lose the books.
//!
//! [`Store`] owns one writer thread (every write goes through it, in order)
//! and a reader connection. Backups are SQLCipher exports under the same data
//! key, so the same vault unlocks them; [`backup`] schedules them, keeps a
//! manifest that anchors the journal's chain head, and runs the restore drill.

pub mod backup;
mod error;
mod keystore;
mod migrate;
mod secret;
mod store;
mod vault;

pub use error::StoreError;
#[cfg(feature = "os-keychain")]
pub use keystore::OsKeyStore;
pub use keystore::{KeyStore, MemoryKeyStore};
pub use migrate::Migration;
pub use secret::{DataKey, RecoveryKey};
pub use store::Store;
pub use vault::{KdfParams, MIN_PASSPHRASE_CHARS, Vault};
