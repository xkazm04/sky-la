use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use data_encoding::HEXLOWER;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ErrorCode, OpenFlags};
use sha2::{Digest, Sha256};

use crate::{DataKey, Migration, StoreError, migrate};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// An open, unlocked, encrypted database.
///
/// All writes run in order on one dedicated thread ([`Store::write`]). Reads
/// use a separate connection ([`Store::read`]); with WAL they don't block the
/// writer. Dropping the store finishes queued writes, then closes.
pub struct Store {
    path: PathBuf,
    key: DataKey,
    jobs: Option<mpsc::Sender<Job>>,
    writer: Option<JoinHandle<()>>,
    reader: Mutex<Connection>,
}

fn open_connection(path: &Path, key: &DataKey, flags: OpenFlags) -> Result<Connection, StoreError> {
    let conn = Connection::open_with_flags(path, flags)?;
    conn.execute_batch(&format!(
        "PRAGMA key = {};",
        key.sqlcipher_literal().as_str()
    ))?;
    // SQLCipher only notices a wrong key on the first read.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::NotADatabase => {
            return Err(StoreError::WrongSecret);
        }
        Err(e) => return Err(e.into()),
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    // Journal settings are persistent and need write access; read-only
    // connections inherit them from the file.
    if !flags.contains(OpenFlags::SQLITE_OPEN_READ_ONLY) {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")?;
    }
    Ok(conn)
}

