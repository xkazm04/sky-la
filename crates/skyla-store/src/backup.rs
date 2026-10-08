//! Scheduled encrypted backups and the restore drill (WP-30).
//!
//! A backup is an encrypted copy of the database (same data key, so the
//! passphrase and the recovery key both open it) with a manifest beside
//! it: when it was made, the schema version, a content hash, the file's
//! own SHA-256 and the journal's hash-chain head. The head anchors the
//! chain outside the database: someone who rebuilds the whole chain in the
//! live file can't also change every backup's manifest.
//!
//! The restore drill restores a backup into a scratch directory, opens it,
//! and checks that the content and the chain head are what the manifest
//! says. It runs in CI.

use std::fs;
use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DataKey, Store, StoreError, content_hash_of, export_encrypted, open_keyed_read_only};

/// What can be backed up: a [`Store`], or a keyed connection the caller
/// manages ([`Keyed`]).
pub trait Source {
    /// Writes an encrypted copy to `dest` (which must not exist).
    fn export_to(&self, dest: &Path) -> Result<(), StoreError>;
    /// The schema version (0 when the database has no migrations table).
    fn schema_version(&self) -> Result<u32, StoreError>;
    /// The content hash ([`content_hash_of`]).
    fn content_hash(&self) -> Result<String, StoreError>;
}

impl Source for Store {
    fn export_to(&self, dest: &Path) -> Result<(), StoreError> {
        self.backup_to(dest)
    }
    fn schema_version(&self) -> Result<u32, StoreError> {
        Store::schema_version(self)
    }
    fn content_hash(&self) -> Result<String, StoreError> {
        Store::content_hash(self)
    }
}

/// A connection the caller opened with [`crate::open_keyed`], and its key.
pub struct Keyed<'a> {
    /// The connection.
    pub conn: &'a Connection,
    /// Its data key.
    pub key: &'a DataKey,
}

fn schema_version_of(conn: &Connection) -> Result<u32, StoreError> {
    let has: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
        [],
        |r| r.get(0),
    )?;
    if has == 0 {
        return Ok(0);
    }
    Ok(conn.query_row(
        "SELECT coalesce(max(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?)
}

impl Source for Keyed<'_> {
    fn export_to(&self, dest: &Path) -> Result<(), StoreError> {
        export_encrypted(self.conn, self.key, dest)
    }
    fn schema_version(&self) -> Result<u32, StoreError> {
        schema_version_of(self.conn)
    }
    fn content_hash(&self) -> Result<String, StoreError> {
        content_hash_of(self.conn)
    }
}

/// The backup file's extension.
pub const EXTENSION: &str = "skyla-backup";

/// How often to back up and how many backups to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPolicy {
    /// Back up when the newest backup is at least this many days old.
    pub every_days: u32,
    /// Keep this many backups; older ones are removed.
    pub keep: usize,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            every_days: 1,
            keep: 14,
        }
    }
}

/// What a backup's manifest records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Manifest format.
    pub format: u32,
    /// The entity, as named in the file.
    pub entity: String,
    /// When it was made (UTC, RFC 3339).
    pub created_at: String,
    /// The database schema version.
    pub schema_version: u32,
    /// [`Store::content_hash`] at backup time.
    pub content_hash: String,
    /// SHA-256 of the backup file.
    pub file_sha256: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// The journal's hash-chain head when it was made, if the books have one.
    pub chain_head: Option<String>,
}

/// A backup on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The encrypted database copy.
    pub file: PathBuf,
    /// Its manifest.
    pub manifest: Manifest,
}

fn slug(entity: &str) -> String {
    let s: String = entity
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "entity".into() } else { s }
}

fn compact(at: &str) -> String {
    at.chars().filter(char::is_ascii_alphanumeric).collect()
}

fn sha256_file(path: &Path) -> Result<String, StoreError> {
    let bytes = fs::read(path)?;
    Ok(HEXLOWER.encode(&Sha256::digest(&bytes)))
}

fn manifest_path(file: &Path) -> PathBuf {
    file.with_extension(format!("{EXTENSION}.json"))
}

