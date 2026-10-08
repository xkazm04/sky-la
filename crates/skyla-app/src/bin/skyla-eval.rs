//! `skyla-eval --live`: the advisor eval set, run on demand through the
//! user's own Claude Code CLI (WP-29). It never runs in default CI: it
//! refuses when `CI` is set unless `--allow-ci` is passed, and `just ci`
//! doesn't call it. The report, with the provider's cost estimates, is
//! written to `target/evals/`.
//!
//! Usage: `just eval-live` (builds the shim first), or
//! `skyla-eval --live [--limit N] [--out DIR]`. The CLI driver uses the
//! model set in the user's Claude Code.

use std::path::PathBuf;
use std::process::ExitCode;

use skyla_advisor::{Availability, ClaudeCodeCli, LlmProvider};
use skyla_app::Core;
use skyla_app::evals::{TAX_CASES, run_tax};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if flag("--list") {
        for c in TAX_CASES {
            println!("{}", c.name);
        }
        return ExitCode::SUCCESS;
    }
    if !flag("--live") {
        eprintln!(
            "skyla-eval runs the eval set against your own Claude Code installation and uses your plan's usage.\n\
             Pass --live to run it (or --list to see the cases)."
        );
        return ExitCode::from(2);
    }
    if std::env::var_os("CI").is_some() && !flag("--allow-ci") {
        eprintln!(
            "skyla-eval: CI is set; live evals never run in default CI (pass --allow-ci to override)."
        );
        return ExitCode::from(2);
    }
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    else {
        eprintln!("skyla-eval: can't find where it runs from");
        return ExitCode::from(1);
    };
    let shim = dir.join(if cfg!(windows) {
        "skyla-mcp.exe"
    } else {
        "skyla-mcp"
    });
    if !shim.is_file() {
        eprintln!(
            "skyla-eval: build the shim first (cargo build -p skyla-mcp); looked for {}",
            shim.display()
        );
        return ExitCode::from(1);
    }
    let provider = ClaudeCodeCli::from_system();
    match provider.availability() {
        Availability::Ready { version } => eprintln!("Claude Code {version}"),
        other => {
            eprintln!(
                "skyla-eval: Claude Code isn't ready: {other:?}. Install it and run `claude auth login` in a terminal."
            );
            return ExitCode::from(1);
        }
    }
    let core = match Core::demo() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skyla-eval: {e}");
            return ExitCode::from(1);
        }
    };
    core.set_shim_path(shim);
    core.replace_provider(Box::new(provider));
    let limit = value("--limit").and_then(|v| v.parse().ok());
    let report = match run_tax(&core, "claude-code-cli", limit) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skyla-eval: {e}");
            return ExitCode::from(1);
        }
    };
    for c in &report.cases {
        println!(
            "{} {:<45} {:<12} {}",
            if c.passed { "✓" } else { "✗" },
            c.name,
            c.status,
            c.reasons.join("; ")
        );
    }
    println!(
        "{}/{} passed · about ${:.4} (the provider's estimates)",
        report.passed,
        report.cases.len(),
        report.cost_estimate_usd
    );
    let out = value("--out").map_or_else(|| PathBuf::from("target/evals"), PathBuf::from);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let file = out.join(format!("tax-{stamp}.json"));
    if let Err(e) = std::fs::create_dir_all(&out).and_then(|()| {
        std::fs::write(
            &file,
            serde_json::to_vec_pretty(&report).unwrap_or_default(),
        )
    }) {
        eprintln!("skyla-eval: couldn't write {}: {e}", file.display());
        return ExitCode::from(1);
    }
    println!("Report: {}", file.display());
    if report.passed == report.cases.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
