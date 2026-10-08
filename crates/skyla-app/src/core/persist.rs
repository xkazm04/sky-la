//! What real books keep between sessions, beside the journal (improvement
//! wave 2): the bank workbench (imported statements, rules, what's booked,
//! learnt payer accounts), the advisor policies, the inbox's advisor items
//! and dismissals (wave 12), reference data and its
//! switch, the update-check switch, and a verified rule-pack update. Each is
//! a JSON document in `app_state`, inside the same encrypted file, written
//! after every change. The demo keeps all of it in memory.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Core;
use crate::error::CoreError;

const SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS app_state (key TEXT PRIMARY KEY, json TEXT NOT NULL) STRICT;";

const BANK: &str = "bank";
const POLICIES: &str = "egress_policies";
const REFDATA: &str = "reference_data";
const UPDATES: &str = "update_check";
const PACK: &str = "pack_update";
const INBOX: &str = "inbox";

/// A rule-pack update, as installed: its text and its signature, verified
/// again whenever the books open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedPack {
    pub(crate) toml: String,
    pub(crate) signature: String,
}

#[derive(Serialize, Deserialize)]
struct SavedRefData {
    fx: Vec<skyla_rules::refdata::FxDay>,
    repo: Vec<skyla_rules::refdata::RepoChange>,
    sources: Vec<crate::dto::RefSourceDto>,
    fetch_enabled: bool,
}

fn json_err(e: serde_json::Error) -> CoreError {
    CoreError::BadRequest(format!("saved state: {e}"))
}

fn sql_err(e: rusqlite::Error) -> CoreError {
    CoreError::Ledger(skyla_ledger::LedgerError::from(e))
}

pub(crate) fn apply_schema(conn: &Connection) -> Result<(), CoreError> {
    conn.execute_batch(SCHEMA).map_err(sql_err)?;
    conn.execute_batch(super::purchases::SCHEMA)
        .map_err(sql_err)
}

fn load(conn: &Connection, key: &str) -> Result<Option<Value>, CoreError> {
    let text: Option<String> = conn
        .query_row("SELECT json FROM app_state WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sql_err)?;
    text.map(|t| serde_json::from_str(&t).map_err(json_err))
        .transpose()
}

/// The rule pack to use: a saved update when it still verifies against the
/// trusted keys and is newer than the built-in pack, else the built-in one.
pub(crate) fn pack_for(conn: &Connection, keys: &[&str]) -> Result<skyla_rules::Pack, CoreError> {
    let built_in = skyla_rules::Pack::cz_2026()?;
    let saved = load(conn, PACK)?
        .map(serde_json::from_value::<SavedPack>)
        .transpose()
        .map_err(json_err)?;
    Ok(saved
        .and_then(|s| skyla_rules::verify_pack_update(&s.toml, &s.signature, keys, &built_in).ok())
        .unwrap_or(built_in))
}

impl Core {
    fn save(&self, key: &str, value: Value) -> Result<(), CoreError> {
        if self.is_demo() {
            return Ok(());
        }
        let json = serde_json::to_string(&value).map_err(json_err)?;
        self.db()
            .execute(
                "INSERT INTO app_state (key, json) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET json = excluded.json",
                params![key, json],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    /// Saves the bank workbench.
    pub(crate) fn persist_bank(&self) -> Result<(), CoreError> {
        let value = serde_json::to_value(&*self.bank_state()).map_err(json_err)?;
        self.save(BANK, value)
    }

    /// Saves the advisor policies.
    pub(crate) fn persist_policies(&self) -> Result<(), CoreError> {
        let map: std::collections::BTreeMap<String, skyla_egress::Policy> = self
            .egress_policies
            .lock()
            .map(|p| p.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect())
            .unwrap_or_default();
        self.save(POLICIES, serde_json::to_value(map).map_err(json_err)?)
    }

    /// Saves the reference data and its switch.
    pub(crate) fn persist_refdata(&self) -> Result<(), CoreError> {
        let value = {
            let s = self.refdata_state();
            serde_json::to_value(SavedRefData {
                fx: s.fx.clone(),
                repo: s.repo.clone(),
                sources: s.sources.clone(),
                fetch_enabled: s.fetch_enabled,
            })
            .map_err(json_err)?
        };
        self.save(REFDATA, value)
    }

    /// Saves the update-check switch.
    pub(crate) fn persist_updates(&self) -> Result<(), CoreError> {
        let enabled = self.update_status().enabled;
        self.save(UPDATES, Value::Bool(enabled))
    }

    /// Saves what advisors filed and what was dismissed.
    pub(crate) fn persist_inbox(&self) -> Result<(), CoreError> {
        let value = serde_json::to_value(&*self.inbox()).map_err(json_err)?;
        self.save(INBOX, value)
    }

    /// Saves a verified pack update, to use from the next opening.
    pub(crate) fn persist_pack(&self, pack: &SavedPack) -> Result<(), CoreError> {
        self.save(PACK, serde_json::to_value(pack).map_err(json_err)?)
    }

    /// Reads back what [`Core::persist_bank`] and the others saved.
    pub(crate) fn restore_state(&self) -> Result<(), CoreError> {
        let (bank, policies, refdata, updates, inbox) = {
            let db = self.db();
            (
                load(&db, BANK)?,
                load(&db, POLICIES)?,
                load(&db, REFDATA)?,
                load(&db, UPDATES)?,
                load(&db, INBOX)?,
            )
        };
        if let Some(v) = inbox {
            *self.inbox() = serde_json::from_value(v).map_err(json_err)?;
        }
        if let Some(v) = bank {
            *self.bank_state() = serde_json::from_value(v).map_err(json_err)?;
        }
        if let Some(v) = policies {
            let saved: std::collections::BTreeMap<String, skyla_egress::Policy> =
                serde_json::from_value(v).map_err(json_err)?;
            if let Ok(mut map) = self.egress_policies.lock() {
                for (task, policy) in saved {
                    // Only tasks this build knows; a removed one is dropped.
                    if let Ok(t) = super::egress::task(&task) {
                        map.insert(t.id, policy);
                    }
                }
            }
        }
        if let Some(v) = refdata {
            let saved: SavedRefData = serde_json::from_value(v).map_err(json_err)?;
            let mut s = self.refdata_state();
            s.fx = saved.fx;
            s.repo = saved.repo;
            s.sources = saved.sources;
            s.fetch_enabled = saved.fetch_enabled;
        }
        if let Some(Value::Bool(enabled)) = updates {
            self.set_update_check(enabled);
        }
        Ok(())
    }
}
