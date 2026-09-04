---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Reference for the `apollia.toml` configuration surface.

The file has three locations, and the two runtimes do not search the same ones.

| Reader | Search order, first match wins |
| --- | --- |
| The daemon, started by `apollia-os start` | `./apollia.toml`, then `$XDG_CONFIG_HOME/apollia/apollia.toml`, falling back to `~/.config/apollia/apollia.toml` |
| The desktop application | `~/.apollia/apollia.toml`, then `./apollia.toml`. The `~/.config` location is never read, deliberately: a copy forgotten there would resurrect backends on a fresh profile |

The command line writes to both, depending on the subcommand. `apollia-os config
set` writes the daemon's file; `apollia-os llm costs --threshold 25` writes
`~/.apollia/apollia.toml`, which only the desktop application reads. Check which
file you edited before concluding that a key had no effect.

Every section is optional: an absent section falls back to its defaults. Runtime
data (databases, the API token, models) lives separately under `~/.apollia/`.

## Sections

Nine sections are read. A section outside this list is ignored, and
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
| `[filesystem]` | The reversible journal, and the paths an agent works in without being asked (`trusted_paths`, see below). |

The desktop application reads one more section, `[observability]`, documented
below. It is not settable from the CLI.

The tables below are generated from the Rust types, so a field cannot drift out
of them. They cover the **top level of each section**. A nested table, such as an
entry of `[[llm.backends]]` or `[[mcp.servers]]`, has its own fields: the MCP one
is documented in full below, the others are read from the types they name.

### Rows the tables cannot qualify

The tables are derived from the Rust types, and a type states what a key means,
not whether anything reads it. Four things need a caveat the derivation cannot
carry.

**`[api]` is split between two loaders, and neither reads all of it.** The
daemon started by `apollia-os start` and the runtime embedded in the desktop
application build their listener from different fields of the same section. No
key is read by both, and one key is read by neither.

| Key | `apollia-os start` | The desktop application |
| --- | --- | --- |
| `bind` | read | ignored, the embedded listener is always `127.0.0.1` |
| `port` | ignored, see below | ignored, the embedded listener is always 7771 |
| `require_token` | read | ignored, the embedded TCP listener always requires the token |
| `unix_socket` | ignored, the daemon takes its socket from the `--socket` flag of `apollia-os start` and falls back to the same default | read |
| `tls_cert` | read | ignored, the embedded listener never terminates TLS |
| `tls_key` | read | ignored, the embedded listener never terminates TLS |

A key a loader ignores is still parsed and validated, then dropped: setting it
is silent, not an error. So `require_token = false` does not disarm the desktop
application, and `tls_cert` there is inert.

`[api] port` is read by neither. The daemon takes its TCP port from the
`--port` flag of `apollia-os start` and falls back to 7771; a file that sets `port = 8080`
still gets 7771.

`[llm] pricing_overrides` is not applied. A running daemon builds its backends
from the backends stored in its database, and that path hands the client an
empty override table, so a price written here never reaches a cost calculation.

`[llm.runner]` selects the speech-to-text sidecar, not the LLM engine. Its
`backend` key decides which `apollia-runner-*` binary the daemon spawns for
transcription. Local text inference runs through the bundled llama-server,
which is tuned from
[Environment variables](/reference/environment-variables) instead.

<!-- BEGIN GENERATED: config-fields -->

### `[llm]`

