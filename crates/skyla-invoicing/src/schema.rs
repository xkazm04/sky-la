use rusqlite::Connection;
use skyla_ledger::SchemaStep;

use crate::InvoicingError;

/// The invoicing schema, applied after the ledger's. Issued documents are
/// frozen by triggers, so even raw SQL can't edit or delete them.
pub const SCHEMA: &[SchemaStep] = &[SchemaStep {
    name: "invoicing_documents",
    sql: r#"
CREATE TABLE doc_series (
    code        TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('invoice', 'credit_note', 'advance', 'advance_tax')),
    pattern     TEXT NOT NULL CHECK (instr(pattern, '{N') > 0),
    description TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE TABLE document (
    id               INTEGER PRIMARY KEY,
    uid              TEXT    NOT NULL UNIQUE,
    kind             TEXT    NOT NULL CHECK (kind IN ('invoice', 'credit_note', 'advance', 'advance_tax')),
    series           TEXT    NOT NULL REFERENCES doc_series (code),
    status           TEXT    NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'issued')),
    number           TEXT    UNIQUE,
    year             INTEGER,
    seq              INTEGER,
    issue_date       TEXT CHECK (issue_date IS NULL OR date(issue_date) IS issue_date),
    tax_point_date   TEXT CHECK (tax_point_date IS NULL OR date(tax_point_date) IS tax_point_date),
    due_date         TEXT CHECK (due_date IS NULL OR date(due_date) IS due_date),
    customer_name    TEXT    NOT NULL,
    customer_ico     TEXT,
    customer_dic     TEXT,
    customer_address TEXT,
    currency         TEXT    NOT NULL,
    note             TEXT    NOT NULL DEFAULT '',
    related_id       INTEGER REFERENCES document (id),
    entry_id         INTEGER UNIQUE,
    pack             TEXT,
    -- Imported from another program: numbered there, so gaps are reported, not refused.
    imported         INTEGER NOT NULL DEFAULT 0 CHECK (imported IN (0, 1)),
    UNIQUE (series, year, seq),
    CHECK ((status = 'issued') = (number IS NOT NULL AND issue_date IS NOT NULL AND seq IS NOT NULL AND pack IS NOT NULL))
) STRICT;

CREATE TABLE document_line (
    document_id      INTEGER NOT NULL REFERENCES document (id) ON DELETE CASCADE,
    line_no          INTEGER NOT NULL CHECK (line_no > 0),
    description      TEXT    NOT NULL CHECK (length(trim(description)) > 0),
    quantity         TEXT    NOT NULL,
    unit             TEXT    NOT NULL DEFAULT '',
    unit_price_minor INTEGER NOT NULL,
    vat_code         TEXT    NOT NULL,
    account          TEXT,
    PRIMARY KEY (document_id, line_no)
) STRICT;

-- The VAT recapitulation, fixed when the document is issued.
CREATE TABLE document_total (
    document_id INTEGER NOT NULL REFERENCES document (id),
    vat_code    TEXT    NOT NULL,
    rate        TEXT    NOT NULL,
    base_minor  INTEGER NOT NULL,
    vat_minor   INTEGER NOT NULL,
    PRIMARY KEY (document_id, vat_code)
) STRICT;

-- A final invoice deducts the advances it settles.
CREATE TABLE document_advance (
    document_id INTEGER NOT NULL REFERENCES document (id),
    advance_tax_id INTEGER NOT NULL UNIQUE REFERENCES document (id),
    PRIMARY KEY (document_id, advance_tax_id)
) STRICT;

CREATE TRIGGER document_issued_is_frozen BEFORE UPDATE ON document
WHEN OLD.status = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: an issued document is immutable; issue a credit note');
END;

CREATE TRIGGER document_issued_never_deleted BEFORE DELETE ON document
WHEN OLD.status = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: an issued document is never deleted');
END;

CREATE TRIGGER document_line_frozen_insert BEFORE INSERT ON document_line
WHEN (SELECT status FROM document WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: lines of an issued document are immutable');
END;
CREATE TRIGGER document_line_frozen_update BEFORE UPDATE ON document_line
WHEN (SELECT status FROM document WHERE id = OLD.document_id) IS NOT 'draft'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: lines of an issued document are immutable');
END;
CREATE TRIGGER document_line_frozen_delete BEFORE DELETE ON document_line
WHEN (SELECT status FROM document WHERE id = OLD.document_id) = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: lines of an issued document are immutable');
END;

CREATE TRIGGER document_total_frozen BEFORE UPDATE ON document_total
BEGIN
    SELECT RAISE(ABORT, 'invoicing: a document''s totals are fixed when it is issued');
END;
CREATE TRIGGER document_total_kept BEFORE DELETE ON document_total
BEGIN
    SELECT RAISE(ABORT, 'invoicing: a document''s totals are fixed when it is issued');
END;

-- Numbers are gapless per series and year.
CREATE TRIGGER document_number_gapless BEFORE UPDATE OF status ON document
WHEN OLD.status = 'draft' AND NEW.status = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'invoicing: numbers are gapless within a series and year')
    WHERE NEW.imported = 0 AND NEW.seq IS NOT (SELECT coalesce(max(seq), 0) + 1 FROM document
                          WHERE series = NEW.series AND year = NEW.year AND status = 'issued');
    SELECT RAISE(ABORT, 'invoicing: the series is for another kind of document')
    WHERE (SELECT kind FROM doc_series WHERE code = NEW.series) IS NOT NEW.kind;
END;
"#,
}];

/// Applies [`SCHEMA`] directly (tests and tools); the app migrates it.
pub fn apply_schema(conn: &Connection) -> Result<(), InvoicingError> {
    for step in SCHEMA {
        conn.execute_batch(step.sql)?;
    }
    Ok(())
}
