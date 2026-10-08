//! WP-25 acceptance (the shim): the real `skyla-mcp` process speaks MCP on
//! stdio, forwards to the host it was given with the run's token, touches
//! nothing on disk, and links nothing that could open the database.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const SHIM: &str = env!("CARGO_BIN_EXE_skyla-mcp");

/// A stand-in tool host: answers one connection, checking the token.
fn host(token: &'static str) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut seen = Vec::new();
        for line in BufReader::new(stream).lines() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let answer = if request["token"] != token {
                json!({ "id": request["id"], "ok": false, "error": "not authorised for this run" })
            } else if request["method"] == "list" {
                json!({ "id": request["id"], "ok": true, "result": [{ "name": "get_period_summary", "description": "d", "inputSchema": { "type": "object" } }] })
            } else {
                json!({ "id": request["id"], "ok": true, "result": { "profit_minor": 28_035_000 } })
            };
            seen.push(request);
            writeln!(writer, "{answer}").unwrap();
        }
        seen
    });
    (addr, handle)
}

#[test]
fn the_shim_forwards_mcp_to_the_host_with_the_token_and_writes_nothing() {
    let (addr, host) = host("t0ken");
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(SHIM)
        .args(["--connect", &addr])
        .env_clear()
        .env("SKYLA_TOOL_TOKEN", "t0ken")
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for m in [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "get_period_summary", "arguments": { "from": "2026-07-01", "to": "2026-09-30" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }),
        ] {
            writeln!(stdin, "{m}").unwrap();
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let answers: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers[0]["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(
        answers[1]["result"]["tools"][0]["name"],
        "get_period_summary"
    );
    assert!(
        answers[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("28035000")
    );
    assert_eq!(answers[3]["error"]["code"], -32601, "nothing but tools");
    let seen = host.join().unwrap();
    assert!(seen.iter().all(|r| r["token"] == "t0ken"));
    assert_eq!(seen.len(), 2);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "the shim writes nothing"
    );
}

#[test]
fn the_shim_refuses_to_start_without_a_token_or_off_the_loopback() {
    let no_token = Command::new(SHIM)
        .args(["--connect", "127.0.0.1:9"])
        .env_clear()
        .output()
        .unwrap();
    assert_eq!(no_token.status.code(), Some(2));
    let remote = Command::new(SHIM)
        .args(["--connect", "192.0.2.1:443"])
        .env_clear()
        .env("SKYLA_TOOL_TOKEN", "x")
        .output()
        .unwrap();
    assert_eq!(remote.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&remote.stderr).contains("loopback"));
}

#[test]
fn the_shim_links_nothing_that_could_open_the_database() {
    let manifest = include_str!("../Cargo.toml");
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();
    let names: Vec<&str> = deps
        .lines()
        .filter_map(|l| l.split('=').next())
        .map(str::trim)
        .filter(|n| !n.is_empty() && !n.starts_with('#'))
        .collect();
    assert_eq!(names, ["serde_json"], "the shim's only dependency");
}
