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
pub mod recordings;

pub use crate::core::Core;
pub use error::{CoreError, IpcFailure};
