use rusqlite::Connection;

use crate::LedgerError;

/// One step of the ledger schema. The application numbers these into its
/// migration list (see `skyla-store`); the order never changes.
#[derive(Debug, Clone, Copy)]
pub struct SchemaStep {
    /// Short name for the migration log.
    pub name: &'static str,
    /// SQL to run once, in one transaction.
    pub sql: &'static str,
}

/// The ledger schema, in order. Rules live in triggers as well as in Rust, so
/// they hold even for raw SQL. Every trigger message starts with `ledger: `.
pub const SCHEMA: &[SchemaStep] = &[
    SchemaStep {
        name: "ledger_accounts_categories_periods",
        sql: r#"
CREATE TABLE account (
    id          INTEGER PRIMARY KEY,
    code        TEXT    NOT NULL UNIQUE,
    name_cs     TEXT    NOT NULL CHECK (length(trim(name_cs)) > 0),
    name_en     TEXT    NOT NULL CHECK (length(trim(name_en)) > 0),
    kind        TEXT    NOT NULL CHECK (kind IN ('asset', 'liability', 'equity', 'revenue', 'expense', 'closing')),
    normal_side TEXT    NOT NULL CHECK (normal_side IN ('debit', 'credit')),
    contra      INTEGER NOT NULL DEFAULT 0 CHECK (contra IN (0, 1)),
    parent_id   INTEGER REFERENCES account (id),
    active      INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    CHECK (parent_id IS NULL OR parent_id <> id)
) STRICT;
CREATE INDEX account_parent ON account (parent_id);

-- A leaf has no sub-accounts; only leaves can be posted to.
CREATE VIEW account_with_leaf AS
SELECT a.*, NOT EXISTS (SELECT 1 FROM account c WHERE c.parent_id = a.id) AS is_leaf
FROM account a;

CREATE TRIGGER account_sub_matches_parent BEFORE INSERT ON account
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ledger: a sub-account must have its parent''s kind and normal side')
    WHERE NOT EXISTS (SELECT 1 FROM account p
                      WHERE p.id = NEW.parent_id AND p.kind = NEW.kind
                        AND p.normal_side = NEW.normal_side AND p.contra = NEW.contra);
    SELECT RAISE(ABORT, 'ledger: a sub-account code must be its parent''s code, a dot and a suffix')
    WHERE NOT EXISTS (SELECT 1 FROM account p
                      WHERE p.id = NEW.parent_id
                        AND substr(NEW.code, 1, length(p.code) + 1) = p.code || '.'
                        AND length(NEW.code) > length(p.code) + 1
                        AND instr(substr(NEW.code, length(p.code) + 2), '.') = 0);
    SELECT RAISE(ABORT, 'ledger: a category posts to this account; give it a sub-account only after moving the category')
    WHERE EXISTS (SELECT 1 FROM category WHERE account_id = NEW.parent_id);
END;

CREATE TRIGGER account_top_level_code BEFORE INSERT ON account
WHEN NEW.parent_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'ledger: a top-level account code is exactly three digits')
    WHERE NEW.code NOT GLOB '[0-9][0-9][0-9]';
END;

CREATE TRIGGER account_structure_fixed BEFORE UPDATE OF code, kind, normal_side, contra, parent_id ON account
BEGIN
    SELECT RAISE(ABORT, 'ledger: an account''s code, kind, side and parent can''t change; deactivate it instead');
END;

CREATE TRIGGER account_never_deleted BEFORE DELETE ON account
BEGIN
    SELECT RAISE(ABORT, 'ledger: accounts are deactivated, never deleted');
END;

CREATE TABLE category (
    id            INTEGER PRIMARY KEY,
    key           TEXT    NOT NULL UNIQUE,
    name_cs       TEXT    NOT NULL,
    name_en       TEXT    NOT NULL,
    direction     TEXT    NOT NULL CHECK (direction IN ('income', 'expense', 'owner')),
    account_id    INTEGER NOT NULL REFERENCES account (id),
    tax_treatment TEXT    NOT NULL CHECK (tax_treatment IN ('taxable', 'exempt', 'deductible', 'non_deductible', 'not_tax_relevant'))
) STRICT;

CREATE TRIGGER category_targets_leaf BEFORE INSERT ON category
BEGIN
    SELECT RAISE(ABORT, 'ledger: a category must post to an account without sub-accounts')
    WHERE EXISTS (SELECT 1 FROM account WHERE parent_id = NEW.account_id);
END;

CREATE TABLE period (
    id        INTEGER PRIMARY KEY,
    starts_on TEXT NOT NULL CHECK (date(starts_on) IS starts_on),
    ends_on   TEXT NOT NULL CHECK (date(ends_on) IS ends_on),
    state     TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closing', 'closed')),
    CHECK (ends_on >= starts_on)
) STRICT;

CREATE TRIGGER period_no_overlap BEFORE INSERT ON period
BEGIN
    SELECT RAISE(ABORT, 'ledger: periods must not overlap')
    WHERE EXISTS (SELECT 1 FROM period p WHERE NEW.starts_on <= p.ends_on AND NEW.ends_on >= p.starts_on);
END;

CREATE TRIGGER period_dates_fixed BEFORE UPDATE OF starts_on, ends_on ON period
BEGIN
    SELECT RAISE(ABORT, 'ledger: a period''s dates can''t change');
END;
"#,
    },
    SchemaStep {
        name: "ledger_journal",
        sql: JOURNAL_SQL,
    },
    SchemaStep {
        name: "ledger_close_reversal_chain",
        sql: CLOSE_SQL,
    },
];

