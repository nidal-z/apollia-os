#!/usr/bin/env python3
"""Mock MCP server for integration tests.

Reads JSON-RPC messages on stdin, responds on stdout.
Supports: initialize, tools/list, tools/call.
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
                "serverInfo": {"name": "mock-mcp-server", "version": "1.0.0"}
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
                    },
                    {
                        "name": "add",
                        "description": "Add two numbers",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "a": {"type": "number"},
                                "b": {"type": "number"}
                            }
                        }
                    }
                ]
            })
        elif method == "tools/call":
            name = msg["params"]["name"]
            args = msg["params"].get("arguments", {})
            if name == "echo":
                respond(id, {
                    "content": [{"type": "text", "text": args.get("message", "")}],
                    "isError": False
                })
            elif name == "add":
                result = args.get("a", 0) + args.get("b", 0)
                respond(id, {
                    "content": [{"type": "text", "text": str(result)}],
                    "isError": False
                })
            else:
                respond(id, {
                    "content": [{"type": "text", "text": f"Unknown tool: {name}"}],
                    "isError": True
                })


if __name__ == "__main__":
    main()
