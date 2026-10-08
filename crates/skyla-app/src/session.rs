//! The session gate (WP-30): before any books are open, the shell talks
//! to this. It sets up a new entity (passphrase, recovery key shown once and
//! confirmed), unlocks one with the passphrase, the OS keychain or the
//! recovery key, and hands back the [`Core`]. The passphrase and keys never
//! reach the webview beyond the recovery key the user must write down.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use skyla_store::{DataKey, KdfParams, KeyStore, MemoryKeyStore, RecoveryKey, Vault};

use crate::Core;
use crate::dto::{EntitySetupDto, RecoveryKeyDto, SessionStateDto};
use crate::error::CoreError;

const VAULT: &str = "vault.json";
const BOOKS: &str = "books.db";
/// Only the name, so the unlock screen can say whose books these are.
const LABEL: &str = "entity.json";
const ACCOUNT: &str = "books";

fn store(e: skyla_store::StoreError) -> CoreError {
    match e {
        skyla_store::StoreError::WrongSecret => {
            CoreError::BadRequest("that passphrase or recovery key doesn't open these books".into())
        }
        other => CoreError::BadRequest(other.to_string()),
    }
}

/// Where keys and dates come from.
enum Source {
    /// The OS random generator and the system clock: the app.
    System,
    /// Keys derived from a counter and a fixed day: recordings and tests,
    /// so every run gives the same answers.
    Reproducible { counter: AtomicU64, today: String },
}

/// The gate in front of one entity's folder.
pub struct Gate {
    dir: PathBuf,
    kdf: KdfParams,
    source: Source,
    keystore: Box<dyn KeyStore>,
    /// The recovery key just shown, until the user confirms they saved it.
    pending: Mutex<Option<RecoveryKey>>,
}

/// Now in UTC from the system clock (RFC 3339).
pub fn system_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        system_today(),
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Today in UTC from the system clock.
fn system_today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    skyla_rules::date::format(secs.div_euclid(86_400))
}

impl Gate {
    /// The app's gate: OS randomness, the recommended KDF, the system clock.
    pub fn new(dir: PathBuf, keystore: Box<dyn KeyStore>) -> Self {
        Self {
            dir,
            kdf: KdfParams::RECOMMENDED,
            source: Source::System,
            keystore,
            pending: Mutex::new(None),
        }
    }

    /// A reproducible gate for recordings and tests: derived keys, a cheap
    /// KDF and the demo's date. Never for real books.
    pub fn reproducible(dir: PathBuf) -> Self {
        Self {
            dir,
            kdf: KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
            source: Source::Reproducible {
                counter: AtomicU64::new(0),
                today: "2026-10-07".into(),
            },
            keystore: Box::new(MemoryKeyStore::default()),
            pending: Mutex::new(None),
        }
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Today, from the gate's clock.
    pub fn today(&self) -> String {
        match &self.source {
            Source::System => system_today(),
            Source::Reproducible { today, .. } => today.clone(),
        }
    }

    /// "Now" for backups (UTC, RFC 3339).
    pub fn now(&self) -> String {
        match &self.source {
            Source::System => {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
                let rem = secs.rem_euclid(86_400);
                format!(
                    "{}T{:02}:{:02}:{:02}Z",
                    system_today(),
                    rem / 3600,
                    rem % 3600 / 60,
                    rem % 60
                )
            }
            Source::Reproducible { today, .. } => format!("{today}T12:00:00Z"),
        }
    }

    fn bytes(&self) -> Result<[u8; 32], CoreError> {
        match &self.source {
            Source::System => {
                let k = DataKey::generate().map_err(store)?;
                Ok(*k.as_bytes())
            }
            Source::Reproducible { counter, .. } => {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Sha256::digest(format!("sky-la reproducible key {n}").as_bytes()).into())
            }
        }
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join(VAULT)
    }

