//! The hash chain over posted entries (D-007, on by default): tamper evidence
//! for the journal.
//!
//! Posting an entry stores `chain_hash = SHA-256(domain, previous hash,
//! canonical entry)`. Triggers keep posted rows frozen; the chain catches
//! what triggers can't, such as someone dropping a trigger and editing a
//! row, or deleting one. [`verify_chain`] walks the chain and names the first
//! entry that doesn't fit. Closing a period seals the chain head in the
//! period row, so cutting entries off the end is detected too.

use rusqlite::{Connection, ToSql, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::LedgerError;

const DOMAIN: &str = "skyla-ledger/journal-chain/v1";
const GENESIS: [u8; 32] = [0; 32];

/// What a posted entry commits to.
#[derive(Debug, Default)]
pub(crate) struct ChainRecord {
    pub posted_seq: i64,
    pub uid: String,
    pub entry_date: String,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub memo: String,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub posted_at: String,
    pub reverses_uid: Option<String>,
    pub lines: Vec<ChainLine>,
    /// Settlement links: (settled entry uid, functional amount), by uid.
    pub settlements: Vec<(String, i64)>,
}

#[derive(Debug)]
pub(crate) struct ChainLine {
    pub line_no: i64,
    pub account: String,
    pub amount_minor: i64,
    pub currency: String,
    pub amount_func_minor: i64,
    pub fx_rate: Option<String>,
    pub vat_code: Option<String>,
    pub tax_treatment: Option<String>,
    pub memo: String,
}

/// Length-prefixed, unambiguous field encoding into the hasher.
struct Encoder(Sha256);

impl Encoder {
    fn int(&mut self, value: i64) {
        self.0.update(value.to_be_bytes());
    }
    fn text(&mut self, value: &str) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value.as_bytes());
    }
    fn opt(&mut self, value: Option<&str>) {
        match value {
            None => self.0.update([0]),
            Some(text) => {
                self.0.update([1]);
                self.text(text);
            }
        }
    }
}

/// The next link: `SHA-256(domain ‖ previous ‖ record)`.
pub(crate) fn link(previous: &[u8; 32], record: &ChainRecord) -> [u8; 32] {
    let mut e = Encoder(Sha256::new());
    e.text(DOMAIN);
    e.0.update(previous);
    e.int(record.posted_seq);
    e.text(&record.uid);
    e.text(&record.entry_date);
    e.text(&record.source_kind);
    e.opt(record.source_ref.as_deref());
    e.text(&record.memo);
    e.text(&record.created_by);
    e.opt(record.approved_by.as_deref());
    e.text(&record.posted_at);
    e.opt(record.reverses_uid.as_deref());
    e.int(record.lines.len() as i64);
    for line in &record.lines {
        e.int(line.line_no);
        e.text(&line.account);
        e.int(line.amount_minor);
        e.text(&line.currency);
        e.int(line.amount_func_minor);
        e.opt(line.fx_rate.as_deref());
        e.opt(line.vat_code.as_deref());
        e.opt(line.tax_treatment.as_deref());
        e.text(&line.memo);
    }
    e.int(record.settlements.len() as i64);
    for (uid, amount) in &record.settlements {
        e.text(uid);
        e.int(*amount);
    }
    e.0.finalize().into()
}

pub(crate) fn to_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

const RECORD_SQL: &str = "
SELECT e.id, e.posted_seq, e.uid, e.entry_date, e.source_kind, e.source_ref, e.memo, e.created_by,
       e.approved_by, e.posted_at, r.uid, e.chain_hash,
       p.line_no, a.code, p.amount_minor, p.currency, p.amount_func_minor, p.fx_rate, p.vat_code,
       p.tax_treatment, p.memo
FROM journal_entry e
LEFT JOIN journal_entry r ON r.id = e.reverses_id
LEFT JOIN posting p ON p.entry_id = e.id
LEFT JOIN account a ON a.id = p.account_id";