/// Step 2: the journal. Invariants I1–I4 and I7 are re-checked here on every
/// draft → posted transition, and posted rows are frozen (I2).
const JOURNAL_SQL: &str = r#"
CREATE TABLE ledger_setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TRIGGER functional_currency_fixed_once_used BEFORE UPDATE ON ledger_setting
WHEN OLD.key = 'functional_currency' AND EXISTS (SELECT 1 FROM posting)
BEGIN
    SELECT RAISE(ABORT, 'ledger: the functional currency can''t change once anything is booked');
END;

CREATE TABLE journal_entry (
    id          INTEGER PRIMARY KEY,
    uid         TEXT    NOT NULL UNIQUE,
    entry_date  TEXT    NOT NULL CHECK (date(entry_date) IS entry_date),
    status      TEXT    NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'posted')),
    source_kind TEXT    NOT NULL CHECK (source_kind IN ('manual', 'invoice', 'bank', 'rule', 'advisor', 'reversal', 'opening')),
    source_ref  TEXT,
    memo        TEXT    NOT NULL DEFAULT '',
    created_by  TEXT    NOT NULL CHECK (length(created_by) > 0),
    approved_by TEXT,
    period_id   INTEGER REFERENCES period (id),
    posted_at   TEXT,
    posted_seq  INTEGER UNIQUE,
    reverses_id INTEGER UNIQUE REFERENCES journal_entry (id),
    chain_hash  TEXT,
    CHECK ((status = 'posted') = (posted_seq IS NOT NULL AND posted_at IS NOT NULL AND period_id IS NOT NULL))
) STRICT;
CREATE INDEX journal_entry_date ON journal_entry (entry_date);

CREATE TABLE posting (
    id                INTEGER PRIMARY KEY,
    entry_id          INTEGER NOT NULL REFERENCES journal_entry (id) ON DELETE CASCADE,
    line_no           INTEGER NOT NULL CHECK (line_no > 0),
    account_id        INTEGER NOT NULL REFERENCES account (id),
    amount_minor      INTEGER NOT NULL CHECK (amount_minor <> 0),
    currency          TEXT    NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    amount_func_minor INTEGER NOT NULL CHECK (amount_func_minor <> 0),
    fx_rate           TEXT,
    vat_code          TEXT,
    tax_treatment     TEXT CHECK (tax_treatment IN ('taxable', 'exempt', 'deductible', 'non_deductible', 'not_tax_relevant')),
    memo              TEXT    NOT NULL DEFAULT '',
    UNIQUE (entry_id, line_no),
    CHECK ((amount_minor > 0) = (amount_func_minor > 0))
) STRICT;
CREATE INDEX posting_account ON posting (account_id);

