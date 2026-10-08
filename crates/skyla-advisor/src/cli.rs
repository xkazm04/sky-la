//! The production driver: the user's own, unmodified `claude` binary,
//! launched with the hardened profile from the WP-01 spike
//! (`docs/spikes/WP-01-cli.md`).
//!
//! sky-la never reads, stores or proxies Claude credentials. The child
//! starts from an empty environment with only what it needs to find the
//! user's own login (signed in with `claude auth login` in their terminal),
//! in an empty per-run directory, with stdin closed. The first event is
//! checked against the profile; any extra tool, server, command or skill
//! stops the run before the model can use it.

use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde_json::Value;

use crate::cli_stream::{StreamEvent, parse_line, profile_violations};
use crate::provider::{Availability, LlmProvider, RunOutcome, RunRequest, RunStatus, fold};

/// A process to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// The binary.
    pub program: PathBuf,
    /// Arguments, in order.
    pub args: Vec<String>,
    /// The whole environment (nothing is inherited).
    pub env: Vec<(String, String)>,
    /// Working directory.
    pub cwd: PathBuf,
}

/// What to do after a line of output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Keep reading.
    Continue,
    /// Kill the process now.
    Kill,
}

/// Starts processes. The system launcher spawns them; tests replay
/// recorded transcripts through the same driver code.
pub trait Launcher: Send + Sync {
    /// `program --version`, trimmed.
    fn version(&self, program: &Path, env: &[(String, String)]) -> io::Result<String>;
    /// Runs `spec`, handing each stdout line to `on_line`.
    fn run(&self, spec: &CommandSpec, on_line: &mut dyn FnMut(&str) -> Control) -> io::Result<()>;
}

/// Spawns real processes.
#[derive(Debug, Default)]
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn version(&self, program: &Path, env: &[(String, String)]) -> io::Result<String> {
        let out = Command::new(program)
            .arg("--version")
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()?;
        if !out.status.success() {
            return Err(io::Error::other(format!(
                "--version exited with {}",
                out.status
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn run(&self, spec: &CommandSpec, on_line: &mut dyn FnMut(&str) -> Control) -> io::Result<()> {
        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .env_clear()
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .current_dir(&spec.cwd)
            // The CLI waits three seconds for stdin unless it's closed (WP-01, C5).
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        for line in BufReader::new(stdout).lines() {
            if on_line(&line?) == Control::Kill {
                let _ = child.kill();
                break;
            }
        }
        child.wait()?;
        Ok(())
    }
}

/// Environment variables passed through to the child, when set. Only what
/// finding the user's own login and running needs; never API keys or
/// tokens. WP-01's desktop check C6 settles the final list per platform.
pub const ENV_ALLOW_LIST: &[&str] = &[
    "HOME",
    "PATH",
    "LANG",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "SYSTEMROOT",
];

/// The scrubbed environment: the allow-list from `vars`, plus the switch
/// that turns off the CLI's nonessential traffic.
pub fn scrubbed_env(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vars
        .into_iter()
        .filter(|(k, _)| ENV_ALLOW_LIST.contains(&k.as_str()))
        .collect();
    env.sort();
    env.push((
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
        "1".into(),
    ));
    env
}

/// Where to look for `claude`: every `PATH` entry, then the CLI's usual
/// install locations under `home`.
pub fn candidates(path: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let mut out = Vec::new();
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            for n in names {
                out.push(dir.join(n));
            }
        }
    }
    if let Some(home) = home {
        for n in names {
            out.push(home.join(".claude").join("local").join(n));
            out.push(home.join(".local").join("bin").join(n));
        }
    }
    out
}

/// The hardened argv for a run whose files live in `run_dir`.
pub fn hardened_args(req: &RunRequest, run_dir: &Path) -> Vec<String> {
    let path = |name: &str| run_dir.join(name).to_string_lossy().into_owned();
    let mut args: Vec<String> = vec![
        "-p".into(),
        req.prompt.clone(),
        "--restricted".into(),
        "--tools".into(),
        String::new(),
        "--disable-slash-commands".into(),
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        path("skyla-mcp.json"),
        "--allowedTools".into(),
        format!("mcp__{}__*", req.server),
        "--permission-mode".into(),
        "dontAsk".into(),
        "--permission-prompts".into(),
        "none".into(),
        "--system-prompt-file".into(),
        path("system.md"),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--no-session-persistence".into(),
        "--max-turns".into(),
        req.max_turns.to_string(),
    ];
    if let Some(schema) = &req.schema {
        args.push("--json-schema".into());
        args.push(schema.to_string());
    }
    if let Some(model) = &req.model {
        args.push("--model".into());
        args.push(model.clone());
    }
    if let Some(effort) = &req.effort {
        args.push("--effort".into());
        args.push(effort.clone());
    }
    args
}

/// The Claude Code CLI driver.
pub struct ClaudeCodeCli {
    program: Option<PathBuf>,
    looked_in: Vec<String>,
    env: Vec<(String, String)>,
    launcher: Box<dyn Launcher>,
    last: Mutex<Option<RunStatus>>,
}

impl ClaudeCodeCli {
    /// A driver for the first `claude` found among `candidates`, started
    /// by `launcher` with `env` (already scrubbed).
    pub fn new(
        candidates: &[PathBuf],
        env: Vec<(String, String)>,
        launcher: Box<dyn Launcher>,
    ) -> Self {
        let program = candidates.iter().find(|p| p.is_file()).cloned();
        Self {
            program,
            looked_in: candidates.iter().map(|p| p.display().to_string()).collect(),
            env,
            launcher,
            last: Mutex::new(None),
        }
    }

    /// A driver for an explicit binary (the user's setting, or a test).
    pub fn at(program: PathBuf, env: Vec<(String, String)>, launcher: Box<dyn Launcher>) -> Self {
        Self {
            looked_in: vec![program.display().to_string()],
            program: Some(program),
            env,
            launcher,
            last: Mutex::new(None),
        }
    }

    /// The driver for this machine: `claude` on `PATH` or in its usual
    /// places, with this process's environment scrubbed.
    pub fn from_system() -> Self {
        let path = std::env::var("PATH").ok();
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        Self::new(
            &candidates(path.as_deref(), home.as_deref()),
            scrubbed_env(std::env::vars()),
            Box::new(SystemLauncher),
        )
    }

    fn remember(&self, status: &RunStatus) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(status.clone());
        }
    }

    fn write_run_files(dir: &Path, req: &RunRequest) -> io::Result<()> {
        std::fs::write(dir.join("system.md"), &req.system_prompt)?;
        let mcp = req
            .mcp_config
            .clone()
            .unwrap_or_else(|| serde_json::json!({ "mcpServers": {} }));
        std::fs::write(dir.join("skyla-mcp.json"), mcp.to_string())
    }
}