/// Streams records matching `filter` (ordered by `order`), one entry at a
/// time, with the stored hash. `visit` returns false to stop.
fn for_each_record(
    conn: &Connection,
    filter: &str,
    order: &str,
    args: &[&dyn ToSql],
    mut visit: impl FnMut(ChainRecord, Option<String>) -> Result<bool, LedgerError>,
) -> Result<(), LedgerError> {
    let mut stmt = conn.prepare(&format!(
        "{RECORD_SQL} WHERE {filter} ORDER BY {order}, p.line_no"
    ))?;
    let mut links = conn.prepare_cached(
        "SELECT se.uid, s.amount_func_minor FROM settlement s
         JOIN journal_entry se ON se.id = s.settled_entry_id
         WHERE s.cash_entry_id = ?1 ORDER BY se.uid",
    )?;
    let mut emit = |id: i64, mut record: ChainRecord, hash: Option<String>| {
        record.settlements = links
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        visit(record, hash)
    };
    let mut rows = stmt.query(args)?;
    let mut current: Option<(i64, ChainRecord, Option<String>)> = None;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        if current.as_ref().is_none_or(|(cid, _, _)| *cid != id) {
            if let Some((done, record, hash)) = current.take()
                && !emit(done, record, hash)?
            {
                return Ok(());
            }
            let record = ChainRecord {
                posted_seq: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                uid: row.get(2)?,
                entry_date: row.get(3)?,
                source_kind: row.get(4)?,
                source_ref: row.get(5)?,
                memo: row.get(6)?,
                created_by: row.get(7)?,
                approved_by: row.get(8)?,
                posted_at: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                reverses_uid: row.get(10)?,
                lines: Vec::new(),
                settlements: Vec::new(),
            };
            current = Some((id, record, row.get(11)?));
        }
        if let (Some((_, record, _)), Some(line_no)) =
            (current.as_mut(), row.get::<_, Option<i64>>(12)?)
        {
            record.lines.push(ChainLine {
                line_no,
                account: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
                amount_minor: row.get(14)?,
                currency: row.get(15)?,
                amount_func_minor: row.get(16)?,
                fx_rate: row.get(17)?,
                vat_code: row.get(18)?,
                tax_treatment: row.get(19)?,
                memo: row.get(20)?,
            });
        }
    }
    if let Some((done, record, hash)) = current {
        emit(done, record, hash)?;
    }
    Ok(())
}

/// The hash a draft gets when posted as `seq` at `posted_at`. Called inside
/// the posting transaction.
pub(crate) fn hash_for_post(
    conn: &Connection,
    entry_id: i64,
    seq: i64,
    posted_at: &str,
    approved_by: Option<&str>,
) -> Result<String, LedgerError> {
    let previous = if seq == 1 {
        GENESIS
    } else {
        let stored: Option<String> = conn.query_row(
            "SELECT chain_hash FROM journal_entry WHERE posted_seq = ?1",
            [seq - 1],
            |r| r.get(0),
        )?;
        stored.as_deref().and_then(from_hex).ok_or_else(|| {
            LedgerError::ChainCorrupt(format!("posted entry {} has no valid chain hash", seq - 1))
        })?
    };
    let mut found = None;
    for_each_record(conn, "e.id = ?1", "e.id", &[&entry_id], |record, _| {
        found = Some(record);
        Ok(false)
    })?;
    let mut record = found.ok_or(LedgerError::EntryNotFound(entry_id))?;
    record.posted_seq = seq;
    record.posted_at = posted_at.to_owned();
    record.approved_by = approved_by.map(str::to_owned);
    Ok(to_hex(&link(&previous, &record)))
}

/// What's wrong at the first break in the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainBreakKind {
    /// A posted entry with this sequence number is missing (deleted).
    Missing,
    /// The entry, its lines or its stored hash changed after posting.
    Altered,
    /// A closed period's sealed chain head doesn't match the journal
    /// (entries were cut off the end, or the chain was rebuilt).
    SealMismatch,
}

