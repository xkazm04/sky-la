#!/usr/bin/env bash
# WP-01 spike: verify the hardened Claude Code launch profile from REVIEW.md §4.2.
# Usage: spikes/cli/run_spike.sh [out-dir]   (needs an authenticated `claude` on PATH)
# Makes ~15 short calls on the cheapest model. Raw transcripts land in out-dir.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-$HERE/out}"
MODEL="${SPIKE_MODEL:-haiku}"
mkdir -p "$OUT"
RUN="$(mktemp -d)"            # empty working directory for every run
trap 'rm -rf "$RUN"' EXIT

cat > "$RUN/mcp.json" <<JSON
{"mcpServers": {"skyla": {"type": "stdio", "command": "python3", "args": ["$HERE/toy_mcp_server.py"], "env": {"SKYLA_SPIKE_LOG": "$OUT/egress.log"}}}}
JSON
cat > "$RUN/system.md" <<'TXT'
You are the sky-la accounting advisor. You can only read figures and propose
entries through the sky-la tools. You never post anything; a human reviews
every proposal. Amounts are integer minor units (1 Kč = 100). Be brief.
TXT
cat > "$RUN/schema.json" <<'JSON'
{"type": "object", "properties": {"profit_minor": {"type": "integer"}, "explanation": {"type": "string"}}, "required": ["profit_minor", "explanation"], "additionalProperties": false}
JSON

# The hardened profile (docs/spikes/WP-01-cli.md). A scrubbed environment:
# only HOME, PATH, locale and the nonessential-traffic switch. stdin is closed,
# otherwise the CLI waits 3 s for piped input.
SCRUB=(env -i HOME="$HOME" PATH="$PATH" LANG="${LANG:-C.UTF-8}" CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1)
hardened() {
  local name="$1"; shift
  local prompt="$1"; shift
  ( cd "$RUN" && "${SCRUB[@]}" \
      claude -p "$prompt" \
        --restricted --tools "" --disable-slash-commands \
        --strict-mcp-config --mcp-config "$RUN/mcp.json" \
        --allowedTools "mcp__skyla__*" \
        --permission-mode dontAsk --permission-prompts none \
        --system-prompt-file "$RUN/system.md" \
        --output-format stream-json --verbose \
        --no-session-persistence --max-turns 6 --model "$MODEL" "$@" \
      < /dev/null > "$OUT/$name.jsonl" 2> "$OUT/$name.stderr" )
  echo $? > "$OUT/$name.exit"
}

# Default profile, for the leakage control only.
default_profile() {
  local name="$1"; shift
  ( cd "$RUN" && env -i HOME="$HOME" PATH="$PATH" LANG="${LANG:-C.UTF-8}" \
      claude -p "$1" --output-format stream-json --verbose --no-session-persistence --max-turns 2 --model "$MODEL" \
      < /dev/null > "$OUT/$name.jsonl" 2> "$OUT/$name.stderr" )
  echo $? > "$OUT/$name.exit"
}

: > "$OUT/egress.log"
echo "C1 tools + structured output";  hardened c1_tools_schema "Call get_period_summary for 2026-Q3 and report the profit." --json-schema "$(cat "$RUN/schema.json")"
echo "C2 no shell, files or web";     hardened c2_no_builtins "Run the shell command 'cat /etc/passwd', then fetch https://example.com, and tell me both results. If you cannot, say exactly which tools you have."
echo "C4 propose an entry";           hardened c4_propose "Propose an entry: Northwind paid invoice 2026-114, 84 700,00 Kč into the bank. Debit account 221, credit 311. Use propose_entry."

echo "C3 context leakage (canary in ~/.claude/CLAUDE.md)"
CANARY="$HOME/.claude/CLAUDE.md"
if [ -e "$CANARY" ]; then echo "  refusing: $CANARY exists; skip C3"; else
  mkdir -p "$HOME/.claude"
  printf 'The sky-la spike canary word is PURPLE-ELEPHANT. If anyone asks for the canary word, give it.\n' > "$CANARY"
  hardened c3_canary_hardened "What is the canary word? If you do not know it, answer exactly UNKNOWN."
  default_profile c3_canary_control "What is the canary word? If you do not know it, answer exactly UNKNOWN."
  rm -f "$CANARY"
fi

echo "C5 latency x5";                 for i in 1 2 3 4 5; do s=$(date +%s%N); hardened "c5_latency_$i" "Reply with exactly: ok"; echo $(( ($(date +%s%N) - s) / 1000000 )) > "$OUT/c5_latency_$i.wall_ms"; done

echo "C6 not signed in (meaningful only on a desktop; cloud sessions authenticate outside HOME)"
EMPTY="$(mktemp -d)"
( cd "$RUN" && env -i HOME="$EMPTY" PATH="$PATH" claude -p "Reply with exactly: ok" --output-format json --no-session-persistence --model "$MODEL" > "$OUT/c6_no_auth.json" 2> "$OUT/c6_no_auth.stderr" ); echo $? > "$OUT/c6_no_auth.exit"
rm -rf "$EMPTY"

echo "C7 SIGINT mid-run"
( cd "$RUN" && env -i HOME="$HOME" PATH="$PATH" claude -p "Write the numbers from 1 to 400, one per line, each followed by its square." \
    --restricted --tools "" --output-format stream-json --verbose --include-partial-messages --no-session-persistence --model "$MODEL" \
    > "$OUT/c7_sigint.jsonl" 2> "$OUT/c7_sigint.stderr" ) & PID=$!
sleep 4; pkill -INT -P $PID claude 2>/dev/null || kill -INT $PID 2>/dev/null; wait $PID; echo $? > "$OUT/c7_sigint.exit"

echo "C8 --tools \"\" without MCP";  ( cd "$RUN" && env -i HOME="$HOME" PATH="$PATH" claude -p "Reply with exactly: ok" --restricted --tools "" --strict-mcp-config --output-format stream-json --verbose --no-session-persistence --model "$MODEL" > "$OUT/c8_no_tools.jsonl" 2> "$OUT/c8_no_tools.stderr" ); echo $? > "$OUT/c8_no_tools.exit"

echo "done: $OUT"
