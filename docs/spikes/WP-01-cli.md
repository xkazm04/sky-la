# WP-01 spike: the hardened Claude Code launch profile

**Date:** 2026-10-07 · **Where:** a cloud session, Claude Code 2.1.293, signed in with an OAuth token (first-party), model alias `haiku`.
**Script:** [`spikes/cli/run_spike.sh`](../../spikes/cli/run_spike.sh) with the toy MCP server [`spikes/cli/toy_mcp_server.py`](../../spikes/cli/toy_mcp_server.py).
**Fixtures:** [`crates/skyla-advisor/fixtures/cli/`](../../crates/skyla-advisor/fixtures/cli/), replayed by `crates/skyla-advisor/tests/cli_fixtures.rs`.

## Verdict

The profile in `REVIEW.md` §4.2 works with subscription-style OAuth and does what it claims, with three additions the spike found necessary: **close stdin**, **`--disable-slash-commands`**, and a **scrubbed environment** with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`. One check (C6, not signed in) couldn't be run here and must be run on a desktop.

## Results

| # | Check | Result | Evidence |
|---|---|---|---|
| C0 | Baseline `claude -p` | ✓ Works, ~3.5 s wall. `total_cost_usd` present. **Finding:** the child inherited the parent's `CLAUDE_CODE_SESSION_ID`; the launcher must start from an empty environment | raw output |
| C0b | Minimal environment (`env -i HOME PATH LANG`) | ✓ Authenticates | raw output |
| C1 | `--tools ""` + MCP + `--json-schema` | ✓ Tools are exactly `StructuredOutput`, `mcp__skyla__get_period_summary`, `mcp__skyla__propose_entry`. MCP server `connected`. `structured_output = {profit_minor: 28035000, …}` matches the tool result. Cost reported | `c1_tools_schema.jsonl` |
| C2 | No shell, files or web | ✓ Asked to `cat /etc/passwd` and fetch a URL, the model said it can't and listed only the two sky-la tools. No tool call attempted | `c2_no_builtins.jsonl` |
| C3 | User context leakage | ✓ A canary in `~/.claude/CLAUDE.md`: the hardened run answered `UNKNOWN`; the default-profile control answered the canary. `--restricted` keeps user memory out. (Canary removed afterwards) | raw output |
| C4 | Proposal via MCP | ✓ `propose_entry` called with two balanced lines (221 Dr / 311 Cr, 8 470 000 minor). The server's egress log holds exactly the tool calls and the tool results, which is the second egress point the consent gate must cover | `c4_propose.jsonl` |
| C5 | Latency | stdin left open: 4 254–4 406 ms (the CLI waits 3 s for stdin). **stdin closed: 1 306–1 372 ms, p50 ≈ 1.35 s**; API time ≈ 0.5–0.6 s. With nonessential traffic off: ~1.14 s | raw output |
| C6 | Not signed in | ✗ **Not verifiable here.** An empty `HOME` still authenticated because the cloud environment supplies auth outside `HOME`. Run `run_spike.sh` on a desktop signed in through `claude auth login` (keychain) and on one that's signed out | — |
| C7 | SIGINT mid-run | ✓ Ends with a `result` event, subtype `error_during_execution`, `is_error: true`, exit 0. The driver can always fold a run into a final state | `c7_sigint.jsonl` |
| C8 | `--tools ""` without MCP | ✓ Empty tool list | raw output |
| C9 | `--disable-slash-commands` | ✓ 0 slash commands, 0 skills (without it, init lists the user's commands and skills, though no tool exists to invoke them) | raw output |
| C10 | `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` | ✓ Drops the built-in telemetry plugin from init; slightly faster | raw output |

Even hardened, `init` still lists built-in agents and plugins. With no `Task`, `Skill` or `Bash` tool none of them can run; the profile guard (below) checks the tool list, which is what matters.

## The launch profile, final

```text
env -i HOME=<home> PATH=<path> LANG=<lang> CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
claude -p <prompt>
  --restricted --tools "" --disable-slash-commands
  --strict-mcp-config --mcp-config <run>/skyla-mcp.json
  --allowedTools "mcp__skyla__*"
  --permission-mode dontAsk --permission-prompts none
  --system-prompt-file <run>/system.md
  --output-format stream-json --verbose
  --json-schema <schema>
  --no-session-persistence --max-turns 8
  --model <user choice> --effort <per task>
  < /dev/null
cwd = an empty, per-run temp directory
```

On a desktop the scrubbed environment must keep whatever the platform needs to reach the user's login (on macOS the keychain is reached through the user session, not the environment; on Linux possibly `XDG_RUNTIME_DIR` / `DBUS_SESSION_BUS_ADDRESS`). C6 on a real machine settles the exact allow-list.

## Stream-json facts the driver relies on

- `system/init`: `tools`, `mcp_servers[{name, status}]`, `model`, `permissionMode`, `slash_commands`, `skills`, `plugins`, `agents`, `cwd`, `apiKeySource`, `session_id`.
- `assistant` messages carry `tool_use{id, name, input}` and `text` blocks; `user` messages carry `tool_result{tool_use_id, content}` blocks. **Tool results are what leaves the machine** and must pass the egress gate before the MCP server returns them.
- `--json-schema` adds a `StructuredOutput` tool; the final value arrives in `result.structured_output`.
- `result`: `subtype` (`success`, `error_during_execution`, …), `is_error`, `result`, `structured_output`, `total_cost_usd`, `num_turns`, `permission_denials`, `duration_ms`.
- Incidental events (`rate_limit_event`, `system/status`, `system/thinking_tokens`, partial deltas) are ignored. Unknown types never fail a run.

## What landed in code

- `skyla_advisor::cli_stream`: `parse_line` / `parse_transcript` into typed `StreamEvent`s, and `profile_violations(init, "skyla")`, which the CLI driver (WP-24) runs on `init` and aborts on any violation: an unexpected tool or MCP server, a server that didn't connect, or loaded slash commands or skills.
- Six replay tests over the recorded fixtures.

## Follow-ups

- **C6 on a desktop** (macOS keychain login, Linux, Windows; signed out): before WP-24 ships. Tracked in `STATUS.md`.
- `total_cost_usd` is a client-side estimate. Show it as "about $0.0005", never as money in the ledger.