impl LlmProvider for ClaudeCodeCli {
    fn id(&self) -> &str {
        "claude-code-cli"
    }

    fn availability(&self) -> Availability {
        let Some(program) = &self.program else {
            return Availability::NotInstalled {
                looked_in: self.looked_in.clone(),
            };
        };
        let version = match self.launcher.version(program, &self.env) {
            Ok(v) => v,
            Err(_) => {
                return Availability::NotInstalled {
                    looked_in: self.looked_in.clone(),
                };
            }
        };
        match self.last.lock().ok().and_then(|l| l.clone()) {
            Some(RunStatus::NotSignedIn) => Availability::NotSignedIn,
            Some(RunStatus::RateLimited(resets_at)) => Availability::RateLimited { resets_at },
            _ => Availability::Ready { version },
        }
    }

    fn run(&self, req: &RunRequest, on_event: &mut dyn FnMut(&StreamEvent)) -> RunOutcome {
        let Some(program) = &self.program else {
            return RunOutcome::with_status(RunStatus::NotInstalled);
        };
        let dir = match tempfile::Builder::new().prefix("skyla-run-").tempdir() {
            Ok(d) => d,
            Err(e) => {
                return RunOutcome::with_status(RunStatus::Failed(format!("run directory: {e}")));
            }
        };
        if let Err(e) = Self::write_run_files(dir.path(), req) {
            return RunOutcome::with_status(RunStatus::Failed(format!("run files: {e}")));
        }
        let spec = CommandSpec {
            program: program.clone(),
            args: hardened_args(req, dir.path()),
            env: self.env.clone(),
            cwd: dir.path().to_path_buf(),
        };
        let mut events = Vec::new();
        let mut violation: Option<Vec<String>> = None;
        let mut unparsed = 0_usize;
        let launched = self.launcher.run(&spec, &mut |line| {
            let Ok(parsed) = parse_line(line) else {
                unparsed += 1;
                return Control::Continue;
            };
            for e in parsed {
                if let StreamEvent::Init(init) = &e {
                    let problems = profile_violations(init, &req.server);
                    if !problems.is_empty() {
                        violation = Some(problems);
                        events.push(e);
                        return Control::Kill;
                    }
                }
                on_event(&e);
                events.push(e);
            }
            Control::Continue
        });
        let outcome = match (launched, violation) {
            (_, Some(problems)) => RunOutcome {
                status: RunStatus::ProfileViolation(problems),
                ..fold(&events)
            },
            (Err(e), None) if e.kind() == io::ErrorKind::NotFound => {
                RunOutcome::with_status(RunStatus::NotInstalled)
            }
            (Err(e), None) => RunOutcome::with_status(RunStatus::Failed(e.to_string())),
            (Ok(()), None) => {
                let mut out = fold(&events);
                if events.is_empty() && unparsed > 0 {
                    out.status = RunStatus::Failed(format!("{unparsed} lines weren't stream-json"));
                }
                out
            }
        };
        self.remember(&outcome.status);
        outcome
    }
}

/// The arguments' JSON Schema text, for tests that inspect argv.
pub fn schema_arg(args: &[String]) -> Option<Value> {
    let i = args.iter().position(|a| a == "--json-schema")?;
    serde_json::from_str(args.get(i + 1)?).ok()
}
