use std::fs;
use std::io::Write;
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::secret::{KEY_LEN, constant_time_eq, random_bytes};
use crate::{DataKey, RecoveryKey, StoreError};

/// The shortest passphrase accepted. The UI may ask for more.
pub const MIN_PASSPHRASE_CHARS: usize = 10;

const VERSION: u32 = 1;
const NONCE_LEN: usize = 24;
// Bind each wrapped copy to its purpose so the two can't be swapped in the file.
const AAD_PASSPHRASE: &[u8] = b"sky-la/vault/v1/passphrase";
const AAD_RECOVERY: &[u8] = b"sky-la/vault/v1/recovery";
const KEY_CHECK_DOMAIN: &[u8] = b"sky-la/vault/v1/key-check";

/// Argon2id cost parameters, stored in the vault so they can be raised later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub m_cost_kib: u32,
    /// Number of passes.
    pub t_cost: u32,
    /// Parallelism.
    pub p_cost: u32,
}

impl KdfParams {
    /// Default for unlocking on a desktop: 64 MiB, 3 passes, 1 lane.
    pub const RECOMMENDED: Self = Self {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Wrapped {
    nonce: String,
    ciphertext: String,
}

/// The key envelope, stored beside the database as JSON. Nothing in it is
/// secret on its own: it holds the data key encrypted twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    version: u32,
    kdf: KdfParams,
    salt: String,
    by_passphrase: Wrapped,
    by_recovery: Wrapped,
    /// SHA-256 over a domain tag and the data key. Lets the vault confirm a
    /// key is its own without the passphrase; reveals nothing about a 256-bit
    /// random key.
    key_check: String,
}

fn key_check(key: &DataKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(KEY_CHECK_DOMAIN);
    hasher.update(key.as_bytes());
    HEXLOWER.encode(&hasher.finalize())
}

fn check_passphrase(passphrase: &str) -> Result<(), StoreError> {
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(StoreError::PassphraseTooShort {
            min: MIN_PASSPHRASE_CHARS,
        });
    }
    Ok(())
}

fn derive_kek(
    passphrase: &str,
    salt: &[u8],
    kdf: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, StoreError> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|e| StoreError::Kdf(e.to_string()))?;
    let mut kek = Zeroizing::new([0_u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, kek.as_mut())
        .map_err(|e| StoreError::Kdf(e.to_string()))?;
    Ok(kek)
}

fn wrap(kek: &[u8; KEY_LEN], key: &DataKey, aad: &[u8]) -> Result<Wrapped, StoreError> {
    let cipher = XChaCha20Poly1305::new_from_slice(kek).map_err(|_| StoreError::Encrypt)?;
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| StoreError::Random)?;
    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: key.as_bytes(),
                aad,
            },
        )
        .map_err(|_| StoreError::Encrypt)?;
    Ok(Wrapped {
        nonce: HEXLOWER.encode(&nonce),
        ciphertext: HEXLOWER.encode(&ciphertext),
    })
}

fn unwrap(kek: &[u8; KEY_LEN], wrapped: &Wrapped, aad: &[u8]) -> Result<DataKey, StoreError> {
    let malformed = |what: &str| StoreError::VaultFormat(format!("{what} is not valid hex"));
    let nonce: [u8; NONCE_LEN] = HEXLOWER
        .decode(wrapped.nonce.as_bytes())
        .map_err(|_| malformed("nonce"))?
        .try_into()
        .map_err(|_| StoreError::VaultFormat("nonce has the wrong length".into()))?;
    let ciphertext = HEXLOWER
        .decode(wrapped.ciphertext.as_bytes())
        .map_err(|_| malformed("ciphertext"))?;
    let cipher = XChaCha20Poly1305::new_from_slice(kek).map_err(|_| StoreError::Encrypt)?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &ciphertext,
                    aad,
                },
            )
            .map_err(|_| StoreError::WrongSecret)?,
    );
    let bytes: [u8; KEY_LEN] = plain
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::WrongSecret)?;
    Ok(DataKey::from_bytes(bytes))
}

