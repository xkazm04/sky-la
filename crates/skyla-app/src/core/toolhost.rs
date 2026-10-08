//! The tool host (WP-25): during one advisor run the core listens on a
//! loopback port for the `skyla-mcp` shim, and answers only requests that
//! carry that run's token. A new run gets a new port and a new token; when
//! the run ends the listener closes and the token is worthless.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use skyla_egress::{Gate, GateReport};

use super::Core;
use super::tools::tool_specs;
use crate::error::CoreError;

/// One run's connection details for the shim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    /// `127.0.0.1:<port>`.
    pub addr: String,
    /// The run's token.
    pub token: String,
}

impl ToolRun {
    /// The MCP config Claude Code is given: the shim, and nothing else.
    pub fn mcp_config(&self, shim: &Path) -> Value {
        json!({
            "mcpServers": {
                "skyla": {
                    "type": "stdio",
                    "command": shim.display().to_string(),
                    "args": ["--connect", self.addr],
                    "env": { "SKYLA_TOOL_TOKEN": self.token },
                }
            }
        })
    }
}

/// A tool call as the host served it: what came in and what went back to
/// the model after the gate (the egress register records these).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCallLog {
    /// The tool.
    pub name: String,
    /// Its arguments.
    pub arguments: Value,
    /// What the model was sent.
    pub result: String,
    /// The core refused the call.
    pub refused: bool,
    /// What the gate withheld from it.
    pub report: GateReport,
}

/// 244 random bits from the OS, as hex.
fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Compares without stopping at the first difference.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0_u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

impl Core {
    /// Runs `f` with a tool host listening for this run only, and returns
    /// what `f` returned with every tool call served.
    pub fn with_tool_host<R>(
        &self,
        gate: &Gate,
        f: impl FnOnce(&ToolRun) -> R,
    ) -> Result<(R, Vec<ToolCallLog>), CoreError> {
        let io = |e: std::io::Error| CoreError::BadRequest(format!("tool host: {e}"));
        let listener = TcpListener::bind("127.0.0.1:0").map_err(io)?;
        listener.set_nonblocking(true).map_err(io)?;
        let run = ToolRun {
            addr: listener.local_addr().map_err(io)?.to_string(),
            token: new_token(),
        };
        let stop = AtomicBool::new(false);
        let log = Mutex::new(Vec::new());
        let result = std::thread::scope(|s| {
            s.spawn(|| self.serve_tools(&listener, &run.token, gate, &stop, &log));
            let r = f(&run);
            stop.store(true, Ordering::SeqCst);
            r
        });
        let calls = log.into_inner().unwrap_or_default();
        Ok((result, calls))
    }

    fn serve_tools(
        &self,
        listener: &TcpListener,
        token: &str,
        gate: &Gate,
        stop: &AtomicBool,
        log: &Mutex<Vec<ToolCallLog>>,
    ) {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => self.serve_connection(stream, token, gate, stop, log),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(_) => return,
            }
        }
    }

    fn serve_connection(
        &self,
        stream: TcpStream,
        token: &str,
        gate: &Gate,
        stop: &AtomicBool,
        log: &Mutex<Vec<ToolCallLog>>,
    ) {
        if stream.set_nonblocking(false).is_err()
            || stream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .is_err()
        {
            return;
        }
        let Ok(mut writer) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        while !stop.load(Ordering::SeqCst) {
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    continue;
                }
                Err(_) => return,
            }
            let answer = self.answer(&line, token, gate, log);
            line.clear();
            if writeln!(writer, "{answer}").is_err() {
                return;
            }
        }
    }

    fn answer(&self, line: &str, token: &str, gate: &Gate, log: &Mutex<Vec<ToolCallLog>>) -> Value {
        let Ok(request) = serde_json::from_str::<Value>(line) else {
            return json!({ "ok": false, "error": "unreadable request" });
        };
        let id = request["id"].clone();
        if !same_token(request["token"].as_str().unwrap_or_default(), token) {
            return json!({ "id": id, "ok": false, "error": "not authorised for this run" });
        }
        match request["method"].as_str() {
            Some("list") => {
                let tools: Vec<Value> = tool_specs()
                    .into_iter()
                    .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.input_schema }))
                    .collect();
                json!({ "id": id, "ok": true, "result": tools })
            }
            Some("call") => {
                let name = request["name"].as_str().unwrap_or_default();
                let arguments = request["arguments"].clone();
                // The result passes the gate before it goes anywhere.
                let (result, report, refused) = self.gated_tool_call(gate, name, &arguments);
                let answer = if refused {
                    json!({ "id": id, "ok": false, "error": result })
                } else {
                    let value: Value =
                        serde_json::from_str(&result).unwrap_or(Value::String(result.clone()));
                    json!({ "id": id, "ok": true, "result": value })
                };
                if let Ok(mut l) = log.lock() {
                    l.push(ToolCallLog {
                        name: name.to_owned(),
                        arguments,
                        result,
                        refused,
                        report,
                    });
                }
                answer
            }
            _ => json!({ "id": id, "ok": false, "error": "unknown method" }),
        }
    }
}
