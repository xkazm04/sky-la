use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

/// One schema step. Versions must run 1, 2, 3, … with no gaps.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Position in the sequence, starting at 1.
    pub version: u32,
    /// Short name recorded in `schema_migrations`.
    pub name: &'static str,
    /// SQL executed in one transaction together with its bookkeeping row.
    pub sql: &'static str,
}

fn validate(migrations: &[Migration]) -> Result<(), StoreError> {
    for (index, migration) in migrations.iter().enumerate() {
        if usize::try_from(migration.version).ok() != Some(index + 1) {
            return Err(StoreError::InvalidMigrations(
                "versions must be 1, 2, 3, … in order",
            ));
        }
    }
    Ok(())
}

/// Applies every migration newer than the database's version. Returns the
/// resulting version. Refuses a database migrated by a newer app.
pub(crate) fn apply(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, StoreError> {
    validate(migrations)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT    NOT NULL,
             applied_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         ) STRICT;",
    )?;
    let current: u32 = conn
        .query_row("SELECT max(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<u32>>(0)
        })
        .optional()?
        .flatten()
        .unwrap_or(0);
    let supported =
        u32::try_from(migrations.len()).map_err(|_| StoreError::InvalidMigrations("too many"))?;
    if current > supported {
        return Err(StoreError::NewerSchema {
            found: current,
            supported,
        });
    }
    for migration in migrations.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
        tx.commit()?;
    }
    Ok(supported.max(current))
}