-- I2: only drafts can gain, change or lose postings.
CREATE TRIGGER posting_insert_only_on_draft BEFORE INSERT ON posting
WHEN (SELECT status FROM journal_entry WHERE id = NEW.entry_id) IS NOT 'draft'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a posted entry can''t gain postings');
END;
CREATE TRIGGER posting_update_only_on_draft BEFORE UPDATE ON posting
WHEN (SELECT status FROM journal_entry WHERE id = OLD.entry_id) IS NOT 'draft'
  OR (SELECT status FROM journal_entry WHERE id = NEW.entry_id) IS NOT 'draft'
BEGIN
    SELECT RAISE(ABORT, 'ledger: postings of a posted entry are immutable');
END;
CREATE TRIGGER posting_delete_only_on_draft BEFORE DELETE ON posting
WHEN (SELECT status FROM journal_entry WHERE id = OLD.entry_id) = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'ledger: postings of a posted entry are immutable');
END;

-- I4: postings target active accounts without sub-accounts.
CREATE TRIGGER posting_targets_active_leaf BEFORE INSERT ON posting
BEGIN
    SELECT RAISE(ABORT, 'ledger: postings must target an account without sub-accounts')
    WHERE EXISTS (SELECT 1 FROM account WHERE parent_id = NEW.account_id);
    SELECT RAISE(ABORT, 'ledger: postings must target an active account')
    WHERE (SELECT active FROM account WHERE id = NEW.account_id) IS NOT 1;
END;

-- Functional-currency lines carry no FX; foreign lines must say which rate converted them.
CREATE TRIGGER posting_fx_consistent BEFORE INSERT ON posting
BEGIN
    SELECT RAISE(ABORT, 'ledger: the functional currency isn''t set')
    WHERE NOT EXISTS (SELECT 1 FROM ledger_setting WHERE key = 'functional_currency');
    SELECT RAISE(ABORT, 'ledger: a functional-currency line has no FX rate and equal amounts')
    WHERE NEW.currency = (SELECT value FROM ledger_setting WHERE key = 'functional_currency')
      AND (NEW.fx_rate IS NOT NULL OR NEW.amount_minor <> NEW.amount_func_minor);
    SELECT RAISE(ABORT, 'ledger: a foreign-currency line needs its FX rate')
    WHERE NEW.currency <> (SELECT value FROM ledger_setting WHERE key = 'functional_currency')
      AND NEW.fx_rate IS NULL;
END;

-- The only allowed change to a journal entry after creation is draft edits and
-- draft → posted. Posted entries are frozen (I2).
CREATE TRIGGER journal_entry_posted_is_frozen BEFORE UPDATE ON journal_entry
WHEN OLD.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a posted entry is immutable; correct it with a reversal');
END;

CREATE TRIGGER journal_entry_post_checks BEFORE UPDATE OF status ON journal_entry
WHEN OLD.status = 'draft' AND NEW.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'ledger: an entry needs at least two postings')
    WHERE (SELECT count(*) FROM posting WHERE entry_id = NEW.id) < 2;
    -- I1
    SELECT RAISE(ABORT, 'ledger: debits and credits must balance in the functional currency')
    WHERE (SELECT coalesce(sum(amount_func_minor), 0) FROM posting WHERE entry_id = NEW.id) <> 0;
    -- I3
    SELECT RAISE(ABORT, 'ledger: no open period covers the entry date')
    WHERE NOT EXISTS (SELECT 1 FROM period WHERE id = NEW.period_id AND state <> 'closed'
                        AND NEW.entry_date BETWEEN starts_on AND ends_on);
    -- I4 again: accounts may have changed since the draft's lines were written.
    SELECT RAISE(ABORT, 'ledger: every posting must target an active account without sub-accounts')
    WHERE EXISTS (SELECT 1 FROM posting p JOIN account_with_leaf a ON a.id = p.account_id
                  WHERE p.entry_id = NEW.id AND (a.is_leaf = 0 OR a.active = 0));
    -- I7
    SELECT RAISE(ABORT, 'ledger: entries proposed by a rule or the advisor need a human approver')
    WHERE NEW.source_kind IN ('rule', 'advisor') AND (NEW.approved_by IS NULL OR length(NEW.approved_by) = 0);
    SELECT RAISE(ABORT, 'ledger: posting sequence numbers are gapless')
    WHERE NEW.posted_seq IS NOT (SELECT coalesce(max(posted_seq), 0) + 1 FROM journal_entry);
