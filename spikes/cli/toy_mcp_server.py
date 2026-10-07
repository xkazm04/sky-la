#!/usr/bin/env python3
"""WP-01 spike only: a minimal MCP stdio server mimicking sky-la's tool surface.

Stdlib only. Speaks newline-delimited JSON-RPC 2.0 on stdin/stdout. Every
request and every tool result is appended to $SKYLA_SPIKE_LOG, which is
exactly what would leave the machine. Throwaway: WP-25 builds the real shim
in Rust.
"""

import json
import os
import sys

LOG = os.environ.get("SKYLA_SPIKE_LOG", "/dev/null")

TOOLS = [
    {
        "name": "get_period_summary",
        "description": "Aggregate figures for an accounting period. Amounts are integer minor units (haléř).",
        "inputSchema": {
            "type": "object",
            "properties": {"period": {"type": "string", "description": "e.g. 2026-Q3"}},
            "required": ["period"],
            "additionalProperties": False,
        },
    },
    {
        "name": "propose_entry",
        "description": "Propose a journal entry for human review. Never posts anything.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "memo": {"type": "string"},
                "lines": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"account": {"type": "string"}, "amount_minor": {"type": "integer"}},
                        "required": ["account", "amount_minor"],
                        "additionalProperties": False,
                    },
                },
            },
            "required": ["memo", "lines"],
            "additionalProperties": False,
        },
    },
]

SUMMARY = {"period": "2026-Q3", "revenue_minor": 45_650_000, "expenses_minor": 17_615_000, "profit_minor": 28_035_000}


def log(kind, payload):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(json.dumps({"kind": kind, "payload": payload}, ensure_ascii=False) + "\n")


def reply(msg_id, result=None, error=None):
    out = {"jsonrpc": "2.0", "id": msg_id}
    out["error" if error else "result"] = error or result
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()


for raw in sys.stdin:
    raw = raw.strip()
    if not raw:
        continue
    msg = json.loads(raw)
    method, msg_id = msg.get("method"), msg.get("id")
    log("request", {"method": method, "params": msg.get("params")})
    if msg_id is None:  # notification
        continue
    if method == "initialize":
        version = msg.get("params", {}).get("protocolVersion", "2025-06-18")
        reply(msg_id, {"protocolVersion": version, "capabilities": {"tools": {}}, "serverInfo": {"name": "skyla-spike", "version": "0.0.1"}})
    elif method == "tools/list":
        reply(msg_id, {"tools": TOOLS})
    elif method == "tools/call":
        name = msg["params"]["name"]
        args = msg["params"].get("arguments", {})
        if name == "get_period_summary":
            text = json.dumps(SUMMARY)
        elif name == "propose_entry":
            total = sum(line["amount_minor"] for line in args.get("lines", []))
            text = json.dumps({"stored": True, "proposal_id": "P-1", "balanced": total == 0})
        else:
            reply(msg_id, error={"code": -32602, "message": f"unknown tool {name}"})
            continue
        log("tool_result", {"tool": name, "text": text})
        reply(msg_id, {"content": [{"type": "text", "text": text}], "isError": False})
    elif method == "ping":
        reply(msg_id, {})
    else:
        reply(msg_id, error={"code": -32601, "message": f"method not found: {method}"})
