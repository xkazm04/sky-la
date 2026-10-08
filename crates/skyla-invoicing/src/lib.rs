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
//!
//! WP-13 adds the ISDOC 6.0.2 writer; WP-14 the EN 16931 writers: UBL 2.1
//! (Peppol BIS Billing 3.0) and CII D16B; WP-15 recurring templates, dunning
//! and statutory late interest.

#![deny(clippy::float_arithmetic)]

pub mod address;
pub mod cii;
mod documents;
pub mod dunning;
mod error;
mod exchange;
pub mod import;
pub mod isdoc;
pub mod late_interest;
pub mod recurring;
mod schema;
pub mod spayd;
mod supplier;
pub mod ubl;
mod xml;

pub use cii::to_cii;
pub use documents::{
    Accounts, Customer, DocKind, DocState, Document, DraftInput, IssueReplay, Issued, Line,
    LineInput, Settlement, Totals, VatRecap, compute_totals, create_draft, create_draft_as,
    customers, define_series, delete_draft, draft_credit_note, format_number, get, import_issued,
    issue, next_number, reductions, series_gaps, state, update_draft,
};
pub use error::InvoicingError;
pub use exchange::ExportInput;
pub use isdoc::to_isdoc;
pub use late_interest::{InterestPeriod, LateInterest, RepoRate, late_interest};
pub use schema::{SCHEMA, apply_schema};
pub use supplier::{Supplier, set_supplier, supplier, valid_ico};
pub use ubl::to_ubl;
