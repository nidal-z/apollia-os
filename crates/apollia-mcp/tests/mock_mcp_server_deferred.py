#!/usr/bin/env python3
"""Mock MCP server that exits after the SECOND tools/list response.

Used to prove the deferred-mode schema cache. The first tools/list serves the
boot index; the second serves the first fetch_tool_schema (which caches every
schema); the server then exits. A subsequent fetch_tool_schema that succeeds
therefore must have been served from the cache, because a third tools/list
would hit a dead process.
"""
import json
import sys


def respond(request_id, result):
    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


TOOLS = [
    {
        "name": "echo",
        "description": "Echo the input",
        "inputSchema": {
            "type": "object",
            "properties": {"message": {"type": "string"}}
        }
    },
    {
        "name": "add",
        "description": "Add two numbers",
        "inputSchema": {
            "type": "object",
            "properties": {"a": {"type": "number"}, "b": {"type": "number"}}
        }
    }
]


def main():
    tools_list_count = 0
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        request_id = msg.get("id")

        if method == "initialize":
            respond(request_id, {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "deferred-mcp-server", "version": "1.0.0"}
            })
        elif method == "notifications/initialized":
            pass  # notification, no response
        elif method == "tools/list":
            tools_list_count += 1
            respond(request_id, {"tools": TOOLS})
            # Exit after the second tools/list so a third would hit a dead
            # process; a fetch that still succeeds proves the cache was used.
            if tools_list_count >= 2:
                sys.exit(0)


if __name__ == "__main__":
    main()
