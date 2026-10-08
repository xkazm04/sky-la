//! MCP stdio shim that forwards advisor tool calls to the core over an
//! authenticated local socket (WP-25).
//!
//! Claude Code starts `skyla-mcp` as the run's only MCP server. The shim
//! speaks MCP (JSON-RPC 2.0, one message per line) on stdin and stdout and
//! forwards `tools/list` and `tools/call` to the core's tool host on a
//! loopback port, with the per-run token from `SKYLA_TOOL_TOKEN`. It holds
//! no data, opens no database and links nothing but `serde_json`: what a
//! tool may do is decided in the core, and the token dies with the run.

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;

use serde_json::{Value, json};

/// Where tool calls go.
pub trait Backend {
    /// The tools, as `{ name, description, inputSchema }` objects.
    fn list(&mut self) -> Result<Value, String>;
    /// Calls a tool; `Err` is a refusal the model sees as a tool error.
    fn call(&mut self, name: &str, arguments: &Value) -> Result<Value, String>;
}

/// The core's tool host, over TCP on the loopback interface.
pub struct HostBackend {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    token: String,
    next: u64,
}

impl HostBackend {
    /// Connects to `addr` (`127.0.0.1:<port>` only).
    pub fn connect(addr: &str, token: String) -> io::Result<Self> {
        let parsed: std::net::SocketAddr = addr
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "not a socket address"))?;
        if !parsed.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the tool host is only ever on the loopback interface",
            ));
        }
        let stream = TcpStream::connect(parsed)?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            token,
            next: 0,
        })
    }

    fn request(&mut self, method: &str, name: &str, arguments: &Value) -> Result<Value, String> {
        self.next += 1;
        let message = json!({
            "token": self.token,
            "id": self.next,
            "method": method,
            "name": name,
            "arguments": arguments,
        });
        writeln!(self.writer, "{message}").map_err(|e| format!("the core is unreachable: {e}"))?;
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("the core is unreachable: {e}"))?;
        let answer: Value = serde_json::from_str(&line)
            .map_err(|_| "the core sent something unreadable".to_owned())?;
        if answer["ok"] == true {
            Ok(answer["result"].clone())
        } else {
            Err(answer["error"].as_str().unwrap_or("refused").to_owned())
        }
    }
}

impl Backend for HostBackend {
    fn list(&mut self) -> Result<Value, String> {
        self.request("list", "", &Value::Null)
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Result<Value, String> {
        self.request("call", name, arguments)
    }
}

/// The MCP protocol version this shim implements when the client asks for
/// one it doesn't know.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Answers one JSON-RPC message; `None` for notifications.
pub fn handle(message: &Value, backend: &mut dyn Backend) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message["method"].as_str().unwrap_or_default();
    let params = &message["params"];
    let result = match method {
        "initialize" => json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "skyla", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => match backend.list() {
            Ok(tools) => json!({ "tools": tools }),
            Err(e) => return Some(error(&id, -32603, &e)),
        },
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return Some(error(&id, -32602, "tools/call needs a name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match backend.call(name, &arguments) {
                Ok(value) => json!({
                    "content": [{ "type": "text", "text": value.to_string() }],
                    "isError": false,
                }),
                Err(reason) => json!({
                    "content": [{ "type": "text", "text": reason }],
                    "isError": true,
                }),
            }
        }
        _ => return Some(error(&id, -32601, &format!("no method {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Serves MCP on `input`/`output` until the input closes.
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
    backend: &mut dyn Backend,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&message, backend),
            Err(_) => Some(error(&Value::Null, -32700, "parse error")),
        };
        if let Some(a) = answer {
            writeln!(output, "{a}")?;
            output.flush()?;
        }
    }
    Ok(())
}