LLM backends and routing.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `default` | `String` | **required** | Default backend name (must exist in `backends`). |
| `backends` | `Vec<BackendConfig>` | **required** | Backends to instantiate from `[[llm.backends]]`. |
| `observability` | `ObservabilityConfig` | type default | Observability settings (tokens, latency, cost, prompt debug). |
| `routing` | `Option<LlmRoutingConfig>` | `None` | LLM routing by precision level (`[llm.routing]` section). |
| `pricing_overrides` | `HashMap<String, PricingTier>` | empty | Operator pricing overrides (`[llm.pricing_overrides]` section), not applied by a running daemon: only `LlmRouter::from_config` passes them to a client, and the daemon instead builds its backends from `system.db` through `instantiate_cloud_backend`, which hands over an empty table. |
| `cost_alert_threshold_usd` | `Option<f64>` | `None` | Cost threshold in USD above which `RuntimeEvent::TokenBudgetUpdated` is emitted with `threshold_exceeded = true`. |
| `vertex` | `Option<VertexConfig>` | `None` | Optional Google Vertex AI backend configuration (`[llm.vertex]`), not instantiated by a running daemon: the backend is only built on the `LlmRouter::from_config` path, and `to_db_configs` copies `backends` alone into `system.db`, so this section never reaches the router the daemon runs. |
| `runner` | `LlmRunnerConfig` | type default | Speech-to-text sidecar runner configuration (`[llm.runner]` section). |

### `[runtime]`

EventBus and mailbox capacities.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `eventbus_capacity` | `usize` | `1024` | EventBus broadcast channel capacity. |
| `mailbox_capacity` | `usize` | `100` | Maximum capacity of an actor mailbox. |
| `mailbox_visibility_timeout_secs` | `u64` | `60` | Visibility timeout of a leased mailbox message, in seconds. |
| `mailbox_message_ttl_secs` | `u64` | `86_400` | Time-to-live of a never-received mailbox message, in seconds. |
| `mailbox_send_quota_per_run` | `u32` | `50` | Maximum number of mailbox sends allowed per run (anti-spam guard). |
| `mailbox_max_payload_bytes` | `usize` | `65_536` | Maximum serialized payload size of a mailbox message, in bytes. |
| `mailbox_audit_full_payload` | `bool` | `false` | Whether the audit journal records the full message payload. |
| `startup_timeout_secs` | `u64` | `300` | Runtime startup timeout in seconds. |

### `[api]`

TCP listener, authentication, TLS, Unix socket.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `bind` | `String` | `"127.0.0.1".to_owned()` | IP address to bind the TCP listener to. |
| `port` | `u16` | `7771` | TCP port of the REST server. |
| `require_token` | `bool` | `true` | Require a Bearer token on every inbound TCP connection. |
| `unix_socket` | `PathBuf` | `crate::paths::socket_path_or_temp()` | Local Unix socket path. |
| `tls_cert` | `Option<PathBuf>` | `None` | PEM certificate chain for native TLS on the TCP listener. |
| `tls_key` | `Option<PathBuf>` | `None` | PEM private key matching `tls_cert`. |

### `[hitl]`

Human-in-the-loop timeout and scan interval.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `timeout_hours` | `Option<u64>` | `None` | Maximum wait for human approval, in hours. |
| `scan_interval_secs` | `u64` | `60` | Scan interval for expired HITL tasks, in seconds. |

### `[tools]`

Native tools: static disabling and per-tool settings.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `max_output_chars` | `usize` | `30_000` | Maximum size of a tool output forwarded to the LLM, in UTF-8 bytes. |
| `file_path_extraction_pattern` | `Option<String>` | `None` | Regex pattern for extracting paths from bash output. |
| `disabled` | `Vec<String>` | empty | Native tools statically disabled by the operator in `apollia.toml`. |
| `web_search` | `WebSearchConfig` | type default | Configuration of the native `web_search` tool. |
| `web_read` | `WebReadConfig` | type default | Configuration of the native `web_read` tool. |

### `[mcp]`

MCP client: tool loading and response limits.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `approval_ttl_hours` | `u64` | `24` | Validity duration of MCP HITL approvals, in hours. |
| `tool_loading` | `McpToolLoading` | type default | Tool schema loading strategy for all MCP servers. |
| `tool_search_limit` | `usize` | `20` | Maximum number of results returned by the `tool_search` synthetic tool. |

### `[hooks]`

Lifecycle hook handlers. `PreToolUse` is outside the supported surface of `v0.1.0-preview`: its decision is applied best effort, and a handler that times out, fails to deliver, or answers with something unparseable falls back to `allow`, so the tool call proceeds.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `handlers` | `Vec<HookHandlerConfig>` | empty | Registered hook handlers. |

