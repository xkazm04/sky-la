//! Double-entry ledger kernel: accounts, journal entries, invariants, periods and projections.
//!
//! Implemented in WP-04 to WP-07 (see `docs/plan/IMPLEMENTATION_PLAN.md`).
//!
//! Money paths must never use floating point: `clippy::float_arithmetic` is denied here.

#![deny(clippy::float_arithmetic)]
