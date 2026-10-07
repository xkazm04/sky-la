//! Double-entry ledger kernel: accounts, journal entries, invariants, periods and projections.
//!
//! Jurisdiction-neutral: a chart of accounts arrives as data ([`ChartSpec`],
//! e.g. `rules/cz/chart.toml`) and the kernel validates and stores it. The
//! kernel owns its SQL ([`SCHEMA`]) and works on any `rusqlite` connection.
//! The application supplies an encrypted one from `skyla-store`.
//!
//! Money paths must never use floating point: `clippy::float_arithmetic` is denied here.

#![deny(clippy::float_arithmetic)]

mod accounts;
mod chart;
mod error;
mod schema;

pub use accounts::{
    Account, Category, add_analytic_account, list_accounts, list_categories, open_period,
    seed_chart,
};
pub use chart::{
    AccountKind, AccountSpec, CategorySpec, ChartSpec, Direction, NormalSide, TaxTreatment,
};
pub use error::LedgerError;
pub use schema::{SCHEMA, SchemaStep, apply_schema};
