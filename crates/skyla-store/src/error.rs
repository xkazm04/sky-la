use std::path::PathBuf;

/// Errors from storage and key management.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The passphrase, recovery key or data key doesn't open this vault or database.
    #[error("wrong passphrase, recovery key or data key")]
    WrongSecret,
    /// The passphrase is shorter than [`crate::MIN_PASSPHRASE_CHARS`].
    #[error("a passphrase needs at least {min} characters")]
    PassphraseTooShort {
        /// The minimum length in characters.
        min: usize,
    },
    /// The recovery key text isn't a well-formed key.
    #[error("invalid recovery key: {0}")]
    InvalidRecoveryKey(&'static str),
    /// The operating system's random number generator failed.
    #[error("the system random number generator failed")]
    Random,
    /// Argon2id rejected its parameters.
    #[error("key derivation failed: {0}")]
    Kdf(String),
    /// Encryption of a key failed.
    #[error("key encryption failed")]
    Encrypt,
    /// The vault file can't be parsed.
    #[error("vault file is malformed: {0}")]
    VaultFormat(String),
    /// The vault was written by a newer format version.
    #[error("unsupported vault version {0}")]
    VaultVersion(u32),
    /// Refusing to overwrite an existing file.
    #[error("a file already exists at {0}")]
    AlreadyExists(PathBuf),
    /// There is no database at the path.
    #[error("no database at {0}")]
    NotFound(PathBuf),
    /// The database was migrated by a newer version of the app.
    #[error("database schema version {found} is newer than this app supports ({supported})")]
    NewerSchema {
        /// Version recorded in the database.
        found: u32,
        /// Highest version this build knows.
        supported: u32,
    },
    /// The migration list isn't 1, 2, 3, … in order.
    #[error("invalid migration list: {0}")]
    InvalidMigrations(&'static str),
    /// `PRAGMA integrity_check` or the SQLCipher HMAC check failed.
    #[error("integrity check failed: {0}")]
    Integrity(String),
    /// The writer thread has stopped, so no more writes are possible.
    #[error("the database writer thread has stopped")]
    WriterGone,
    /// SQLite or SQLCipher error.
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    /// The OS keychain refused or isn't available.
    #[error("keychain: {0}")]
    Keychain(String),
    /// Filesystem error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
