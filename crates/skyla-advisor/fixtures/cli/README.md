# Recorded Claude Code CLI transcripts

Raw `--output-format stream-json --verbose` output from the WP-01 spike
(`spikes/cli/run_spike.sh`), Claude Code 2.1.293, model alias `haiku`, against
the toy MCP server. The only edit is replacing the per-run temp directory with
`/tmp/skyla-run`. They contain no credentials, account identifiers or user data.

| File | What it shows |
|---|---|
| `c1_tools_schema.jsonl` | Hardened profile; calls `get_period_summary`; returns `structured_output` via `--json-schema` |
| `c2_no_builtins.jsonl` | Asked to run a shell command and fetch a URL; has no such tools and says so |
| `c4_propose.jsonl` | Calls `propose_entry` with a balanced two-line entry |
| `c7_sigint.jsonl` | Interrupted with SIGINT mid-answer; ends with an `error_during_execution` result |

The `Fake` provider (WP-24) replays these. Re-record them with the spike
script when the CLI's stream format changes.

## Synthetic transcripts (hand-written, not recorded)

The WP-24 contract tests also need endings the spike couldn't record. These
are written by hand in the same format and say so in their names:

| File | What it stands for |
|---|---|
| `synthetic_not_signed_in.jsonl` | The CLI isn't signed in. The exact wording waits for WP-01 check C6 on a desktop; the driver matches it broadly |
| `synthetic_rate_limited.jsonl` | A usage limit with its reset time after `|`, following an API retry for `rate_limit` |
| `synthetic_extra_tool.jsonl` | A session that started with `Bash`: the profile guard must stop it before the tool call |
