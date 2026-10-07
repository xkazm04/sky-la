//! Parsing Claude Code's `--output-format stream-json --verbose` output.
//!
//! The format carries many incidental events (status, thinking-token
//! estimates, rate-limit info, partial-message deltas). Those parse as
//! [`StreamEvent::Other`] so a new event type never breaks a run; only the
//! events the advisor acts on are typed.

use serde::Deserialize;
use serde_json::Value;

/// One MCP server as reported in the `system/init` event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct McpServer {
    /// Server name from the MCP config.
    pub name: String,
    /// `connected`, `failed`, `pending`, …
    pub status: String,
}

/// The session's starting state: model, tools and servers actually loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Init {
    /// Resolved model id.
    pub model: String,
    /// Every tool the model can call.
    pub tools: Vec<String>,
    /// MCP servers in the session.
    pub mcp_servers: Vec<McpServer>,
    /// Permission mode in force.
    pub permission_mode: String,
    /// Claude Code version, when reported.
    pub claude_code_version: Option<String>,
    /// Number of slash commands loaded (0 with `--disable-slash-commands`).
    pub slash_commands: usize,
    /// Number of skills loaded (0 with `--disable-slash-commands`).
    pub skills: usize,
}

/// The final `result` event.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// `success`, `error_during_execution`, `error_max_turns`, …
    pub subtype: String,
    /// True when the run failed.
    pub is_error: bool,
    /// Final text, when the run produced one.
    pub text: Option<String>,
    /// The `--json-schema` output, when requested.
    pub structured_output: Option<Value>,
    /// Claude Code's own client-side cost estimate in USD. Shown to the user
    /// for transparency; it is never booked and isn't money in the ledger sense.
    pub cost_estimate_usd: Option<f64>,
    /// Agentic turns used.
    pub num_turns: Option<u64>,
    /// Tool calls the permission system denied.
    pub permission_denials: usize,
}

/// A parsed event. One stream line can yield several (an assistant message
/// with text and a tool call, for example).
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// `system/init`.
    Init(Init),
    /// The model called a tool.
    ToolUse {
        /// Tool-use id, matched by the later [`StreamEvent::ToolResult`].
        id: String,
        /// Tool name, e.g. `mcp__skyla__get_period_summary`.
        name: String,
        /// The arguments.
        input: Value,
    },
    /// A tool's result came back (this content was sent to the model).
    ToolResult {
        /// The tool-use id it answers.
        tool_use_id: String,
        /// Text content, joined.
        content: String,
        /// Whether the tool reported an error.
        is_error: bool,
    },
    /// Assistant text.
    Text(String),
    /// Claude Code is retrying an API call.
    ApiRetry {
        /// Attempt number.
        attempt: u64,
        /// Error category, e.g. `rate_limit`.
        error: String,
    },
    /// The final result.
    Result(RunResult),
    /// Anything else; tolerated and ignored.
    Other,
}

/// A line that isn't valid JSON or misses fields the typed events need.
#[derive(Debug, thiserror::Error)]
#[error("unparseable stream-json line: {0}")]
pub struct ParseError(String);

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Parses one line of stream-json output.
pub fn parse_line(line: &str) -> Result<Vec<StreamEvent>, ParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(line).map_err(|e| ParseError(e.to_string()))?;
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    let events = match (kind, subtype) {
        ("system", "init") => {
            let mcp_servers = match value.get("mcp_servers") {
                Some(servers) => serde_json::from_value(servers.clone())
                    .map_err(|e| ParseError(e.to_string()))?,
                None => Vec::new(),
            };
            let count = |key: &str| value.get(key).and_then(Value::as_array).map_or(0, Vec::len);
            vec![StreamEvent::Init(Init {
                model: str_field(&value, "model").unwrap_or_default(),
                tools: value
                    .get("tools")
                    .and_then(Value::as_array)
                    .map(|t| {
                        t.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                mcp_servers,
                permission_mode: str_field(&value, "permissionMode").unwrap_or_default(),
                claude_code_version: str_field(&value, "claude_code_version"),
                slash_commands: count("slash_commands"),
                skills: count("skills"),
            })]
        }
        ("system", "api_retry") => vec![StreamEvent::ApiRetry {
            attempt: value.get("attempt").and_then(Value::as_u64).unwrap_or(0),
            error: str_field(&value, "error").unwrap_or_default(),
        }],
        ("assistant" | "user", _) => {
            let blocks = value
                .pointer("/message/content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            blocks
                .iter()
                .filter_map(|block| match block.get("type").and_then(Value::as_str)? {
                    "tool_use" => Some(StreamEvent::ToolUse {
                        id: str_field(block, "id").unwrap_or_default(),
                        name: str_field(block, "name").unwrap_or_default(),
                        input: block.get("input").cloned().unwrap_or(Value::Null),
                    }),
                    "tool_result" => Some(StreamEvent::ToolResult {
                        tool_use_id: str_field(block, "tool_use_id").unwrap_or_default(),
                        content: block.get("content").map(text_of).unwrap_or_default(),
                        is_error: block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    }),
                    "text" if kind == "assistant" => Some(StreamEvent::Text(
                        str_field(block, "text").unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect()
        }
        ("result", _) => vec![StreamEvent::Result(RunResult {
            subtype: subtype.to_owned(),
            is_error: value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            text: str_field(&value, "result"),
            structured_output: value
                .get("structured_output")
                .cloned()
                .filter(|v| !v.is_null()),
            cost_estimate_usd: value.get("total_cost_usd").and_then(Value::as_f64),
            num_turns: value.get("num_turns").and_then(Value::as_u64),
            permission_denials: value
                .get("permission_denials")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        })],
        _ => vec![StreamEvent::Other],
    };
    Ok(events)
}

/// Parses a whole transcript.
pub fn parse_transcript(text: &str) -> Result<Vec<StreamEvent>, ParseError> {
    let mut events = Vec::new();
    for line in text.lines() {
        events.extend(parse_line(line)?);
    }
    Ok(events)
}

/// Built-in tool that `--json-schema` adds so the model can return the structured result.
pub const STRUCTURED_OUTPUT_TOOL: &str = "StructuredOutput";

/// Checks a run's `init` against the hardened profile: the only MCP server is
/// `server` (and it connected), every tool is one of its tools or the
/// structured-output tool, and no slash commands or skills are loaded.
/// Returns every violation; the CLI driver aborts the run on any.
pub fn profile_violations(init: &Init, server: &str) -> Vec<String> {
    let prefix = format!("mcp__{server}__");
    let mut problems = Vec::new();
    for tool in &init.tools {
        if tool != STRUCTURED_OUTPUT_TOOL && !tool.starts_with(&prefix) {
            problems.push(format!("unexpected tool {tool}"));
        }
    }
    for mcp in &init.mcp_servers {
        if mcp.name != server {
            problems.push(format!("unexpected MCP server {}", mcp.name));
        } else if mcp.status != "connected" {
            problems.push(format!("MCP server {} is {}", mcp.name, mcp.status));
        }
    }
    if !init.mcp_servers.iter().any(|m| m.name == server) {
        problems.push(format!("MCP server {server} is missing"));
    }
    if init.slash_commands > 0 || init.skills > 0 {
        problems.push(format!(
            "{} slash commands and {} skills loaded; pass --disable-slash-commands",
            init.slash_commands, init.skills
        ));
    }
    problems
}
