//! Egress gate: scopes, policies, pseudonymisation and the encrypted
//! register of everything sent to an LLM (WP-26).
//!
//! Every prompt and every MCP tool result passes the [`Gate`] before it
//! reaches the user's model: IBANs, account numbers, personal IDs and card
//! numbers are always withheld ([`redact`]), counterparty names become
//! stable [`Pseudonyms`] unless the task's scope grants names, and fields
//! outside the scope are dropped. Each run is appended to the [`register`],
//! with the exact bytes that were sent.

mod gate;
mod redact;
pub mod register;

pub use gate::{FieldClass, Gate, GateReport, Policy, Pseudonyms, Role};
pub use redact::{Withheld, redact};
