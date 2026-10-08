//! WP-24 acceptance: every driver meets the same contract on recorded
//! transcripts, ends each run in a clear state (not installed, not signed
//! in, rate-limited, stopped by the profile guard, interrupted), and the
//! CLI driver launches `claude` with the hardened profile and nothing else.

#![allow(clippy::unwrap_used)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use skyla_advisor::cli::{
    CommandSpec, Control, Launcher, candidates, hardened_args, schema_arg, scrubbed_env,
};
use skyla_advisor::cli_stream::StreamEvent;
use skyla_advisor::{Availability, ClaudeCodeCli, Fake, LlmProvider, RunRequest, RunStatus};

/// The recorded transcripts as the hardened profile now produces them:
/// WP-01 recorded them before `--disable-slash-commands`, which empties
/// these two lists (check C9).
fn hardened(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut v: Value = serde_json::from_str(line).unwrap();
            if v["type"] == "system" && v["subtype"] == "init" {
                v["slash_commands"] = json!([]);
                v["skills"] = json!([]);
            }
            v.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn transcript(name: &str) -> String {
    match name {
        "c1" => hardened(include_str!("../fixtures/cli/c1_tools_schema.jsonl")),
        "c4" => hardened(include_str!("../fixtures/cli/c4_propose.jsonl")),
        // C7 was recorded without the MCP server; its interrupted ending
        // follows the hardened C1 start here.
        "c7" => {
            let start = hardened(include_str!("../fixtures/cli/c1_tools_schema.jsonl"));
            let init = start.lines().next().unwrap().to_owned();
            let rest: Vec<&str> = include_str!("../fixtures/cli/c7_sigint.jsonl")
                .lines()
                .filter(|l| !l.contains("\"subtype\":\"init\""))
                .collect();
            format!("{init}\n{}", rest.join("\n"))
        }
        "not_signed_in" => include_str!("../fixtures/cli/synthetic_not_signed_in.jsonl").into(),
        "rate_limited" => include_str!("../fixtures/cli/synthetic_rate_limited.jsonl").into(),
        "extra_tool" => include_str!("../fixtures/cli/synthetic_extra_tool.jsonl").into(),
        "recorded_c1" => include_str!("../fixtures/cli/c1_tools_schema.jsonl").into(),
        other => panic!("no transcript {other}"),
    }
}

fn request(task: &str) -> RunRequest {
    RunRequest {
        task: task.into(),
        system_prompt: "You are the sky-la test advisor.".into(),
        prompt: "Call get_period_summary for 2026-Q3 and report the profit.".into(),
        schema: Some(
            json!({ "type": "object", "properties": { "profit_minor": { "type": "integer" } } }),
        ),
        mcp_config: Some(json!({ "mcpServers": { "skyla": { "command": "skyla-mcp" } } })),
        server: "skyla".into(),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        max_turns: 8,
    }
}

/// Replays a transcript as the child's stdout and records what was launched.
struct Replay {
    text: String,
    launched: Arc<Mutex<Vec<CommandSpec>>>,
    lines_read: Arc<Mutex<usize>>,
}

impl Launcher for Replay {
    fn version(&self, _: &Path, _: &[(String, String)]) -> io::Result<String> {
        Ok("2.1.293 (Claude Code)".into())
    }

    fn run(&self, spec: &CommandSpec, on_line: &mut dyn FnMut(&str) -> Control) -> io::Result<()> {
        self.launched.lock().unwrap().push(spec.clone());
        // The run's files are written before launch.
        assert!(spec.cwd.join("system.md").is_file() && spec.cwd.join("skyla-mcp.json").is_file());
        for line in self.text.lines() {
            *self.lines_read.lock().unwrap() += 1;
            if on_line(line) == Control::Kill {
                break;
            }
        }
        Ok(())
    }
}

/// What the replay launcher saw: the launches and the lines handed over.
type Seen = (Arc<Mutex<Vec<CommandSpec>>>, Arc<Mutex<usize>>);

fn cli(name: &str) -> (ClaudeCodeCli, Seen) {
    let launched = Arc::new(Mutex::new(Vec::new()));
    let lines = Arc::new(Mutex::new(0));
    let driver = ClaudeCodeCli::at(
        PathBuf::from("/usr/local/bin/claude"),
        scrubbed_env([
            ("HOME".into(), "/home/u".into()),
            ("PATH".into(), "/usr/bin".into()),
        ]),
        Box::new(Replay {
            text: transcript(name),
            launched: Arc::clone(&launched),
            lines_read: Arc::clone(&lines),
        }),
    );
    (driver, (launched, lines))
}

/// Both drivers, answering `task` with the transcript `name`.
fn drivers(task: &str, name: &str) -> Vec<Box<dyn LlmProvider>> {
    vec![
        Box::new(Fake::new().with(task, &transcript(name))),
        Box::new(cli(name).0),
    ]
}

#[test]
fn a_completed_run_returns_the_structured_answer_and_its_tool_calls() {
    for d in drivers("tax.summary", "c1") {
        let mut seen = 0;
        let out = d.run(&request("tax.summary"), &mut |_| seen += 1);
        assert_eq!(out.status, RunStatus::Completed, "{}", d.id());
        assert_eq!(
            out.structured.as_ref().unwrap()["profit_minor"],
            28_035_000,
            "{}",
            d.id()
        );
        let call = &out.tool_calls[0];
        assert_eq!(call.name, "mcp__skyla__get_period_summary");
        assert!(
            call.result.as_ref().unwrap().contains("28035000"),
            "tool results are kept for the register"
        );
        assert!(out.cost_estimate_usd.is_some() && seen > 0);
    }
}

#[test]
fn a_proposal_comes_back_as_a_tool_call_never_as_a_posting() {
    for d in drivers("propose", "c4") {
        let out = d.run(&request("propose"), &mut |_| {});
        assert_eq!(out.status, RunStatus::Completed, "{}", d.id());
        assert!(
            out.tool_calls
                .iter()
                .any(|c| c.name == "mcp__skyla__propose_entry")
        );
    }
}

#[test]
fn every_ending_has_a_clear_state() {
    for (name, expected) in [
        ("not_signed_in", RunStatus::NotSignedIn),
        (
            "rate_limited",
            RunStatus::RateLimited(Some("1791763200".into())),
        ),
        ("c7", RunStatus::Interrupted),
    ] {
        for d in drivers("t", name) {
            assert_eq!(
                d.run(&request("t"), &mut |_| {}).status,
                expected,
                "{name} on {}",
                d.id()
            );
        }
    }
}

#[test]
fn the_profile_guard_stops_a_run_before_any_tool_runs() {
    let (driver, (_, lines)) = cli("extra_tool");
    let mut tool_events = 0;
    let out = driver.run(&request("t"), &mut |e| {
        if matches!(e, StreamEvent::ToolUse { .. }) {
            tool_events += 1;
        }
    });
    assert_eq!(
        out.status,
        RunStatus::ProfileViolation(vec!["unexpected tool Bash".into()])
    );
    assert_eq!(*lines.lock().unwrap(), 1, "killed on the init line");
    assert_eq!(tool_events, 0);
    // The unhardened recording is stopped too (its commands and skills).
    let (driver, _) = cli("recorded_c1");
    assert!(matches!(
        driver.run(&request("t"), &mut |_| {}).status,
        RunStatus::ProfileViolation(_)
    ));
    let fake = Fake::new().with("t", &transcript("extra_tool"));
    assert!(matches!(
        fake.run(&request("t"), &mut |_| {}).status,
        RunStatus::ProfileViolation(_)
    ));
}

#[test]
fn availability_follows_the_last_run() {
    let (driver, _) = cli("not_signed_in");
    assert!(matches!(driver.availability(), Availability::Ready { .. }));
    driver.run(&request("t"), &mut |_| {});
    assert_eq!(driver.availability(), Availability::NotSignedIn);
    let (driver, _) = cli("rate_limited");
    driver.run(&request("t"), &mut |_| {});
    assert_eq!(
        driver.availability(),
        Availability::RateLimited {
            resets_at: Some("1791763200".into())
        }
    );
    let missing = ClaudeCodeCli::new(
        &[PathBuf::from("/nonexistent/claude")],
        Vec::new(),
        Box::new(skyla_advisor::cli::SystemLauncher),
    );
    assert_eq!(
        missing.availability(),
        Availability::NotInstalled {
            looked_in: vec!["/nonexistent/claude".into()]
        }
    );
    assert_eq!(
        missing.run(&request("t"), &mut |_| {}).status,
        RunStatus::NotInstalled
    );
}

#[test]
fn the_launch_is_the_hardened_profile() {
    let (driver, (launched, _)) = cli("c1");
    driver.run(&request("tax.summary"), &mut |_| {});
    let spec = launched.lock().unwrap()[0].clone();
    let a = &spec.args;
    let has = |pair: &[&str]| {
        a.windows(pair.len())
            .any(|w| w.iter().zip(pair).all(|(x, y)| x == y))
    };
    assert_eq!(a[0], "-p");
    assert!(has(&["--tools", ""]));
    for flag in [
        "--restricted",
        "--disable-slash-commands",
        "--strict-mcp-config",
        "--no-session-persistence",
        "--verbose",
    ] {
        assert!(a.contains(&flag.to_owned()), "{flag}");
    }
    assert!(has(&["--allowedTools", "mcp__skyla__*"]));
    assert!(has(&["--permission-mode", "dontAsk"]));
    assert!(has(&["--output-format", "stream-json"]));
    assert!(has(&["--max-turns", "8"]) && has(&["--model", "haiku"]) && has(&["--effort", "low"]));
    assert_eq!(
        schema_arg(a).unwrap()["properties"]["profit_minor"]["type"],
        "integer"
    );
    // Nothing inherited: only the allow-list and the traffic switch.
    let keys: Vec<&str> = spec.env.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["HOME", "PATH", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"]
    );
    // The run directory is gone afterwards.
    assert!(!spec.cwd.exists());
}

#[test]
fn credentials_and_session_variables_never_reach_the_child() {
    let env = scrubbed_env([
        ("HOME".into(), "/home/u".into()),
        ("ANTHROPIC_API_KEY".into(), "sk-ant-secret".into()),
        ("CLAUDE_CODE_OAUTH_TOKEN".into(), "secret".into()),
        ("CLAUDE_CODE_SESSION_ID".into(), "parent".into()),
        ("AWS_SECRET_ACCESS_KEY".into(), "secret".into()),
    ]);
    assert!(
        env.iter()
            .all(|(_, v)| !v.contains("secret") && v != "parent"),
        "{env:?}"
    );
}

#[test]
fn the_binary_is_looked_for_on_path_then_in_the_usual_places() {
    let c = candidates(Some("/opt/a:/opt/b"), Some(Path::new("/home/u")));
    let shown: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
    if cfg!(unix) {
        assert_eq!(
            shown,
            [
                "/opt/a/claude",
                "/opt/b/claude",
                "/home/u/.claude/local/claude",
                "/home/u/.local/bin/claude"
            ]
        );
    }
    let args = hardened_args(&request("t"), Path::new("/run"));
    assert!(args.contains(&"/run/skyla-mcp.json".to_owned()) || cfg!(windows));
}

/// A stand-in `claude` that reports what it was given: the real launcher,
/// a real process.
#[cfg(unix)]
#[test]
fn the_real_launcher_scrubs_the_environment_closes_stdin_and_runs_in_an_empty_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report");
    std::fs::create_dir(&report).unwrap();
    let fixture = dir.path().join("c1.jsonl");
    std::fs::write(&fixture, transcript("c1")).unwrap();
    let script = dir.path().join("claude");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.293 (Claude Code)'; exit 0; fi\n\
             env | sort > {r}/env\nls -A > {r}/ls\n\
             if read -r line; then echo open > {r}/stdin; else echo closed > {r}/stdin; fi\n\
             cat {f}\n",
            r = report.display(),
            f = fixture.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let driver = ClaudeCodeCli::new(
        &[script],
        scrubbed_env([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), dir.path().display().to_string()),
            ("ANTHROPIC_API_KEY".into(), "sk-ant-secret".into()),
        ]),
        Box::new(skyla_advisor::cli::SystemLauncher),
    );
    assert_eq!(
        driver.availability(),
        Availability::Ready {
            version: "2.1.293 (Claude Code)".into()
        }
    );
    let out = driver.run(&request("t"), &mut |_| {});
    assert_eq!(out.status, RunStatus::Completed);
    let env = std::fs::read_to_string(report.join("env")).unwrap();
    assert!(
        !env.contains("ANTHROPIC_API_KEY")
            && env.contains("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1"),
        "{env}"
    );
    let ls = std::fs::read_to_string(report.join("ls")).unwrap();
    assert_eq!(
        ls.lines().collect::<Vec<_>>(),
        ["skyla-mcp.json", "system.md"]
    );
    assert_eq!(
        std::fs::read_to_string(report.join("stdin"))
            .unwrap()
            .trim(),
        "closed"
    );
}
