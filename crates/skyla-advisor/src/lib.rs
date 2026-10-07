//! Advisor orchestration: the LlmProvider trait and the Claude Code CLI, Anthropic API and Fake drivers.
//!
//! Implemented in WP-24 to WP-29 (see `docs/plan/IMPLEMENTATION_PLAN.md`).
//! WP-01 added [`cli_stream`]: a parser for Claude Code's `stream-json`
//! output and the profile guard the CLI driver applies to every run.

pub mod cli_stream;