/// Makes a backup of `store` in `dir` now (`now` is UTC, RFC 3339), with
/// `chain_head` from the ledger.
pub fn backup_now(
    store: &impl Source,
    dir: &Path,
    entity: &str,
    now: &str,
    chain_head: Option<String>,
) -> Result<Backup, StoreError> {
    fs::create_dir_all(dir)?;
    let file = dir.join(format!(
        "skyla-{}-{}.{EXTENSION}",
        slug(entity),
        compact(now)
    ));
    store.export_to(&file)?;
    let manifest = Manifest {
        format: 1,
        entity: entity.to_owned(),
        created_at: now.to_owned(),
        schema_version: store.schema_version()?,
        content_hash: store.content_hash()?,
        file_sha256: sha256_file(&file)?,
        bytes: fs::metadata(&file)?.len(),
        chain_head,
    };
    let json =
        serde_json::to_vec_pretty(&manifest).map_err(|e| StoreError::VaultFormat(e.to_string()))?;
    fs::write(manifest_path(&file), json)?;
    Ok(Backup { file, manifest })
}

/// The entity's backups in `dir`, newest first. Files without a readable
/// manifest are skipped.
pub fn list(dir: &Path, entity: &str) -> Result<Vec<Backup>, StoreError> {
    let prefix = format!("skyla-{}-", slug(entity));
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for e in fs::read_dir(dir)? {
        let path = e?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !name.starts_with(&prefix) || !name.ends_with(&format!(".{EXTENSION}")) {
            continue;
        }
        let Ok(text) = fs::read(manifest_path(&path)) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<Manifest>(&text) else {
            continue;
        };
        out.push(Backup {
            file: path,
            manifest,
        });
    }
    out.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
    Ok(out)
}

/// Days since 1970 for the date part of an RFC 3339 timestamp.
fn day_of(at: &str) -> Option<i64> {
    let y: i64 = at.get(0..4)?.parse().ok()?;
    let m: i64 = at.get(5..7)?.parse().ok()?;
    let d: i64 = at.get(8..10)?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    Some(era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468)
}

/// A backup is due when there's none, or the newest is old enough.
pub fn is_due(policy: BackupPolicy, newest: Option<&Backup>, now: &str) -> bool {
    match newest
        .and_then(|b| day_of(&b.manifest.created_at))
        .zip(day_of(now))
    {
        Some((last, today)) => today - last >= i64::from(policy.every_days),
        None => true,
    }
}

/// Removes all but the newest `keep` backups; returns what was removed.
pub fn prune(dir: &Path, entity: &str, keep: usize) -> Result<Vec<PathBuf>, StoreError> {
    let mut removed = Vec::new();
    for b in list(dir, entity)?.into_iter().skip(keep.max(1)) {
        fs::remove_file(&b.file)?;
        let _ = fs::remove_file(manifest_path(&b.file));
        removed.push(b.file);
    }
    Ok(removed)
}

/// What the drill found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrillReport {
    /// The backup.
    pub file: String,
    /// The file is the one the manifest describes.
    pub file_matches: bool,
    /// It restored and opened with the key.
    pub opens: bool,
    /// Its content hash equals the manifest's.
    pub content_matches: bool,
    /// Its chain head equals the manifest's (when the manifest has one).
    pub chain_matches: Option<bool>,
}

impl DrillReport {
    /// Everything checked out.
    pub fn passed(&self) -> bool {
        self.file_matches && self.opens && self.content_matches && self.chain_matches != Some(false)
    }
}

/// Restores `backup` into `scratch`, opens it read-only and checks it
/// against its manifest. `head_of` reads the journal's chain head from the
/// restored database (the ledger knows how).
pub fn drill(
    backup: &Backup,
    key: &DataKey,
    scratch: &Path,
    head_of: impl FnOnce(&Connection) -> Option<String>,
) -> Result<DrillReport, StoreError> {
    let mut report = DrillReport {
        file: backup.file.display().to_string(),
        file_matches: sha256_file(&backup.file)? == backup.manifest.file_sha256,
        opens: false,
        content_matches: false,
        chain_matches: None,
    };
    if !report.file_matches {
        return Ok(report);
    }
    // Its own directory: the opened copy leaves WAL files beside it.
    let work = scratch.join(format!("drill-{}", compact(&backup.manifest.created_at)));
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;
    let target = work.join("restored.db");
    Store::restore(&backup.file, &target, key)?;
    // Read-only, so the check can't change what it checks.
    let conn = open_keyed_read_only(&target, key)?;
    report.opens = true;
    report.content_matches = content_hash_of(&conn)? == backup.manifest.content_hash
        && schema_version_of(&conn)? == backup.manifest.schema_version;
    if let Some(expected) = backup.manifest.chain_head.clone() {
        report.chain_matches = Some(head_of(&conn).as_deref() == Some(expected.as_str()));
    }
    drop(conn);
    let _ = fs::remove_dir_all(&work);
    Ok(report)
}
