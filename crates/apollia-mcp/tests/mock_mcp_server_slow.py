#!/usr/bin/env python3
"""Mock MCP server whose tool calls take time and run concurrently.

Reads JSON-RPC messages on stdin, responds on stdout. Each `tools/call` is
answered from a thread of its own, so two calls in flight finish in the order
of their durations, not of their arrival.

Tool `wait`: sleeps `seconds`, then answers; `fail: true` answers an error.
"""
import json
import sys
import threading
import time

WRITE_LOCK = threading.Lock()


def respond(request_id, result):
    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    with WRITE_LOCK:
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()


def answer_wait(request_id, args):
    time.sleep(float(args.get("seconds", 0)))
    if args.get("fail"):
        respond(request_id, {
            "content": [{"type": "text", "text": "object_not_found"}],
            "isError": True,
        })
    else:
        respond(request_id, {
            "content": [{"type": "text", "text": "done"}],
            "isError": False,
        })


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
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "mock-slow-server", "version": "1.0.0"},
            })
        elif method == "tools/list":
            respond(request_id, {
                "tools": [
                    {
                        "name": "wait",
                        "description": "Wait a number of seconds, then answer",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "seconds": {"type": "number"},
                                "fail": {"type": "boolean"},
                            },
                        },
                    }
                ]
            })
        elif method == "tools/call":
            args = msg["params"].get("arguments", {}) or {}
            threading.Thread(
                target=answer_wait, args=(request_id, args), daemon=True
            ).start()


if __name__ == "__main__":
    main()
