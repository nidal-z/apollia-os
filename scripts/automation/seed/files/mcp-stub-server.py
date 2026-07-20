#!/usr/bin/env python3
"""Deterministic stdio MCP server for the desktop automation seed.

Why this exists
---------------
The desktop connections sidebar is fed by `list_mcp_servers`, which the embedded
runtime answers from the MCP client manager's LIVE session set, not from the
`mcp.db` rows directly. A server row is loaded from `mcp.db` at boot and only
appears in the sidebar once its MCP `initialize` + `tools/list` handshake
succeeds (see apollia-mcp `McpClientManager::start`: a config that fails to
connect is dropped from the session map and never reaches `status()`).

A purely static SQL seed therefore cannot surface any `sidebar-mcp-*`,
`mcp-detail-*`, `mcp-tools-*`, or `mcp-tool-*` testid. This tiny server is the
minimum needed to make the seeded rows connect at boot, so those testids find
data. It speaks just enough of the MCP protocol (2025-11-25) over
newline-delimited JSON-RPC on stdin/stdout.

Constraints
-----------
* Python standard library only (json, sys). No third-party dependency.
* Deterministic: fixed server identity and a fixed tool list, no timestamps,
  no randomness, no network, no filesystem writes.
* stdout carries ONLY JSON-RPC responses (one per line). Diagnostics, if any,
  go to stderr, which the runtime drains separately.
"""

import json
import sys

PROTOCOL_VERSION = "2025-11-25"

SERVER_INFO = {"name": "seed-mcp-stub", "version": "1.0.0"}

INSTRUCTIONS = (
    "Deterministic stub MCP server used by the Apollia desktop automation seed. "
    "It exposes a fixed set of read-only demo tools and performs no real work."
)

# Fixed tool catalogue. Each entry is a valid MCP tool definition
# (name, description, inputSchema). Kept small and stable so the tools tab
# renders a deterministic count and list.
TOOLS = [
    {
        "name": "echo",
        "description": "Return the provided text unchanged. Deterministic demo tool.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Text to echo back."}
            },
            "required": ["text"],
        },
    },
    {
        "name": "list_seed_notes",
        "description": "List the fixed demo notes bundled with the seed stub.",
        "inputSchema": {
            "type": "object",
            "properties": {},
        },
    },
    {
        "name": "get_seed_status",
        "description": "Report a static, always-healthy status string.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "Include the extended status payload.",
                }
            },
        },
    },
]


def _result(request_id, result):
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _error(request_id, code, message):
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message},
    }


def _handle_tools_call(request_id, params):
    name = (params or {}).get("name", "")
    arguments = (params or {}).get("arguments") or {}
    if name == "echo":
        text = str(arguments.get("text", ""))
        payload = f"echo: {text}"
    elif name == "list_seed_notes":
        payload = "note-1; note-2; note-3"
    elif name == "get_seed_status":
        payload = "status: ok (seed stub)"
    else:
        return _error(request_id, -32602, f"unknown tool '{name}'")
    return _result(
        request_id,
        {"content": [{"type": "text", "text": payload}], "isError": False},
    )


def _dispatch(message):
    """Return a response dict for a request, or None for a notification."""
    method = message.get("method", "")
    request_id = message.get("id")

    # Notifications (no id) never get a response.
    if request_id is None:
        return None

    if method == "initialize":
        return _result(
            request_id,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": SERVER_INFO,
                "instructions": INSTRUCTIONS,
            },
        )
    if method == "tools/list":
        return _result(request_id, {"tools": TOOLS})
    if method == "tools/call":
        return _handle_tools_call(request_id, message.get("params"))
    if method == "ping":
        return _result(request_id, {})
    if method in ("resources/list", "prompts/list"):
        # Capabilities do not advertise these, but answer gracefully if asked.
        key = "resources" if method.startswith("resources") else "prompts"
        return _result(request_id, {key: []})

    return _error(request_id, -32601, f"method not found: {method}")


def main():
    stdin = sys.stdin
    stdout = sys.stdout
    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            # Cannot recover an id from an unparseable line, skip it.
            continue
        response = _dispatch(message)
        if response is None:
            continue
        stdout.write(json.dumps(response) + "\n")
        stdout.flush()


if __name__ == "__main__":
    main()
