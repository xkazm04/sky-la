//! Periods and the close: open → closing → closed, with pluggable close checks.
//!
//! A period in `closing` still takes postings (adjustments, accruals); a
//! `closed` one is final. Closing runs every check and refuses while any
//! fails. Checks that need other modules (bank tie-out, VAT ledger against
//! account 343) implement [`CloseCheck`] in those modules and are passed in.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::LedgerError;
use crate::chain::{stored_head, verify_chain};
use crate::posting::atomically;

/// Where a period is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodState {
    /// Takes postings.
    Open,
    /// Being closed: still takes postings; checks are being worked through.
    Closing,
    /// Final. Corrections go into an open period.
    Closed,
}

impl PeriodState {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closing => "closing",
            Self::Closed => "closed",
        }
    }

    fn from_db(index: usize, text: &str) -> rusqlite::Result<Self> {
        match text {
            "open" => Ok(Self::Open),
            "closing" => Ok(Self::Closing),
            "closed" => Ok(Self::Closed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                format!("unknown period state {other:?}").into(),
            )),
        }
    }
}

/// An accounting period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Period {
    /// Row id.
    pub id: i64,
    /// First day, `YYYY-MM-DD`.
    pub starts_on: String,
    /// Last day, inclusive.
    pub ends_on: String,
    /// Current state.
    pub state: PeriodState,
    /// When it closed.
    pub closed_at: Option<String>,
    /// Who closed it.
    pub closed_by: Option<String>,
}

const PERIOD_COLUMNS: &str = "id, starts_on, ends_on, state, closed_at, closed_by";

fn period_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Period> {
    Ok(Period {
        id: row.get(0)?,
        starts_on: row.get(1)?,
        ends_on: row.get(2)?,
        state: PeriodState::from_db(3, &row.get::<_, String>(3)?)?,
        closed_at: row.get(4)?,
        closed_by: row.get(5)?,
    })
}

/// Every period, oldest first.
pub fn list_periods(conn: &Connection) -> Result<Vec<Period>, LedgerError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PERIOD_COLUMNS} FROM period ORDER BY starts_on"
    ))?;
    let periods = stmt
        .query_map([], period_from_row)?
        .collect::<Result<_, _>>()?;
    Ok(periods)
}

/// One period.
pub fn get_period(conn: &Connection, id: i64) -> Result<Period, LedgerError> {
    conn.query_row(
        &format!("SELECT {PERIOD_COLUMNS} FROM period WHERE id = ?1"),
        [id],
        period_from_row,
    )
    .optional()?
    .ok_or(LedgerError::PeriodNotFound(id))
}

fn require_state(period: &Period, needed: PeriodState) -> Result<(), LedgerError> {
    if period.state == needed {
        Ok(())
    } else {
        Err(LedgerError::WrongPeriodState {
            id: period.id,
            is: period.state,
            needs: needed,
        })
    }
}

/// Starts closing an open period: open → closing.
pub fn begin_close(conn: &Connection, id: i64) -> Result<Period, LedgerError> {
    require_state(&get_period(conn, id)?, PeriodState::Open)?;
    conn.execute("UPDATE period SET state = 'closing' WHERE id = ?1", [id])?;
    get_period(conn, id)
}

/// Abandons a close in progress: closing → open.
pub fn reopen_period(conn: &Connection, id: i64) -> Result<Period, LedgerError> {
    require_state(&get_period(conn, id)?, PeriodState::Closing)?;
    conn.execute("UPDATE period SET state = 'open' WHERE id = ?1", [id])?;
    get_period(conn, id)
}

/// One check a period must pass before it closes.
pub trait CloseCheck {
    /// Stable key, e.g. `no_drafts`.
    fn key(&self) -> &'static str;
    /// Short human title.
    fn title(&self) -> &'static str;
    /// Runs the check; every returned string is a problem. Empty means it passed.
    fn run(&self, conn: &Connection, period: &Period) -> Result<Vec<String>, LedgerError>;
}

/// One check's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckResult {
    /// The check's key.
    pub key: &'static str,
    /// The check's title.
    pub title: &'static str,
    /// Problems found; empty when it passed.
    pub problems: Vec<String>,
}

impl CheckResult {
    /// True when nothing is wrong.
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Every check's outcome for one period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloseReport {
    /// The period checked.
    pub period_id: i64,
    /// One result per check, in the order given.
    pub results: Vec<CheckResult>,
}

impl CloseReport {
    /// True when every check passed.
    pub fn passed(&self) -> bool {
        self.results.iter().all(CheckResult::passed)
    }

    /// The checks that failed.
    pub fn failures(&self) -> impl Iterator<Item = &CheckResult> {
        self.results.iter().filter(|r| !r.passed())
    }
}

/// Runs `checks` against a period without changing anything.
pub fn run_close_checks(
    conn: &Connection,
    id: i64,
    checks: &[&dyn CloseCheck],
) -> Result<CloseReport, LedgerError> {
    let period = get_period(conn, id)?;
    let results = checks
        .iter()
        .map(|check| {
            Ok(CheckResult {
                key: check.key(),
                title: check.title(),
                problems: check.run(conn, &period)?,
            })
        })
        .collect::<Result<_, LedgerError>>()?;
    Ok(CloseReport {
        period_id: id,
        results,
    })
}

