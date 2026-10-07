//! Invoices and credit notes: number series, lifecycle, ISDOC, UBL and CII writers, SPAYD.
//!
//! WP-11 adds the document model and lifecycle: drafts change freely; issuing
//! assigns a gapless number, fixes the totals with the rule pack and posts
//! receivables, revenue and VAT through the ledger kernel in one transaction;
//! issued documents are frozen (triggers too). Credit notes settle the
//! invoice they correct, so open amounts and the cash basis stay right.
//! Advance invoices, the tax document on a received advance and the final
//! invoice's deduction follow Czech practice.
//!
//! WP-12 adds the supplier profile (snapshotted onto each document at issue)
//! and the SPAYD descriptor behind the QR Platba code.

#![deny(clippy::float_arithmetic)]

mod documents;
mod error;
mod schema;
pub mod spayd;
mod supplier;

pub use documents::{
    Accounts, Customer, DocKind, DocState, Document, DraftInput, IssueReplay, Issued, Line,
    LineInput, Settlement, Totals, VatRecap, compute_totals, create_draft, define_series,
    delete_draft, draft_credit_note, format_number, get, import_issued, issue, series_gaps, state,
    update_draft,
};
pub use error::InvoicingError;
pub use schema::{SCHEMA, apply_schema};
pub use supplier::{Supplier, set_supplier, supplier, valid_ico};
