//! Money in integer minor units, rates, rounding modes and exact allocation.
//!
//! Implemented in WP-02 (see `docs/plan/IMPLEMENTATION_PLAN.md`).
//!
//! Money paths must never use floating point: `clippy::float_arithmetic` is denied here.

#![deny(clippy::float_arithmetic)]
