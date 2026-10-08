//! Application services behind the desktop shell's commands.
//!
//! The Tauri commands in `apps/desktop/src-tauri` are thin wrappers over
//! [`Core`]; everything they return is a DTO from [`dto`], exported to
//! TypeScript by tauri-specta. [`recordings`] captures the core's answers to a
//! set of canonical requests so the webview's mock transport replays exactly
//! what the real core returns (WP-09).

mod core;
pub mod demo;
pub mod dto;
mod error;
pub mod evals;
pub mod recordings;
pub mod session;

pub use crate::core::Core;
/// The financial advisor's detectors, for tests and tools.
pub use crate::core::findings as core_findings;
pub use crate::core::toolhost::{ToolCallLog, ToolRun};
pub use crate::core::tools::{ToolError, ToolKind, ToolSpec, tool_specs};
pub use error::{CoreError, IpcFailure};
pub use skyla_rules::Pack;
pub use skyla_store::KeyStore;