/// Closes a period that is `closing`: runs every check and refuses with
/// [`LedgerError::CloseBlocked`] while any fails. On success, records who
/// closed it and seals the current chain head into the period.
pub fn close_period(
    conn: &Connection,
    id: i64,
    checks: &[&dyn CloseCheck],
    closed_by: &str,
) -> Result<CloseReport, LedgerError> {
    if closed_by.trim().is_empty() {
        return Err(LedgerError::InvalidEntry(
            "closing a period records who closed it".into(),
        ));
    }
    require_state(&get_period(conn, id)?, PeriodState::Closing)?;
    atomically(conn, |tx| {
        let report = run_close_checks(tx, id, checks)?;
        if !report.passed() {
            return Err(LedgerError::CloseBlocked(Box::new(report)));
        }
        let (seq, head) = stored_head(tx)?;
        tx.execute(
            "UPDATE period SET state = 'closed', closed_by = ?2,
                 closed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                 chain_seq_at_close = ?3, chain_head_at_close = ?4
             WHERE id = ?1",
            params![id, closed_by, seq, head],
        )?;
        Ok(report)
    })
}

/// Earlier periods must be closed first.
#[derive(Debug, Clone, Copy)]
pub struct EarlierPeriodsClosed;

impl CloseCheck for EarlierPeriodsClosed {
    fn key(&self) -> &'static str {
        "earlier_periods_closed"
    }
    fn title(&self) -> &'static str {
        "Earlier periods are closed"
    }
    fn run(&self, conn: &Connection, period: &Period) -> Result<Vec<String>, LedgerError> {
        let mut stmt = conn.prepare(
            "SELECT starts_on, ends_on, state FROM period WHERE ends_on < ?1 AND state <> 'closed' ORDER BY starts_on",
        )?;
        let problems = stmt
            .query_map([&period.starts_on], |r| {
                Ok(format!(
                    "{} – {} is still {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(problems)
    }
}

/// No draft entries dated inside the period.
#[derive(Debug, Clone, Copy)]
pub struct NoDrafts;

impl CloseCheck for NoDrafts {
    fn key(&self) -> &'static str {
        "no_drafts"
    }
    fn title(&self) -> &'static str {
        "No draft entries"
    }
    fn run(&self, conn: &Connection, period: &Period) -> Result<Vec<String>, LedgerError> {
        let mut stmt = conn.prepare(
            "SELECT entry_date, memo, uid FROM journal_entry
             WHERE status = 'draft' AND entry_date BETWEEN ?1 AND ?2 ORDER BY entry_date, id",
        )?;
        let problems = stmt
            .query_map([&period.starts_on, &period.ends_on], |r| {
                let memo: String = r.get(1)?;
                let uid: String = r.get(2)?;
                let label = if memo.is_empty() { uid } else { memo };
                Ok(format!("draft dated {}: {label}", r.get::<_, String>(0)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(problems)
    }
}

/// Every posted entry in the period balances, and so does the period. Triggers
/// guarantee this; the check catches a database edited around them.
#[derive(Debug, Clone, Copy)]
pub struct EntriesBalanced;

impl CloseCheck for EntriesBalanced {
    fn key(&self) -> &'static str {
        "entries_balanced"
    }
    fn title(&self) -> &'static str {
        "Every entry balances"
    }
    fn run(&self, conn: &Connection, period: &Period) -> Result<Vec<String>, LedgerError> {
        let mut stmt = conn.prepare(
            "SELECT e.posted_seq, sum(p.amount_func_minor) FROM journal_entry e JOIN posting p ON p.entry_id = e.id
             WHERE e.status = 'posted' AND e.entry_date BETWEEN ?1 AND ?2
             GROUP BY e.id HAVING sum(p.amount_func_minor) <> 0 ORDER BY e.posted_seq",
        )?;
        let problems = stmt
            .query_map([&period.starts_on, &period.ends_on], |r| {
                Ok(format!(
                    "posted entry {} is off by {} minor units",
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(problems)
    }
}

/// The journal's hash chain verifies end to end.
#[derive(Debug, Clone, Copy)]
pub struct ChainIntact;

impl CloseCheck for ChainIntact {
    fn key(&self) -> &'static str {
        "chain_intact"
    }
    fn title(&self) -> &'static str {
        "Journal hash chain is intact"
    }
    fn run(&self, conn: &Connection, _period: &Period) -> Result<Vec<String>, LedgerError> {
        Ok(verify_chain(conn)?
            .first_break
            .map(|b| b.to_string())
            .into_iter()
            .collect())
    }
}

/// The kernel's own checks. Modules add theirs (bank tie-out, VAT ledger against 343).
pub const STANDARD_CHECKS: &[&dyn CloseCheck] = &[
    &EarlierPeriodsClosed,
    &NoDrafts,
    &EntriesBalanced,
    &ChainIntact,
];