impl Vault {
    /// Creates a new data key and a vault protecting it. Returns the vault, the
    /// data key (to open the database now) and the recovery key (show it once).
    pub fn create(
        passphrase: &str,
        kdf: KdfParams,
    ) -> Result<(Self, DataKey, RecoveryKey), StoreError> {
        check_passphrase(passphrase)?;
        let key = DataKey::generate()?;
        let recovery = RecoveryKey::generate()?;
        let salt = random_bytes()?;
        let kek = derive_kek(passphrase, &salt, kdf)?;
        let vault = Self {
            version: VERSION,
            kdf,
            salt: HEXLOWER.encode(&salt),
            by_passphrase: wrap(&kek, &key, AAD_PASSPHRASE)?,
            by_recovery: wrap(recovery.as_bytes(), &key, AAD_RECOVERY)?,
            key_check: key_check(&key),
        };
        Ok((vault, key, recovery))
    }

    fn salt(&self) -> Result<Vec<u8>, StoreError> {
        HEXLOWER
            .decode(self.salt.as_bytes())
            .map_err(|_| StoreError::VaultFormat("salt is not valid hex".into()))
    }

    /// Unwraps the data key with the passphrase.
    pub fn unlock_with_passphrase(&self, passphrase: &str) -> Result<DataKey, StoreError> {
        let kek = derive_kek(passphrase, &self.salt()?, self.kdf)?;
        let key = unwrap(&kek, &self.by_passphrase, AAD_PASSPHRASE)?;
        self.verify_key(&key)?;
        Ok(key)
    }

    /// Unwraps the data key with the recovery key.
    pub fn unlock_with_recovery(&self, recovery: &RecoveryKey) -> Result<DataKey, StoreError> {
        let key = unwrap(recovery.as_bytes(), &self.by_recovery, AAD_RECOVERY)?;
        self.verify_key(&key)?;
        Ok(key)
    }

    /// Sets a new passphrase (with a fresh salt). The data key, and so the
    /// database, are unchanged. Proves possession of the key first.
    pub fn change_passphrase(
        &mut self,
        key: &DataKey,
        new_passphrase: &str,
    ) -> Result<(), StoreError> {
        check_passphrase(new_passphrase)?;
        self.verify_key(key)?;
        let salt = random_bytes()?;
        let kek = derive_kek(new_passphrase, &salt, self.kdf)?;
        self.salt = HEXLOWER.encode(&salt);
        self.by_passphrase = wrap(&kek, key, AAD_PASSPHRASE)?;
        Ok(())
    }

    /// Replaces the recovery key, e.g. after the old one was exposed.
    pub fn rotate_recovery_key(&mut self, key: &DataKey) -> Result<RecoveryKey, StoreError> {
        self.verify_key(key)?;
        let recovery = RecoveryKey::generate()?;
        self.by_recovery = wrap(recovery.as_bytes(), key, AAD_RECOVERY)?;
        Ok(recovery)
    }

    /// Fails with [`StoreError::WrongSecret`] unless `key` is the key this vault protects.
    pub fn verify_key(&self, key: &DataKey) -> Result<(), StoreError> {
        if constant_time_eq(key_check(key).as_bytes(), self.key_check.as_bytes()) {
            Ok(())
        } else {
            Err(StoreError::WrongSecret)
        }
    }

    /// Serialises to JSON.
    pub fn to_json(&self) -> Result<String, StoreError> {
        serde_json::to_string_pretty(self).map_err(|e| StoreError::VaultFormat(e.to_string()))
    }

    /// Parses JSON, rejecting unknown versions.
    pub fn from_json(json: &str) -> Result<Self, StoreError> {
        let vault: Self =
            serde_json::from_str(json).map_err(|e| StoreError::VaultFormat(e.to_string()))?;
        if vault.version != VERSION {
            return Err(StoreError::VaultVersion(vault.version));
        }
        Ok(vault)
    }

    /// Writes atomically: a temporary file, synced, then renamed into place.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let tmp = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(self.to_json()?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Reads a vault file.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        Self::from_json(&fs::read_to_string(path)?)
    }
}
