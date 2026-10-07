//! WP-01: the recorded CLI transcripts parse, and the profile guard reads them correctly.

use skyla_advisor::cli_stream::{
    Init, StreamEvent, parse_line, parse_transcript, profile_violations,
};

fn load(name: &str) -> Vec<StreamEvent> {
    let text = match name {
        "c1" => include_str!("../fixtures/cli/c1_tools_schema.jsonl"),
        "c2" => include_str!("../fixtures/cli/c2_no_builtins.jsonl"),
        "c4" => include_str!("../fixtures/cli/c4_propose.jsonl"),
        _ => include_str!("../fixtures/cli/c7_sigint.jsonl"),
    };
    parse_transcript(text).expect("fixture parses")
}

fn init(events: &[StreamEvent]) -> &Init {
    events
        .iter()
        .find_map(|e| {
            if let StreamEvent::Init(i) = e {
                Some(i)
            } else {
                None
            }
        })
        .expect("init event")
}

fn result(events: &[StreamEvent]) -> &skyla_advisor::cli_stream::RunResult {
    events
        .iter()
        .rev()
        .find_map(|e| {
            if let StreamEvent::Result(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .expect("result event")
}

#[test]
fn structured_output_and_tool_results_come_through() {
    let events = load("c1");
    let init = init(&events);
    assert_eq!(
        init.tools,
        [
            "StructuredOutput",
            "mcp__skyla__get_period_summary",
            "mcp__skyla__propose_entry"
        ]
    );
    assert_eq!(init.mcp_servers.len(), 1);
    assert_eq!(init.permission_mode, "dontAsk");

    let calls: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let StreamEvent::ToolUse { name, .. } = e {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        calls,
        ["mcp__skyla__get_period_summary", "StructuredOutput"]
    );
    // The tool result is what left the machine: the aggregate figures.
    let sent = events
        .iter()
        .find_map(|e| {
            if let StreamEvent::ToolResult { content, .. } = e {
                Some(content.as_str())
            } else {
                None
            }
        })
        .unwrap();
    assert!(sent.contains("\"profit_minor\": 28035000"));

    let result = result(&events);
    assert_eq!(result.subtype, "success");
    assert!(!result.is_error);
    assert_eq!(
        result.structured_output.as_ref().unwrap()["profit_minor"],
        28_035_000
    );
    assert!(result.cost_estimate_usd.unwrap() > 0.0);
}

#[test]
fn without_builtins_the_model_can_only_say_no() {
    let events = load("c2");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUse { .. }))
    );
    assert_eq!(
        init(&events).tools,
        [
            "mcp__skyla__get_period_summary",
            "mcp__skyla__propose_entry"
        ]
    );
    assert!(result(&events).text.as_deref().unwrap().contains("can't"));
}

#[test]
fn a_proposal_arrives_as_a_tool_call_with_balanced_lines() {
    let events = load("c4");
    let input = events
        .iter()
        .find_map(|e| match e {
            StreamEvent::ToolUse { name, input, .. } if name == "mcp__skyla__propose_entry" => {
                Some(input)
            }
            _ => None,
        })
        .unwrap();
    let lines = input["lines"].as_array().unwrap();
    let total: i64 = lines
        .iter()
        .map(|l| l["amount_minor"].as_i64().unwrap())
        .sum();
    assert_eq!(lines.len(), 2);
    assert_eq!(total, 0);
}

#[test]
fn an_interrupted_run_still_ends_with_a_result() {
    let result_event = load("c7");
    let result = result(&result_event);
    assert_eq!(result.subtype, "error_during_execution");
    assert!(result.is_error);
}

#[test]
fn the_profile_guard_flags_anything_beyond_the_hardened_set() {
    // The fixtures were recorded before --disable-slash-commands was added, so
    // the guard rightly flags the loaded skills and commands; tools and servers pass.
    let violations = profile_violations(init(&load("c1")), "skyla");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("--disable-slash-commands"));

    let mut clean = init(&load("c1")).clone();
    clean.slash_commands = 0;
    clean.skills = 0;
    assert!(profile_violations(&clean, "skyla").is_empty());

    let mut leaky = clean.clone();
    leaky.tools.push("Bash".into());
    leaky
        .mcp_servers
        .push(skyla_advisor::cli_stream::McpServer {
            name: "github".into(),
            status: "connected".into(),
        });
    leaky.mcp_servers[0].status = "failed".into();
    let problems = profile_violations(&leaky, "skyla");
    assert!(problems.iter().any(|p| p == "unexpected tool Bash"));
    assert!(problems.iter().any(|p| p == "unexpected MCP server github"));
    assert!(problems.iter().any(|p| p == "MCP server skyla is failed"));
}

#[test]
fn unknown_and_incidental_events_are_tolerated() {
    assert_eq!(
        parse_line(r#"{"type":"rate_limit_event","rate_limit_info":{}}"#).unwrap(),
        vec![StreamEvent::Other]
    );
    assert_eq!(
        parse_line(r#"{"type":"something_new_in_2027"}"#).unwrap(),
        vec![StreamEvent::Other]
    );
    assert!(parse_line("").unwrap().is_empty());
    assert!(parse_line("not json").is_err());
    let retry =
        parse_line(r#"{"type":"system","subtype":"api_retry","attempt":2,"error":"rate_limit"}"#)
            .unwrap();
    assert_eq!(
        retry,
        vec![StreamEvent::ApiRetry {
            attempt: 2,
            error: "rate_limit".into()
        }]
    );
}