    /// Whether there are books to unlock, and whose.
    pub fn state(&self) -> SessionStateDto {
        let label = std::fs::read_to_string(self.dir.join(LABEL))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["displayName"].as_str().map(str::to_owned));
        let has_vault = self.vault_path().is_file();
        SessionStateDto {
            state: if has_vault { "locked" } else { "needs_setup" }.into(),
            entity: label,
            remembered: has_vault && self.keystore.load(ACCOUNT).ok().flatten().is_some(),
        }
    }

    fn shown(&self, recovery: RecoveryKey) -> RecoveryKeyDto {
        let key = recovery.to_display().to_string();
        let groups = u32::try_from(key.split('-').count()).unwrap_or(0);
        if let Ok(mut p) = self.pending.lock() {
            *p = Some(recovery);
        }
        RecoveryKeyDto { key, groups }
    }

    /// Creates the entity's books, protected by `passphrase`, and returns
    /// the recovery key to show once.
    pub fn create(
        &self,
        setup: &EntitySetupDto,
        passphrase: &str,
    ) -> Result<(Core, RecoveryKeyDto), CoreError> {
        if self.vault_path().exists() {
            return Err(CoreError::BadRequest(
                "these books already exist; unlock them instead".into(),
            ));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let (vault, key, recovery) = Vault::create_with(
            passphrase,
            self.kdf,
            DataKey::from_bytes(self.bytes()?),
            RecoveryKey::from_bytes(self.bytes()?),
        )
        .map_err(store)?;
        let core = Core::create_entity(&self.dir.join(BOOKS), &key, setup, &self.today())?;
        vault.save(&self.vault_path()).map_err(store)?;
        std::fs::write(
            self.dir.join(LABEL),
            serde_json::json!({ "displayName": setup.display_name.trim() }).to_string(),
        )
        .map_err(|e| CoreError::BadRequest(e.to_string()))?;
        Ok((core, self.shown(recovery)))
    }

    /// Checks the user saved the recovery key just shown, by its last group.
    pub fn confirm_recovery_key(&self, last_group: &str) -> Result<bool, CoreError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| CoreError::BadRequest("try again".into()))?;
        let Some(key) = pending.as_ref() else {
            return Err(CoreError::BadRequest(
                "there's no recovery key waiting to be confirmed".into(),
            ));
        };
        let shown = key.to_display();
        let expected = shown.rsplit('-').next().unwrap_or_default();
        let typed: String = last_group.chars().filter(|c| !c.is_whitespace()).collect();
        let ok = typed.eq_ignore_ascii_case(expected);
        if ok {
            *pending = None;
        }
        Ok(ok)
    }

    /// Unlocks with the passphrase; with `remember`, the key is kept in the
    /// OS keychain so the next start doesn't ask.
    pub fn unlock(&self, passphrase: &str, remember: bool) -> Result<Core, CoreError> {
        let vault = Vault::load(&self.vault_path()).map_err(store)?;
        let key = vault.unlock_with_passphrase(passphrase).map_err(store)?;
        let core = Core::open_entity(&self.dir.join(BOOKS), &key, &self.today())?;
        if remember {
            self.keystore.save(ACCOUNT, &key).map_err(store)?;
        } else {
            self.keystore.forget(ACCOUNT).map_err(store)?;
        }
        Ok(core)
    }

    /// Checks the passphrase again before something that leaves the
    /// encryption behind (the full export). Opens nothing.
    pub fn confirm_passphrase(&self, passphrase: &str) -> Result<(), CoreError> {
        let vault = Vault::load(&self.vault_path()).map_err(store)?;
        vault.unlock_with_passphrase(passphrase).map_err(store)?;
        Ok(())
    }

    /// Unlocks with the key kept in the OS keychain, if there is one.
    pub fn unlock_remembered(&self) -> Result<Option<Core>, CoreError> {
        let Some(key) = self.keystore.load(ACCOUNT).map_err(store)? else {
            return Ok(None);
        };
        let vault = Vault::load(&self.vault_path()).map_err(store)?;
        if vault.verify_key(&key).is_err() {
            // A stale key from other books: forget it and ask.
            self.keystore.forget(ACCOUNT).map_err(store)?;
            return Ok(None);
        }
        Core::open_entity(&self.dir.join(BOOKS), &key, &self.today()).map(Some)
    }

    /// Opens the books with the recovery key, sets a new passphrase, and
    /// replaces the recovery key (the old one has been used).
    pub fn recover(
        &self,
        recovery: &str,
        new_passphrase: &str,
    ) -> Result<(Core, RecoveryKeyDto), CoreError> {
        let mut vault = Vault::load(&self.vault_path()).map_err(store)?;
        let old = RecoveryKey::parse(recovery).map_err(store)?;
        let key = vault.unlock_with_recovery(&old).map_err(store)?;
        vault
            .change_passphrase(&key, new_passphrase)
            .map_err(store)?;
        let fresh = vault
            .rotate_recovery_key_with(&key, RecoveryKey::from_bytes(self.bytes()?))
            .map_err(store)?;
        vault.save(&self.vault_path()).map_err(store)?;
        self.keystore.forget(ACCOUNT).map_err(store)?;
        let core = Core::open_entity(&self.dir.join(BOOKS), &key, &self.today())?;
        Ok((core, self.shown(fresh)))
    }
}
