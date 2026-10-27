#!/usr/bin/env python3
"""Mock MCP server that offers resources, for integration tests.

Reads JSON-RPC messages on stdin, responds on stdout.
Supports: initialize, tools/list, tools/call (echo), resources/list,
resources/read.

The resources listed are the lines of the file named by the environment
variable MOCK_RESOURCES_FILE, read at each resources/list, one URI per line;
without the variable, two fixed URIs. Reading a resource answers
"content of <uri>".
"""
import json
import os
import sys

DEFAULT_URIS = ["memo://alpha", "memo://beta"]


def respond(request_id, result):
    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


def listed_uris():
    path = os.environ.get("MOCK_RESOURCES_FILE")
    if not path:
        return DEFAULT_URIS
    with open(path, encoding="utf-8") as handle:
        return [line.strip() for line in handle if line.strip()]


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
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {"listChanged": False},
                    "resources": {"listChanged": False},
                },
                "serverInfo": {"name": "mock-resources-server", "version": "1.0.0"},
            })
        elif method == "notifications/initialized":
            pass
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
        elif method == "tools/call":
            args = msg["params"].get("arguments", {})
            respond(request_id, {
                "content": [{"type": "text", "text": args.get("message", "")}],
                "isError": False
            })
        elif method == "resources/list":
            respond(request_id, {
                "resources": [
                    {"uri": uri, "name": uri.split("://")[-1], "mimeType": "text/plain"}
                    for uri in listed_uris()
                ]
            })
        elif method == "resources/read":
            uri = msg["params"]["uri"]
            respond(request_id, {
                "contents": [{"uri": uri, "mimeType": "text/plain", "text": f"content of {uri}"}]
            })


if __name__ == "__main__":
    main()
