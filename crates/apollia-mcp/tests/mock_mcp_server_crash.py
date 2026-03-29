#!/usr/bin/env python3
"""Mock MCP server that exits after responding to tools/list.

Used to test error handling when the server process dies while a session
is active. The session starts successfully (initialize + tools/list succeed),
then subsequent tool calls encounter a dead process.
"""
import json
import sys


def respond(id, result):
    response = {"jsonrpc": "2.0", "id": id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        id = msg.get("id")

        if method == "initialize":
            respond(id, {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "crash-mcp-server", "version": "1.0.0"}
            })
        elif method == "notifications/initialized":
            pass  # notification, no response
        elif method == "tools/list":
            respond(id, {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo the input",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"message": {"type": "string"}}
                        }
                    }
                ]
            })
            # Exit immediately after tools/list — simulates a server crash
            sys.exit(0)


if __name__ == "__main__":
    main()
