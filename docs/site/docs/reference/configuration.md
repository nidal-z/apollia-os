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

Eight sections are read. A section outside this list is ignored, and
`apollia-os config set` rejects it rather than accepting a value nothing will
ever consult.

| Section | Purpose |
|---|---|
| `[llm]` | LLM backend configuration. |
| `[api]` | TCP listener and authentication (`bind`, `require_token`, `tls_cert`, `tls_key`). |
| `[runtime]` | EventBus and mailbox capacities. |
| `[hitl]` | Human-in-the-loop timeout and scan interval. |
| `[tools]` | Native tools: limits, static disabling, and per-tool `[tools.web_search]` / `[tools.web_read]` configuration. |
| `[mcp]` | MCP module configuration, including `[[mcp.servers]]` (see below). |
| `[hooks]` | Lifecycle hook handlers (command or HTTP). |
| `[chat]` | Chat subsystem session-level defaults (for example `plan_mode_default`). |

The desktop application reads one more section, `[observability]`, documented
below. It is not settable from the CLI.

### Sections that were withdrawn

`[a2a]`, `[oria]`, `[registry]`, `[permissions]`, `[filesystem]`, `[memory]` and
`[budget]` used to be accepted. Each deserialized into a typed structure that
nothing then consulted, so writing a value there had no effect and produced no
error either. They are no longer accepted, and a file that still carries one logs
a warning at startup. Removing them changed no behaviour, because they had none.

`[permissions]` is worth spelling out, since its name suggests otherwise:
`safe_commands` and `injection_detection` fed a permission engine that is not
active in the shipped application, while `prefix_rule_ttl_hours` and `db_path`
had no reader at all. The governance that does run, prefix rules and approvals,
takes nothing from this section. See
[cross-cutting concepts](/architecture/crosscutting-concepts).

Sampling parameters are documented separately in
[Sampling defaults](/reference/sampling-defaults). The `[tools.web_search]` and
`[tools.web_read]` keys are also editable from the CLI with
`apollia-os tools config set <tool>.<key> <value>`.

## Trace capture (`[observability]`)

Read by the desktop application only; the CLI daemon uses the defaults. Editable
from Settings, Observability, or by hand.

These switches decide what an agent's execution leaves behind on disk, in
`runtime_events.db`. Everything stays on the machine, so the question is not who
else sees it, but what remains readable locally after a run.

<!-- claim:observability-capture-switches-enforced -->

| Key | Default | Effect |
|---|---|---|
| `capture_thoughts` | `true` | Persists the reasoning text of each ReAct turn. Off, the turn leaves no thought row. |
| `capture_agent_logs` | `true` | Persists messages emitted through `ctx.logger`. Off, no log row. |
| `capture_tool_args` | `true` | Persists the argument JSON of each tool call. Off, the call is still traced, without its arguments. |
| `capture_tool_outputs` | `true` | Persists the output JSON of each tool call. Off, the call and its result are still linked, without the content. |
| `retention_days` | `90` | How many days of events are kept. The purge runs at startup and deletes nothing else: the audit trail and the signed audit journal are separate stores. `0` means never purge. |
| `max_input_bytes` | `32768` | Truncation threshold for a persisted task input. |
| `max_output_bytes` | `32768` | Truncation threshold for a persisted task output. |

<!-- claim:retention-purges-runtime-events-only -->
Turning off a switch empties the matching part of the timeline: the audit trail
and the cost history are separate and unaffected. The same separation holds for
retention: the purge deletes from the event log only. The signed audit journal is
a hash chain that `audit verify` walks end to end, so it is never trimmed on a
timer.

### Prompt content (`[llm.observability]`)

<!-- claim:debug-log-prompt-logs-at-trace -->

Prompt text is **never written to a database**. The one setting that can expose
it is `debug_log_prompt`, and it lives under `[llm.observability]`, not under
`[observability]`:

| Key | Default | Effect |
|---|---|---|
| `debug_log_prompt` | `false` | Emits the full prompt at `TRACE` level, on both the completion and the streaming path. Nothing is persisted; the exposure is in whatever collects the log stream. |

**The switch alone shows nothing.** The default log filter is `apollia=info`, and
`TRACE` sits below it, so the prompt is emitted into a level that is filtered
out. Seeing it requires both `debug_log_prompt = true` and a trace-level filter,
for instance `RUST_LOG=apollia=trace`. That is a deliberate second lock, not an
oversight, and it is why the setting is safe to leave visible in the interface.

A `debug_log_prompt` written under `[observability]` is read by nothing: the two
sections deserialize into two different types, and only the `[llm.observability]`
one reaches the router.

### Keys that are declared but not implemented

| Key | State |
|---|---|
| `capture_thoughts` on ORIA plan steps | Partial. Plan-step inputs and outputs are persisted with the compiled defaults, so the byte limits above do not apply to them. |
| `max_tool_output_bytes` | **Not implemented.** Never had a use site. |

These are listed rather than hidden because the settings page still shows some of
them. A switch that looks like a privacy control and does nothing is worse than
an absent one, so until they are implemented or removed, this table is the
authority.

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