/// The first entry that doesn't fit the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainBreak {
    /// Sequence number where the chain breaks.
    pub posted_seq: i64,
    /// The entry found there, if any.
    pub entry_uid: Option<String>,
    /// What's wrong.
    pub kind: ChainBreakKind,
}

impl std::fmt::Display for ChainBreak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.kind {
            ChainBreakKind::Missing => "is missing",
            ChainBreakKind::Altered => "changed after it was posted",
            ChainBreakKind::SealMismatch => {
                "doesn't match the chain head sealed when a period closed"
            }
        };
        match &self.entry_uid {
            Some(uid) => write!(f, "posted entry {} ({uid}) {what}", self.posted_seq),
            None => write!(f, "posted entry {} {what}", self.posted_seq),
        }
    }
}

/// The result of walking the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainReport {
    /// Posted entries checked before the first break (or all of them).
    pub entries_checked: u64,
    /// The verified chain head (hex), if anything is posted and intact.
    pub head: Option<String>,
    /// The first break, if any.
    pub first_break: Option<ChainBreak>,
}

impl ChainReport {
    /// True when every posted entry fits the chain.
    pub fn is_intact(&self) -> bool {
        self.first_break.is_none()
    }
}

/// Recomputes the chain over every posted entry, in sequence order, and checks
/// each closed period's seal. Run on open and before export.
pub fn verify_chain(conn: &Connection) -> Result<ChainReport, LedgerError> {
    let mut seals: Vec<(i64, String)> = conn
        .prepare(
            "SELECT chain_seq_at_close, chain_head_at_close FROM period
             WHERE state = 'closed' AND chain_seq_at_close IS NOT NULL ORDER BY chain_seq_at_close",
        )?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?
        .collect::<Result<_, _>>()?;
    seals.reverse(); // pop from the front in sequence order

    let mut previous = GENESIS;
    let mut expected = 1_i64;
    let mut checked = 0_u64;
    let mut first_break = None;
    for_each_record(
        conn,
        "e.status = 'posted'",
        "e.posted_seq",
        &[],
        |record, stored| {
            if record.posted_seq != expected {
                first_break = Some(ChainBreak {
                    posted_seq: expected,
                    entry_uid: None,
                    kind: ChainBreakKind::Missing,
                });
                return Ok(false);
            }
            let computed = link(&previous, &record);
            if stored.as_deref().and_then(from_hex) != Some(computed) {
                first_break = Some(ChainBreak {
                    posted_seq: record.posted_seq,
                    entry_uid: Some(record.uid),
                    kind: ChainBreakKind::Altered,
                });
                return Ok(false);
            }
            while let Some((seq, head)) = seals.last() {
                if *seq != record.posted_seq {
                    break;
                }
                if from_hex(head) != Some(computed) {
                    first_break = Some(ChainBreak {
                        posted_seq: record.posted_seq,
                        entry_uid: Some(record.uid.clone()),
                        kind: ChainBreakKind::SealMismatch,
                    });
                    return Ok(false);
                }
                seals.pop();
            }
            previous = computed;
            expected += 1;
            checked += 1;
            Ok(true)
        },
    )?;
    // A seal beyond the last entry means entries were cut off the end.
    if first_break.is_none()
        && let Some((seq, _)) = seals.last()
    {
        first_break = Some(ChainBreak {
            posted_seq: *seq,
            entry_uid: None,
            kind: ChainBreakKind::SealMismatch,
        });
    }
    Ok(ChainReport {
        entries_checked: checked,
        head: (checked > 0 && first_break.is_none()).then(|| to_hex(&previous)),
        first_break,
    })
}

/// The current chain head: highest sequence number and its stored hash.
pub(crate) fn stored_head(conn: &Connection) -> Result<(Option<i64>, Option<String>), LedgerError> {
    Ok(conn.query_row(
        "SELECT posted_seq, chain_hash FROM journal_entry WHERE posted_seq = (SELECT max(posted_seq) FROM journal_entry)
         UNION ALL SELECT NULL, NULL LIMIT 1",
        params![],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}
