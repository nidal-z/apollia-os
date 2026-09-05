#!/usr/bin/env python3
"""Mock MCP server that answers a protocol version of its own.

The version here is deliberately neither the one Apollia pins nor the one the
other mocks answer, so a test can tell a reported negotiation from a constant.
"""
import json
import sys


def respond(request_id, result):
    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


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
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "version-mcp-server", "version": "1.0.0"}
            })
        elif method == "notifications/initialized":
            pass  # notification, no response
        elif method == "tools/list":
            respond(request_id, {
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


if __name__ == "__main__":
    main()