### `[chat]`

Chat session defaults.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `plan_mode_default` | `bool` | `false` | Default plan-mode state inherited by every new chat session. |
| `default_workspace` | `Option<String>` | `None` | Default working directory for free-chat (project-less) sessions. |
| `tool_turn_temperature` | `Option<f32>` | `None` | LLM temperature applied to a chat turn that advertises tools to the model. |

### `[filesystem]`

The reversible journal, and the paths an agent works in without being asked. `trusted_paths` sets friction rather than a wall, and the two surfaces read it differently; see *Trusted paths, and what happens outside them* below.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `journal` | `JournalConfig` | type default | Sub-section dedicated to the reversible journal. |
| `trusted_paths` | `Vec<PathBuf>` | `vec![PathBuf::from("~")]` | Paths an agent may read and write without an approval prompt. |

### `[observability]`

Trace capture and retention. Read by the desktop application only.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `max_input_bytes` | `usize` | `DEFAULT_MAX_INPUT_BYTES` | Max size of task/step inputs in bytes (default 32768). |
| `max_output_bytes` | `usize` | `DEFAULT_MAX_OUTPUT_BYTES` | Max size of task/step/completion outputs in bytes (default 32768). |
| `max_tool_output_bytes` | `usize` | `DEFAULT_MAX_TOOL_OUTPUT_BYTES` | Max size of tool stdout/stderr in bytes (default 10240). |
| `capture_thoughts` | `bool` | `true` | If `true`, persists ReAct `Thought` records on the trace (default `true`). Disabling empties the reasoning bubbles in the builder UI. |
| `capture_tool_args` | `bool` | `true` | If `true`, persists the full `args_json` of tool calls (default `true`). Disabling leaves only the tool name and duration visible. |
| `capture_tool_outputs` | `bool` | `true` | If `true`, persists the full `output_json` of tool calls (default `true`). Disabling leaves only success/failure visible. |
| `capture_agent_logs` | `bool` | `true` | If `true`, persists Python `ctx.log()` calls on the trace (default `true`). Disabling keeps `tracing::*` working but writes no record in `runtime_events.db`. |
| `retention_days` | `u32` | `90` | Retention period in days for `runtime_events` before automatic purge (default 90, consistent with audit.db). |
<!-- END GENERATED: config-fields -->

### The Unix socket, in full

The default of `[api].unix_socket` is rendered above as the Rust expression the
type falls back to, since the table is derived from the source. It resolves to
`~/.apollia/runtime.sock`, and to a socket under the platform temporary
directory when no home directory can be resolved. The server sets the file to
mode `0600` right after binding, so only the account that started the runtime
can reach it.

On Windows there is no Unix socket, and this key is inert: neither the server
nor the client reads the path. The local transport there is a named pipe,
`\\.\pipe\apollia-runtime-<user>`, whose name the runtime derives from
`USERNAME` so two accounts on one machine do not contend for it. The pipe is
created with a default security descriptor rather than the `0600` of the socket
file, so it carries the same Bearer token the TCP listener carries, and the
token is what gates access. The global `--socket` flag is accepted on Windows
and ignored.

### Trusted paths, and what happens outside them

`[filesystem] trusted_paths` sets friction, not a boundary. A path under none of
these roots, and outside the session's working directory, is not refused: the
operation is suspended and the user is asked. Emptying the list therefore does
not lock an agent out of the machine, it means every write outside the working
directory raises an approval. Adding a path is a statement of trust, and the
sensitive paths of the risk table (`~/.ssh`, `/etc`, stored credentials) keep
their own classification whatever this list contains.

The working directory itself, `[chat] default_workspace` or the project's, is
where relative paths resolve. It is trusted like any entry in the list and is
never a limit on its own.

The two surfaces do not read the list the same way, and the difference is worth
knowing before you empty it:

| Surface | What the list does |
| --- | --- |
| Chat | Sets friction. Outside the roots, an operation is suspended and the user is asked. |
| Agent mode | Sets a boundary. Outside the roots, a file tool refuses, naming this setting. |

