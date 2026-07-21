---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Reference for the `apollia.toml` configuration surface.

The runtime looks for `apollia.toml` in the working directory first, then in
`~/.config/apollia/apollia.toml`. Every section is optional: an absent section
falls back to its defaults. Runtime data (databases, the API token, models) lives
separately under `~/.apollia/`.

## Sections

| Section | Purpose |
|---|---|
| `[llm]` | LLM backend configuration. |
| `[api]` | TCP listener and authentication (`bind`, `require_token`, `tls_cert`, `tls_key`). |
| `[runtime]` | EventBus and mailbox capacities. |
| `[hitl]` | Human-in-the-loop timeout and scan interval. |
| `[a2a]` | Inter-agent routing. |
| `[oria]` | The Observer-Reasoner-Actor engine. |
| `[registry]` | Community registry URL. |
| `[tools]` | Native tools: limits, static disabling, and per-tool `[tools.web_search]` / `[tools.web_read]` configuration. |
| `[mcp]` | MCP module configuration, including `[[mcp.servers]]` (see below). |
| `[permissions]` | Permissions engine (SafeList, injection detection). |
| `[filesystem]` | Reversible journal and filesystem configuration. |
| `[hooks]` | Lifecycle hook handlers (command or HTTP). |
| `[chat]` | Chat subsystem session-level defaults (for example `plan_mode_default`). |

Sampling parameters are documented separately in
[Sampling defaults](/reference/sampling-defaults). The `[tools.web_search]` and
`[tools.web_read]` keys are also editable from the CLI with
`apollia-os tools config set <tool>.<key> <value>`.

The MCP section below is documented in full because its limits are
security-relevant. The other sections are summarised above; consult the field
defaults in the runtime configuration types.

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
