//! WP-25 acceptance: MCP messages reach the core only through the shim and
//! the run's token; each run rotates the token and the port.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use serde_json::{Value, json};
use skyla_app::Core;
use skyla_mcp::{HostBackend, serve};

fn mcp(messages: &[Value], addr: &str, token: &str) -> Vec<Value> {
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
    let mut out = Vec::new();
    let mut backend = HostBackend::connect(addr, token.to_owned()).unwrap();
    serve(input.as_bytes(), &mut out, &mut backend).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn a_run_lists_and_calls_tools_through_the_shim() {
    let core = Core::demo().unwrap();
    let (answers, calls) = core
        .with_tool_host(|run| {
            mcp(
                &[
                    json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
                    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                    json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
                    json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "get_rule_value", "arguments": { "key": "vat.rate.standard", "on": "2026-09-30" } } }),
                    json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "post_entry", "arguments": {} } }),
                ],
                &run.addr,
                &run.token,
            )
        })
        .unwrap();
    assert_eq!(answers.len(), 4, "the notification gets no answer");
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "skyla");
    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 10);
    let rate: Value =
        serde_json::from_str(answers[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(rate["value"], "21");
    assert!(rate["citation"].as_str().unwrap().contains("§ 47"));
    assert_eq!(answers[3]["result"]["isError"], true);
    assert_eq!(calls.len(), 2, "both calls are logged for the register");
    assert!(calls[1].refused && calls[1].result.contains("there is no tool post_entry"));
}

fn raw(addr: &str, token: &str) -> Option<Value> {
    let mut s = TcpStream::connect(addr).ok()?;
    writeln!(
        s,
        "{}",
        json!({ "token": token, "id": 1, "method": "list" })
    )
    .ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

#[test]
fn the_token_is_checked_and_rotates_per_run() {
    let core = Core::demo().unwrap();
    let ((first, refused), _) = core
        .with_tool_host(|run| (run.clone(), raw(&run.addr, "not-the-token").unwrap()))
        .unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"], "not authorised for this run");
    let ((second, old_token_here), _) = core
        .with_tool_host(|run| (run.clone(), raw(&run.addr, &first.token).unwrap()))
        .unwrap();
    assert_ne!(first.token, second.token);
    assert_eq!(first.token.len(), 64);
    assert_eq!(
        old_token_here["ok"], false,
        "the last run's token is worthless"
    );
    // After the run, nothing listens.
    assert!(raw(&first.addr, &first.token).is_none());
}

#[test]
fn the_mcp_config_names_only_the_shim() {
    let core = Core::demo().unwrap();
    let (config, _) = core
        .with_tool_host(|run| run.mcp_config(std::path::Path::new("/opt/skyla/skyla-mcp")))
        .unwrap();
    let servers = config["mcpServers"].as_object().unwrap();
    assert_eq!(servers.keys().collect::<Vec<_>>(), ["skyla"]);
    assert_eq!(servers["skyla"]["command"], "/opt/skyla/skyla-mcp");
    assert_eq!(servers["skyla"]["args"][0], "--connect");
    assert_eq!(
        servers["skyla"]["env"].as_object().unwrap().len(),
        1,
        "only the token"
    );
}
