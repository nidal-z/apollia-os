---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Reference for the `apollia.toml` configuration surface.

_Phase 1 placeholder. Migrate from the existing configuration reference in a
later phase._

## MCP servers (`[[mcp.servers]]`)

Each entry configures one MCP server. The security-relevant limits:

### `max_response_bytes`

Maximum number of bytes accepted from a single server response before the
transport aborts the read with an error.

- Type: integer (bytes)
- Default: `8388608` (8 MiB)
- Bounds: `1024` to `1073741824` (1 KiB to 1 GiB)
- Applies to: `stdio`, `streamable-http`, and `sse` transports

MCP servers are untrusted. A server that never terminates a line, streams
without end, or returns an oversized body would otherwise grow daemon memory
without bound. The cap bounds a single stdio line, an HTTP body read, and the
SSE receive buffer. Raise it for servers with legitimately large tool payloads.

### `max_tools`

Maximum number of tools retained from a server's tool list. Tools beyond the cap
are dropped at discovery.

- Type: integer (count)
- Default: `256`
- Bounds: `1` to `8192`

MCP servers are untrusted. A server advertising thousands of tools would
otherwise flood the tool registry and the model's tool catalogue, exhausting
context and memory. Tool names are also validated (dropped unless they match
`[A-Za-z0-9_.-]`) and tool descriptions are stripped of control characters, so a
server cannot forge log lines or plant instructions in the model context. Raise
`max_tools` for aggregating servers that legitimately expose many tools.

```toml
[[mcp.servers]]
name = "example"
transport = "streamable-http"
url = "https://mcp.example.com/mcp"
max_response_bytes = 16777216  # 16 MiB
max_tools = 512
```