END;

CREATE TRIGGER journal_entry_insert_as_draft BEFORE INSERT ON journal_entry
WHEN NEW.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'ledger: entries start as drafts');
END;

CREATE TRIGGER journal_entry_posted_never_deleted BEFORE DELETE ON journal_entry
WHEN OLD.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a posted entry is never deleted; reverse it instead');
END;

-- Accounts with postings can't gain sub-accounts (their history would sit on a group).
CREATE TRIGGER account_with_postings_stays_leaf BEFORE INSERT ON account
WHEN NEW.parent_id IS NOT NULL AND EXISTS (SELECT 1 FROM posting WHERE account_id = NEW.parent_id)
BEGIN
    SELECT RAISE(ABORT, 'ledger: an account with postings can''t gain sub-accounts');
END;
"#;

/// Step 3: the period state machine, reversal links and the hash chain (WP-06).
const CLOSE_SQL: &str = r#"
ALTER TABLE period ADD COLUMN closed_at TEXT;
ALTER TABLE period ADD COLUMN closed_by TEXT;
ALTER TABLE period ADD COLUMN chain_seq_at_close INTEGER;
ALTER TABLE period ADD COLUMN chain_head_at_close TEXT;

CREATE TRIGGER period_starts_open BEFORE INSERT ON period
WHEN NEW.state <> 'open' OR NEW.closed_at IS NOT NULL OR NEW.closed_by IS NOT NULL
  OR NEW.chain_seq_at_close IS NOT NULL OR NEW.chain_head_at_close IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ledger: a new period starts open');
END;

CREATE TRIGGER period_closed_is_final BEFORE UPDATE ON period
WHEN OLD.state = 'closed'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a closed period is final; book corrections in an open period');
END;

-- open → closing → closed; closing may step back to open.
CREATE TRIGGER period_state_machine BEFORE UPDATE ON period
WHEN OLD.state <> 'closed'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a period closes through closing: open → closing → closed')
    WHERE OLD.state = 'open' AND NEW.state = 'closed';
    SELECT RAISE(ABORT, 'ledger: close earlier periods first')
    WHERE NEW.state = 'closed'
      AND EXISTS (SELECT 1 FROM period p WHERE p.ends_on < OLD.starts_on AND p.state <> 'closed');
    SELECT RAISE(ABORT, 'ledger: a period with draft entries can''t close')
    WHERE NEW.state = 'closed'
      AND EXISTS (SELECT 1 FROM journal_entry WHERE status = 'draft' AND entry_date BETWEEN OLD.starts_on AND OLD.ends_on);
    SELECT RAISE(ABORT, 'ledger: closing records who closed the period, when, and the current chain head')
    WHERE NEW.state = 'closed'
      AND (NEW.closed_by IS NULL OR length(trim(NEW.closed_by)) = 0 OR NEW.closed_at IS NULL
           OR NEW.chain_seq_at_close IS NOT (SELECT max(posted_seq) FROM journal_entry)
           OR NEW.chain_head_at_close IS NOT (SELECT chain_hash FROM journal_entry WHERE posted_seq = NEW.chain_seq_at_close));
    SELECT RAISE(ABORT, 'ledger: close details are recorded only when a period closes')
    WHERE NEW.state <> 'closed'
      AND (NEW.closed_at IS NOT NULL OR NEW.closed_by IS NOT NULL
           OR NEW.chain_seq_at_close IS NOT NULL OR NEW.chain_head_at_close IS NOT NULL);
