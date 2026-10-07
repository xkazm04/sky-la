//! Double-entry ledger kernel: accounts, journal entries, invariants, periods, the hash chain and projections.
//!
//! Jurisdiction-neutral: a chart of accounts arrives as data ([`ChartSpec`],
//! e.g. `rules/cz/chart.toml`) and the kernel validates and stores it. The
//! kernel owns its SQL ([`SCHEMA`]) and works on any `rusqlite` connection.
//! The application supplies an encrypted one from `skyla-store`.
//!
//! Money paths must never use floating point: `clippy::float_arithmetic` is denied here.

#![deny(clippy::float_arithmetic)]

mod accounts;
mod chain;
mod chart;
mod close;
mod error;
mod posting;
mod reports;
mod schema;

pub use accounts::{
    Account, Category, add_analytic_account, list_accounts, list_categories, open_period,
    seed_chart,
};
pub use chain::{ChainBreak, ChainBreakKind, ChainReport, verify_chain};
pub use chart::{
    AccountKind, AccountSpec, CategorySpec, ChartSpec, Direction, NormalSide, TaxTreatment,
};
pub use close::{
    ChainIntact, CheckResult, CloseCheck, CloseReport, EarlierPeriodsClosed, EntriesBalanced,
    NoDrafts, Period, PeriodState, STANDARD_CHECKS, begin_close, close_period, get_period,
    list_periods, reopen_period, run_close_checks,
};
pub use error::LedgerError;
pub use posting::{
    Entry, EntryStatus, Line, NewEntry, NewLine, Reversal, SourceKind, create_draft, delete_draft,
    functional_currency, get_entry, is_iso_date, link_settlement, post_entry, reverse_entry,
    set_functional_currency,
};
pub use reports::{
    BalanceSheet, CashBasis, CashBasisLine, CashBasisTotal, ProfitAndLoss, Snapshot, StatementLine,
    TrialBalance, TrialBalanceRow, VatCodeTotals, VatFormRow, VatLedger, VatPart, VatRowRule,
    balance_sheet, cash_basis, profit_and_loss, trial_balance, vat_ledger,
};
pub use schema::{SCHEMA, SchemaStep, apply_schema};
