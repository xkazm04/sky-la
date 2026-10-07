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
pub const SCHEMA: &[SchemaStep] = &[SchemaStep {
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
}];

/// Runs every [`SCHEMA`] step directly, without migration bookkeeping. For
/// tests and tools; the application applies `SCHEMA` through its migrations.
pub fn apply_schema(conn: &Connection) -> Result<(), LedgerError> {
    for step in SCHEMA {
        conn.execute_batch(step.sql)?;
    }
    Ok(())
}
