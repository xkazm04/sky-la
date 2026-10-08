//! The egress register: an append-only record of every advisor run, kept
//! in the entity's encrypted database. Each run stores the exact bytes that
//! left the machine (prompt and tool results, after the gate), so a replay
//! is byte-identical, and a hash chain shows that nothing was edited or
//! removed afterwards.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Errors from the register.
#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    /// The database refused.
    #[error("egress register: {0}")]
    Db(#[from] rusqlite::Error),
    /// No such run.
    #[error("no run {0} in the egress register")]
    NoRun(i64),
}

/// The schema, safe to apply twice.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS egress_run (
    id          INTEGER PRIMARY KEY,
    uid         TEXT    NOT NULL UNIQUE,
    at          TEXT    NOT NULL,
    task        TEXT    NOT NULL,
    advisor     TEXT    NOT NULL,
    purpose     TEXT    NOT NULL,
    provider    TEXT    NOT NULL,
    model       TEXT    NOT NULL,
    scopes      TEXT    NOT NULL,
    withheld    TEXT    NOT NULL,
    tool_calls  INTEGER NOT NULL CHECK (tool_calls >= 0),
    payload     BLOB    NOT NULL,
    outcome     TEXT    NOT NULL,
    cost_note   TEXT,
    prev_hash   TEXT    NOT NULL,
    hash        TEXT    NOT NULL UNIQUE
) STRICT;
CREATE TRIGGER IF NOT EXISTS egress_run_no_update BEFORE UPDATE ON egress_run
BEGIN SELECT RAISE(ABORT, 'the egress register is append-only'); END;
CREATE TRIGGER IF NOT EXISTS egress_run_no_delete BEFORE DELETE ON egress_run
BEGIN SELECT RAISE(ABORT, 'the egress register is append-only'); END;
";

/// Applies [`SCHEMA`].
pub fn apply_schema(conn: &Connection) -> Result<(), RegisterError> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

/// A run to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRun {
    /// Stable id.
    pub uid: String,
    /// RFC 3339 UTC.
    pub at: String,
    /// Task type, e.g. `tax.scenarios`.
    pub task: String,
    /// Which advisor.
    pub advisor: String,
    /// What for, for people.
    pub purpose: String,
    /// The provider.
    pub provider: String,
    /// The model setting.
    pub model: String,
    /// The scope's field classes.
    pub scopes: Vec<String>,
    /// What the gate withheld, one line each.
    pub withheld: Vec<String>,
    /// Tool calls answered.
    pub tool_calls: u32,
    /// Exactly what was sent.
    pub payload: Vec<u8>,
    /// What came of it.
    pub outcome: String,
    /// The provider's own cost estimate, as text ("about $0.002").
    pub cost_note: Option<String>,
}

/// A recorded run, without its payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Row id.
    pub id: i64,
    /// The run as recorded.
    pub run: NewRun,
    /// Its link in the chain.
    pub hash: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn link(prev: &str, r: &NewRun) -> String {
    let mut h = Sha256::new();
    for part in [
        prev,
        &r.uid,
        &r.at,
        &r.task,
        &r.advisor,
        &r.purpose,
        &r.provider,
        &r.model,
        &r.scopes.join("\u{1f}"),
        &r.withheld.join("\u{1f}"),
        &r.tool_calls.to_string(),
        &r.outcome,
        r.cost_note.as_deref().unwrap_or(""),
    ] {
        h.update(part.as_bytes());
        h.update([0x1e]);
    }
    h.update(&r.payload);
    hex(&h.finalize())
}

const GENESIS: &str = "egress-register-v1";

/// Appends a run and returns its id.
pub fn record(conn: &Connection, r: &NewRun) -> Result<i64, RegisterError> {
    let prev: String = conn
        .query_row(
            "SELECT hash FROM egress_run ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| GENESIS.to_owned());
    let hash = link(&prev, r);
    conn.execute(
        "INSERT INTO egress_run (uid, at, task, advisor, purpose, provider, model, scopes, withheld,
                                 tool_calls, payload, outcome, cost_note, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            r.uid,
            r.at,
            r.task,
            r.advisor,
            r.purpose,
            r.provider,
            r.model,
            serde_json::to_string(&r.scopes).unwrap_or_default(),
            serde_json::to_string(&r.withheld).unwrap_or_default(),
            r.tool_calls,
            r.payload,
            r.outcome,
            r.cost_note,
            prev,
            hash,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Run, Vec<u8>, String)> {
    let list = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
    Ok((
        Run {
            id: row.get(0)?,
            run: NewRun {
                uid: row.get(1)?,
                at: row.get(2)?,
                task: row.get(3)?,
                advisor: row.get(4)?,
                purpose: row.get(5)?,
                provider: row.get(6)?,
                model: row.get(7)?,
                scopes: list(row.get(8)?),
                withheld: list(row.get(9)?),
                tool_calls: row.get(10)?,
                payload: Vec::new(),
                outcome: row.get(12)?,
                cost_note: row.get(13)?,
            },
            hash: row.get(15)?,
        },
        row.get(11)?,
        row.get(14)?,
    ))
}

const COLUMNS: &str = "id, uid, at, task, advisor, purpose, provider, model, scopes, withheld,
                       tool_calls, payload, outcome, cost_note, prev_hash, hash";

/// Every run, newest first, without payloads.
pub fn list(conn: &Connection) -> Result<Vec<Run>, RegisterError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM egress_run ORDER BY at DESC, id DESC"
    ))?;
    let rows = stmt.query_map([], row_to_run)?;
    Ok(rows
        .map(|r| r.map(|(run, _, _)| run))
        .collect::<Result<_, _>>()?)
}

/// The exact bytes a run sent.
pub fn replay(conn: &Connection, id: i64) -> Result<Vec<u8>, RegisterError> {
    conn.query_row("SELECT payload FROM egress_run WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or(RegisterError::NoRun(id))
}

/// The first run whose link doesn't follow from its predecessor, if any.
pub fn verify(conn: &Connection) -> Result<Option<i64>, RegisterError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM egress_run ORDER BY id"))?;
    let rows = stmt.query_map([], row_to_run)?;
    let mut prev = GENESIS.to_owned();
    for row in rows {
        let (mut run, payload, stored_prev) = row?;
        run.run.payload = payload;
        if stored_prev != prev || link(&prev, &run.run) != run.hash {
            return Ok(Some(run.id));
        }
        prev = run.hash;
    }
    Ok(None)
}
