//! The provider contract every driver meets: what a run asks for, how it
//! ends, and whether the provider can run at all.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli_stream::StreamEvent;

/// What an advisor asks the model to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRequest {
    /// The advisor task, e.g. `tax.scenarios`; drivers and fakes key by it.
    pub task: String,
    /// The system prompt (replaces the provider's default).
    pub system_prompt: String,
    /// The user turn, already through the egress gate.
    pub prompt: String,
    /// JSON Schema the final answer must match.
    pub schema: Option<Value>,
    /// The MCP server config handed to the provider (the sky-la shim only).
    pub mcp_config: Option<Value>,
    /// The MCP server's name; its tools are the only ones allowed.
    pub server: String,
    /// The model the user chose; the provider's default when `None`.
    pub model: Option<String>,
    /// Reasoning effort for the task (`low`, `medium`, `high`).
    pub effort: Option<String>,
    /// Upper bound on agentic turns.
    pub max_turns: u32,
}

/// A tool call the model made, with what was sent back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool-use id.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Arguments.
    pub input: Value,
    /// The result returned to the model, once it came.
    pub result: Option<String>,
    /// The tool reported an error.
    pub is_error: bool,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "detail")]
pub enum RunStatus {
    /// The model answered.
    Completed,
    /// The provider isn't installed where we looked.
    NotInstalled,
    /// The provider needs the user to sign in (in their own terminal).
    NotSignedIn,
    /// Usage or rate limit reached; when it resets, if the provider said.
    RateLimited(Option<String>),
    /// The run started with tools or servers outside the hardened profile
    /// and was stopped before the model could use them.
    ProfileViolation(Vec<String>),
    /// Stopped by the user or a signal.
    Interrupted,
    /// Anything else, with what the provider said.
    Failed(String),
}

/// The run's outcome, every event folded in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutcome {
    /// How it ended.
    pub status: RunStatus,
    /// The model that ran, as reported.
    pub model: Option<String>,
    /// Final text.
    pub text: Option<String>,
    /// The schema-shaped answer.
    pub structured: Option<Value>,
    /// Every tool call, in order.
    pub tool_calls: Vec<ToolCall>,
    /// The provider's own cost estimate in USD. Shown for transparency
    /// ("about $0.002"); never booked.
    pub cost_estimate_usd: Option<f64>,
    /// Agentic turns used.
    pub turns: Option<u64>,
}

impl RunOutcome {
    /// An outcome with no events, only a status.
    pub fn with_status(status: RunStatus) -> Self {
        Self {
            status,
            model: None,
            text: None,
            structured: None,
            tool_calls: Vec::new(),
            cost_estimate_usd: None,
            turns: None,
        }
    }
}

/// Whether a provider can run now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Availability {
    /// Installed; signed in as far as the last run showed.
    Ready {
        /// What the provider reports about itself.
        version: String,
    },
    /// Not found.
    NotInstalled {
        /// Where we looked.
        looked_in: Vec<String>,
    },
    /// The last run said the user isn't signed in.
    NotSignedIn,
    /// The last run hit a usage or rate limit.
    RateLimited {
        /// When it resets, if known.
        resets_at: Option<String>,
    },
}

/// A model provider. Drivers never write to the ledger: a run returns text,
/// a structured answer and the tool calls it made; the core decides.
pub trait LlmProvider: Send + Sync {
    /// Short id, e.g. `claude-code-cli` or `fake`.
    fn id(&self) -> &str;
    /// Whether it can run now.
    fn availability(&self) -> Availability;
    /// Runs one request to its end. `on_event` sees every parsed event as
    /// it arrives (for progress and for the egress register).
    fn run(&self, request: &RunRequest, on_event: &mut dyn FnMut(&StreamEvent)) -> RunOutcome;
}

/// Folds a run's events into its outcome. Shared by every driver that
/// speaks Claude Code's stream format, so they classify endings alike.
pub fn fold(events: &[StreamEvent]) -> RunOutcome {
    let mut out =
        RunOutcome::with_status(RunStatus::Failed("the run ended without a result".into()));
    let mut last_retry: Option<String> = None;
    for e in events {
        match e {
            StreamEvent::Init(i) => out.model = Some(i.model.clone()),
            StreamEvent::ToolUse { id, name, input } => out.tool_calls.push(ToolCall {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
                result: None,
                is_error: false,
            }),
            StreamEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                if let Some(call) = out.tool_calls.iter_mut().find(|c| &c.id == tool_use_id) {
                    call.result = Some(content.clone());
                    call.is_error = *is_error;
                }
            }
            StreamEvent::ApiRetry { error, .. } => last_retry = Some(error.clone()),
            StreamEvent::Result(r) => {
                out.text = r.text.clone();
                out.structured = r.structured_output.clone();
                out.cost_estimate_usd = r.cost_estimate_usd;
                out.turns = r.num_turns;
                out.status = classify(
                    r.is_error,
                    &r.subtype,
                    r.text.as_deref(),
                    last_retry.as_deref(),
                );
            }
            StreamEvent::Text(_) | StreamEvent::Other => {}
        }
    }
    out
}

/// Usage-limit messages end with `|<unix seconds>` when the reset is known.
fn reset_time(text: &str) -> Option<String> {
    text.rsplit_once('|')
        .map(|(_, t)| t.trim())
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
}

/// Sorts a final result into a status. The sign-in wording comes from the
/// CLI's own error text; WP-01's check C6 (signed out on a desktop) will pin
/// the exact shape, so these markers stay deliberately broad.
pub fn classify(
    is_error: bool,
    subtype: &str,
    text: Option<&str>,
    last_retry: Option<&str>,
) -> RunStatus {
    if !is_error && subtype == "success" {
        return RunStatus::Completed;
    }
    let lower = text.unwrap_or_default().to_lowercase();
    const SIGN_IN: [&str; 5] = [
        "not logged in",
        "please run /login",
        "claude auth login",
        "invalid api key",
        "oauth token has expired",
    ];
    const LIMIT: [&str; 4] = ["usage limit", "rate limit", "rate_limit", "limit reached"];
    if SIGN_IN.iter().any(|m| lower.contains(m)) {
        return RunStatus::NotSignedIn;
    }
    if LIMIT.iter().any(|m| lower.contains(m)) || last_retry == Some("rate_limit") {
        return RunStatus::RateLimited(text.and_then(reset_time));
    }
    if subtype == "error_during_execution" && text.is_none() {
        return RunStatus::Interrupted;
    }
    RunStatus::Failed(match text {
        Some(t) if !t.is_empty() => t.to_owned(),
        _ => subtype.to_owned(),
    })
}
