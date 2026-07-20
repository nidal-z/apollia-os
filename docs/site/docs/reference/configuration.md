---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Reference for the `apollia.toml` configuration surface.

_Phase 1 placeholder. Migrate from the existing configuration reference in a
later phase._

## MCP servers (`[[mcp.servers]]`)

Each entry configures one MCP server. The security-relevant read limit:

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

```toml
[[mcp.servers]]
name = "example"
transport = "streamable-http"
url = "https://mcp.example.com/mcp"
max_response_bytes = 16777216  # 16 MiB
```
