//! `skyla-mcp --connect 127.0.0.1:<port>`, with the run's token in
//! `SKYLA_TOOL_TOKEN`. Started by Claude Code from the run's MCP config.

use std::io::{self, BufReader};
use std::process::ExitCode;

use skyla_mcp::{HostBackend, serve};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let addr = match args
        .iter()
        .position(|a| a == "--connect")
        .and_then(|i| args.get(i + 1))
    {
        Some(a) => a.clone(),
        None => {
            eprintln!("usage: skyla-mcp --connect 127.0.0.1:<port>");
            return ExitCode::from(2);
        }
    };
    let Ok(token) = std::env::var("SKYLA_TOOL_TOKEN") else {
        eprintln!("skyla-mcp: SKYLA_TOOL_TOKEN isn't set");
        return ExitCode::from(2);
    };
    let mut backend = match HostBackend::connect(&addr, token) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skyla-mcp: {e}");
            return ExitCode::from(1);
        }
    };
    match serve(
        BufReader::new(io::stdin().lock()),
        io::stdout().lock(),
        &mut backend,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("skyla-mcp: {e}");
            ExitCode::from(1)
        }
    }
}