END;

CREATE TRIGGER period_kept_once_used BEFORE DELETE ON period
WHEN OLD.state = 'closed' OR EXISTS (SELECT 1 FROM journal_entry WHERE period_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'ledger: a period that holds postings or has closed is never deleted');
END;

-- Reversals: only a reversal references a reversed entry, and it must be posted.
CREATE TRIGGER journal_entry_reversal_link BEFORE INSERT ON journal_entry
BEGIN
    SELECT RAISE(ABORT, 'ledger: a reversal references the entry it reverses, and only a reversal does')
    WHERE (NEW.source_kind = 'reversal') <> (NEW.reverses_id IS NOT NULL);
    SELECT RAISE(ABORT, 'ledger: only a posted entry can be reversed')
    WHERE NEW.reverses_id IS NOT NULL
      AND (SELECT status FROM journal_entry WHERE id = NEW.reverses_id) IS NOT 'posted';
    SELECT RAISE(ABORT, 'ledger: a chain hash is set only when an entry is posted')
    WHERE NEW.chain_hash IS NOT NULL;
END;

CREATE TRIGGER journal_entry_origin_fixed BEFORE UPDATE OF source_kind, reverses_id ON journal_entry
WHEN OLD.source_kind IS NOT NEW.source_kind OR OLD.reverses_id IS NOT NEW.reverses_id
BEGIN
    SELECT RAISE(ABORT, 'ledger: an entry''s origin can''t change');
END;

CREATE TRIGGER journal_entry_reversal_mirrors BEFORE UPDATE OF status ON journal_entry
WHEN OLD.status = 'draft' AND NEW.status = 'posted' AND NEW.reverses_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ledger: a reversal mirrors every line of the reversed entry with the opposite sign')
    WHERE (SELECT count(*) FROM posting WHERE entry_id = NEW.id)
            <> (SELECT count(*) FROM posting WHERE entry_id = NEW.reverses_id)
       OR EXISTS (SELECT 1 FROM posting o WHERE o.entry_id = NEW.reverses_id AND NOT EXISTS (
              SELECT 1 FROM posting r
              WHERE r.entry_id = NEW.id AND r.line_no = o.line_no AND r.account_id = o.account_id
                AND r.currency = o.currency AND r.amount_minor = -o.amount_minor
                AND r.amount_func_minor = -o.amount_func_minor));
    SELECT RAISE(ABORT, 'ledger: a reversal can''t be dated before the entry it reverses')
    WHERE NEW.entry_date < (SELECT entry_date FROM journal_entry WHERE id = NEW.reverses_id);
END;

-- D-007: every posted entry carries its link in the hash chain, computed in
-- the posting transaction. SQLite can't hash, so the trigger checks the shape
-- and the verifier checks the value.
CREATE TRIGGER journal_entry_chain_on_post BEFORE UPDATE OF status, chain_hash ON journal_entry
WHEN OLD.status = 'draft'
BEGIN
    SELECT RAISE(ABORT, 'ledger: a posted entry carries its chain hash (64 lowercase hex digits)')
    WHERE NEW.status = 'posted'
      AND (NEW.chain_hash IS NULL OR length(NEW.chain_hash) <> 64 OR NEW.chain_hash GLOB '*[^0-9a-f]*');
    SELECT RAISE(ABORT, 'ledger: a chain hash is set only when an entry is posted')
    WHERE NEW.status = 'draft' AND NEW.chain_hash IS NOT NULL;
END;
"#;

/// Runs every [`SCHEMA`] step directly, without migration bookkeeping. For
/// tests and tools; the application applies `SCHEMA` through its migrations.
pub fn apply_schema(conn: &Connection) -> Result<(), LedgerError> {
    for step in SCHEMA {
        conn.execute_batch(step.sql)?;
    }
    Ok(())
}