Agent mode has no approval prompt to fall back on: the filesystem approval event
is emitted by the chat invoker alone, and an agent can run with nobody watching.
Until that surface exists, a path an agent needs has to be named here.

```toml
[filesystem]
trusted_paths = ["~", "/Volumes/work", "/opt/data"]
```

### Sections that were withdrawn

`[a2a]`, `[oria]`, `[registry]`, `[permissions]`, `[memory]` and `[budget]` used
to be accepted. Each deserialized into a typed structure that nothing then
consulted, so writing a value there had no effect and produced no error either.
They are no longer accepted, and a file that still carries one logs a warning at
startup. Removing them changed no behaviour, because they had none.

`[filesystem]` was in that list. It is a live section again: `trusted_paths` is
read at startup and reaches the risk classifier, which is what decides whether an
operation runs or asks.

`[permissions]` is worth spelling out, since its name suggests otherwise: not
one of its four keys ever had a reader on an execution path. The governance that
does run, prefix rules and approvals, takes nothing from this section. See
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

## Dictation (`system.db`, not `apollia.toml`)

Voice dictation has no `apollia.toml` section. Its ten settings live in a single
row of `~/.apollia/system.db`, written from Settings, Speech-to-Text in the
desktop application, or with `apollia-os stt config get` and
`apollia-os stt config update`. Writing an `[stt]` block into `apollia.toml`
changes nothing: nothing reads it.

<!-- claim:stt-settings-apply-without-restart -->

Saving re-arms the capture flow, so a change takes effect on the next dictation
without restarting the application.

| Key | Default | Effect |
|---|---|---|
| `enabled` | `false` | Whether the dictation engine starts and the global shortcut is armed. |
| `model_path` | *(empty)* | Whisper model file. `~` is expanded. The desktop scans `~/.apollia/models` for `.bin` and `.gguf`. |
| `hotkey` | `ctrl+shift+space` | Global shortcut that starts and stops dictation. |
| `trigger_mode` | `toggle` | `toggle` (press to start, press to stop) or `push-to-talk` (hold). |
| `input_device` | *(unset)* | Microphone name as the system reports it. Unset means the system default input. |
| `language` | *(unset)* | Language forced on the engine. Unset means auto-detection. Accepted values below. |
| `silence_threshold_db` | `-40.0` | RMS level, in dB, under which a 10 ms window counts as silence. |
| `max_recording_sec` | `60` | Longest recording kept. Beyond it, the audio is truncated. |
| `clipboard_mode` | `paste` | `paste`, `clipboard`, `memo` or `both`. Applies to shortcut dictation only. |
| `clipboard_restore` | `true` | Restores the previous clipboard content after a paste. |

### Accepted language codes

<!-- claim:stt-language-hint-is-a-closed-list -->

`language` is an ISO 639-1 code from this closed list, or unset for
auto-detection. The desktop offers exactly these in a picker; a value outside the
list is rejected rather than passed through, so two machines cannot end up with
different spellings of the same language.

| Code | Language | Code | Language |
|---|---|---|---|
| `fr` | French | `pl` | Polish |
| `en` | English | `ru` | Russian |
| `es` | Spanish | `zh` | Chinese |
| `de` | German | `ja` | Japanese |
| `it` | Italian | `ko` | Korean |
| `pt` | Portuguese | `ar` | Arabic |
| `nl` | Dutch | | |

<!-- claim:stt-api-language-is-per-request -->

`POST /stt/transcribe` also accepts a `language` field, which applies to that
request only and overrides the stored value; sending it empty means
auto-detection for that request.

### Silence is not transcribed

<!-- claim:stt-refuses-silent-audio -->

A recording whose every 10 ms window sits below `silence_threshold_db` is
discarded instead of being sent to the model. This is not an optimisation.
Whisper does not answer silence with an empty string, it answers with filler
learnt from its training data, and those inventions used to arrive as if they
were transcriptions. The interface reports that nothing audible was captured, and
the log line `stt.audio.nothing_audible` records the peak level measured, which
separates a muted microphone from a threshold set too high.

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
