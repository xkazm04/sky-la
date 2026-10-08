//! Czech tax computations, scenario-engine levers and EPO XML writers.
//!
//! WP-21 adds the kontrolní hlášení ([`control_statement`]); WP-22 the
//! § 7 [`worksheet`] and the scenario engine ([`scenarios`]). Rates,
//! rows and the itemisation threshold come from the rule pack. The EPO XML
//! writers wait for the official schemas (see `docs/plan/STATUS.md`).

#![deny(clippy::float_arithmetic)]

mod income;
mod kh;

pub use income::{
    Difference, Expenses, FlatRate, IncomeError, Insurance, PlannedPurchase, Scenario,
    ScenarioAnalysis, ScenarioFacts, Section7, Worksheet, scenarios, worksheet,
};
pub use kh::{
    ControlStatement, KhDocument, KhError, KhItem, KhPart, KhSide, KhTotals, control_statement,
};
