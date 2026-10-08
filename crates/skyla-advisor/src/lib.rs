//! Advisor orchestration: the LlmProvider trait and the Claude Code CLI, Anthropic API and Fake drivers.
//!
//! Implemented in WP-24 to WP-29 (see `docs/plan/IMPLEMENTATION_PLAN.md`).
//! WP-01 added [`cli_stream`]: a parser for Claude Code's `stream-json`
//! output and the profile guard every driver applies. WP-24 added the
//! [`LlmProvider`] contract, the production [`ClaudeCodeCli`] driver and the
//! [`Fake`] driver that replays transcripts. WP-27 added [`grounding`], the
//! check that every figure in advisor prose is one the engine produced.

pub mod cli;
pub mod cli_stream;
mod fake;
pub mod grounding;
mod provider;

pub use cli::ClaudeCodeCli;
pub use fake::Fake;
pub use provider::{
    Availability, LlmProvider, RunOutcome, RunRequest, RunStatus, ToolCall, classify, fold,
};