fn lock(reader: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    reader.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Store {
    /// Creates a new encrypted database at `path` and applies `migrations`.
    /// Refuses to touch an existing file.
    pub fn create(
        path: &Path,
        key: &DataKey,
        migrations: &[Migration],
    ) -> Result<Self, StoreError> {
        if path.exists() {
            return Err(StoreError::AlreadyExists(path.to_owned()));
        }
        Self::start(
            path,
            key,
            migrations,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
    }

    /// Opens an existing database and applies any newer `migrations`.
    /// A wrong key fails with [`StoreError::WrongSecret`].
    pub fn open(path: &Path, key: &DataKey, migrations: &[Migration]) -> Result<Self, StoreError> {
        if !path.is_file() {
            return Err(StoreError::NotFound(path.to_owned()));
        }
        Self::start(path, key, migrations, OpenFlags::SQLITE_OPEN_READ_WRITE)
    }

    fn start(
        path: &Path,
        key: &DataKey,
        migrations: &[Migration],
        flags: OpenFlags,
    ) -> Result<Self, StoreError> {
        let mut writer_conn = open_connection(path, key, flags)?;
        migrate::apply(&mut writer_conn, migrations)?;
        let reader = open_connection(path, key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let (jobs, inbox) = mpsc::channel::<Job>();
        let writer = thread::Builder::new()
            .name("skyla-store-writer".into())
            .spawn(move || {
                let mut conn = writer_conn;
                for job in inbox {
                    job(&mut conn);
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            key: key.clone(),
            jobs: Some(jobs),
            writer: Some(writer),
            reader: Mutex::new(reader),
        })
    }

    /// Runs `f` on the writer thread and waits for its result. Wrap multi-step
    /// changes in `conn.transaction()` inside `f`, so a failure rolls back.
    pub fn write<T, F>(&self, f: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
    {
        let (reply, answer) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |conn| {
            // The caller may have stopped waiting; then the result is simply dropped.
            let _ = reply.send(f(conn));
        });
        self.jobs
            .as_ref()
            .ok_or(StoreError::WriterGone)?
            .send(job)
            .map_err(|_| StoreError::WriterGone)?;
        answer.recv().map_err(|_| StoreError::WriterGone)?
    }

    /// Runs `f` on the read-only connection.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        f(&lock(&self.reader))
    }

    /// The highest applied migration version.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        self.read(|conn| {
            Ok(conn.query_row(
                "SELECT coalesce(max(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )?)
        })
    }

    /// Re-encrypts the whole database under `new_key`. Only needed to rotate
    /// the data key; changing the passphrase never requires it.
    pub fn rekey(&mut self, new_key: &DataKey) -> Result<(), StoreError> {
        let literal = new_key.sqlcipher_literal();
        // Leaving WAL needs exclusive access, so close the reader first.
        let mut reader = lock(&self.reader);
        *reader = Connection::open_in_memory()?;
        let rekeyed = self.write(move |conn| {
            // SQLCipher can't rekey in WAL mode: checkpoint, switch to DELETE, rekey, switch back.
            conn.execute_batch(&format!(
                "PRAGMA wal_checkpoint(TRUNCATE);
                 PRAGMA journal_mode = DELETE;
                 PRAGMA rekey = {};
                 PRAGMA journal_mode = WAL;",
                literal.as_str()
            ))?;
            Ok(())
        });
        // Reopen with whichever key now applies, even if the rekey failed.
        let key = if rekeyed.is_ok() { new_key } else { &self.key };
        *reader = open_connection(&self.path, key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        drop(reader);
        rekeyed?;
        self.key = new_key.clone();
        Ok(())
    }

    /// Writes an encrypted copy (same data key) to `dest`, which must not exist.
    /// The copy is a single file with no WAL, consistent as of the call.
    pub fn backup_to(&self, dest: &Path) -> Result<(), StoreError> {
        if dest.exists() {
            return Err(StoreError::AlreadyExists(dest.to_owned()));
        }
        let dest = dest.to_string_lossy().into_owned();
        let literal = self.key.sqlcipher_literal();
        self.write(move |conn| {
            // KEY takes SQLCipher's raw-key literal; it can't be a bound parameter.
            conn.execute(
                &format!("ATTACH DATABASE ?1 AS backup KEY {}", literal.as_str()),
                [&dest],
            )?;
            let exported = conn.query_row("SELECT sqlcipher_export('backup')", [], |_| Ok(()));
            conn.execute_batch("DETACH DATABASE backup;")?;
            exported?;
            Ok(())
        })
    }

    /// Validates the backup at `backup` (key, `integrity_check`, SQLCipher HMACs)
    /// and copies it to `target`, which must not exist.
    pub fn restore(backup: &Path, target: &Path, key: &DataKey) -> Result<(), StoreError> {
        if target.exists() {
            return Err(StoreError::AlreadyExists(target.to_owned()));
        }
        let conn = open_connection(backup, key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Integrity(integrity));
        }
        let mut stmt = conn.prepare("PRAGMA cipher_integrity_check")?;
        let problems: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        if !problems.is_empty() {
            return Err(StoreError::Integrity(problems.join("; ")));
        }
        drop(stmt);
        drop(conn);
        fs::copy(backup, target)?;
        Ok(())
    }

    /// A SHA-256 over every table's schema and rows, independent of row order
    /// and of encryption. Equal hashes mean equal content.
    pub fn content_hash(&self) -> Result<String, StoreError> {
        self.read(|conn| {
            let mut tables = conn.prepare(
                "SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )?;
            let tables: Vec<(String, String)> =
                tables.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<_, _>>()?;
            let mut hasher = Sha256::new();
            for (name, sql) in tables {
                hasher.update(name.as_bytes());
                hasher.update([0]);
                hasher.update(sql.as_bytes());
                hasher.update([0]);
                let mut stmt = conn.prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))?;
                let columns = stmt.column_count();
                let mut rows: Vec<Vec<u8>> = stmt
                    .query_map([], |row| {
                        let mut bytes = Vec::new();
                        for i in 0..columns {
                            encode_value(row.get_ref(i)?, &mut bytes);
                        }
                        Ok(bytes)
                    })?
                    .collect::<Result<_, _>>()?;
                rows.sort_unstable();
                for row in rows {
                    hasher.update((row.len() as u64).to_le_bytes());
                    hasher.update(&row);
                }
            }
            Ok(HEXLOWER.encode(&hasher.finalize()))
        })
    }
}

/// Type-tagged, length-prefixed encoding so different values never collide.
fn encode_value(value: ValueRef<'_>, out: &mut Vec<u8>) {
    match value {
        ValueRef::Null => out.push(0),
        ValueRef::Integer(i) => {
            out.push(1);
            out.extend_from_slice(&i.to_le_bytes());
        }
        ValueRef::Real(r) => {
            out.push(2);
            out.extend_from_slice(&r.to_bits().to_le_bytes());
        }
        ValueRef::Text(t) | ValueRef::Blob(t) => {
            out.push(if matches!(value, ValueRef::Text(_)) {
                3
            } else {
                4
            });
            out.extend_from_slice(&(t.len() as u64).to_le_bytes());
            out.extend_from_slice(t);
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // Closing the channel ends the writer loop after it drains queued jobs.
        drop(self.jobs.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}
