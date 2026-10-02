#!/usr/bin/env python3
"""Mock MCP server whose three tools declare different behaviour hints.

`lookup` is declared read-only, `purge` destructive, and `ping` declares
nothing. Supports: initialize, tools/list, tools/call.
"""
import json
import sys


def respond(request_id, result):
    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


TOOLS = [
    {
        "name": "lookup",
        "title": "Look up a record",
        "description": "Read a record",
        "inputSchema": {"type": "object"},
        "annotations": {"readOnlyHint": True, "openWorldHint": False},
    },
    {
        "name": "purge",
        "description": "Delete every record",
        "inputSchema": {"type": "object"},
        "annotations": {"readOnlyHint": False, "destructiveHint": True},
    },
    {
        "name": "ping",
        "description": "Answer pong",
        "inputSchema": {"type": "object"},
    },
]


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        request_id = msg.get("id")
        if method == "initialize":
            respond(request_id, {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "mock-annotated", "version": "1.0.0"},
            })
        elif method == "tools/list":
            respond(request_id, {"tools": TOOLS})
        elif method == "tools/call":
            respond(request_id, {
                "content": [{"type": "text", "text": "ok"}],
                "isError": False,
            })


if __name__ == "__main__":
    main()
