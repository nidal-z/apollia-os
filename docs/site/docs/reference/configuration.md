---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Reference for the `apollia.toml` configuration surface.

The file has one location, `~/.apollia/apollia.toml`, and every surface reads
and writes that one: the daemon started by `apollia-os start`, the desktop
application, and every command that touches the configuration
(`apollia-os config`, `apollia-os tools config`, `apollia-os doctor`).
A key set by one of them is the key the others read.

Only a path given explicitly replaces it:

| Command | Option |
| --- | --- |
| `apollia-os start` | `--config <PATH>`. The file must exist, or the start fails |
| `apollia-os config get`, `set`, `show`, `validate`, `edit` | `--file <PATH>` |

The working directory is not searched: an agent package keeps its manifest under
the same name, `apollia.toml`, and starting the daemon in a package folder does
not read it. A file left at a former location (`./apollia.toml`,
`$XDG_CONFIG_HOME/apollia/apollia.toml`, `~/.config/apollia/apollia.toml`) is
never read nor imported. The daemon logs a `config.legacy_location.ignored`
warning for it at startup, and `apollia-os doctor` names it with the command
that moves it to `~/.apollia/apollia.toml`.

Every section is optional: an absent section falls back to its defaults. Runtime
data (databases, the API token, models) lives separately under `~/.apollia/`.

## The default file

When `~/.apollia/apollia.toml` is absent, the first surface to start writes it:
the desktop application, the daemon started by `apollia-os start` (without
`--config`), or any `apollia-os config` command other than `reset`. A file that
exists is never written over.

The default file lists every key of the tables below, section by section, each
under a line saying what it does, as a commented line holding its default:

```toml
[api]

# Address the daemon's TCP listener binds to.
# bind = "127.0.0.1"
```

A commented line is worth its default, and follows the defaults of later
versions. To change a setting, uncomment its line and edit the value; to return
to the default, comment it again. Every line can be uncommented on its own and
the file stays valid. A key with no default is written as a comment that says
so. `[llm]` is written as a sentence with no key: the language models are
managed in the AI settings of the application, which keep them in the engine's
database.

`apollia-os config keys` prints the same list, and `--json` gives each key's
path, type, default, documentation, reload class, whether the organization
policy can fix it, and whether it is withdrawn.

## Changes while the runtime runs

The runtime follows its file. A text saved through `PUT /api/v1/config`, or the
file changed on disk by any other means, is read again once it has stayed
unchanged for a moment. A valid text is applied: a key whose reload class is
`Reloadable` takes effect without a restart, at the next use of the setting (the
next conversation, the next exchange, the next run of an agent, the next scan of
the paused tasks). A key of class `Restart` keeps its former value until the
runtime starts again; `GET /api/v1/config` lists it under `pendingRestart`, with
its line, and the list is empty after a start. A text that does not validate
changes nothing: the last valid configuration stays in force, the read route
returns the faults at their line, and a `ConfigInvalid` event is published. Each
application publishes a `ConfigReloaded` event with the sections applied and the
keys waiting for a restart.

## Sections

The file is read strictly, by the same validation everywhere. A key that no
section declares, or a value of the wrong type, is a fault named at its line:
`apollia-os config validate` lists every fault, `apollia-os config set` refuses
to write one, and `apollia-os start` refuses to start, with exit code 1. The
desktop application starts on the default values instead, and logs each fault
and records the refusal in the audit journal rather than dropping the file in
silence. A withdrawn section (see below) is
still accepted, with a `config.key.retired` warning.

| Section | Purpose |
|---|---|
| `[llm]` | LLM backend configuration. |
| `[api]` | TCP listener and authentication (`bind`, `require_token`, `tls_cert`, `tls_key`). |
| `[runtime]` | EventBus and mailbox capacities, and the time the runtime may take to start. |
| `[hitl]` | Human-in-the-loop timeout and scan interval. |
| `[tools]` | Native tools: limits, static disabling, and per-tool `[tools.web_search]` / `[tools.web_read]` configuration. |
| `[mcp]` | MCP module configuration, including `[[mcp.servers]]` (see below). |
| `[hooks]` | Lifecycle hook handlers (command or HTTP). |
| `[chat]` | Chat subsystem session-level defaults (for example `plan_mode_default`). |
| `[workflows]` | The budget a workflow run starts with, shared by every agent and workflow it runs. |
| `[models]` | The signed index of model cards, the models folder (`dir`), and the rule applied to an imported file (`[models.import]`). |
| `[observability]` | Trace capture and retention, documented below. |
| `[desktop]` | Settings of the desktop application: `developer_mode`, `update_channel` (`stable` or `preview`), `background_service_allowed`, and `link_preview_enabled`, which fetches the preview of a link shown in a conversation. Off by default, since the fetch leaves the machine. |
| `[server]` | The compatible server: `enabled`, `port`, `require_auth`. |
| `[engines]` | Settings of the local inference engines (`log_level`). |
| `[author]` | The publisher name of the agents exported from this machine (`name`). |

The tables below are generated from the key registry of the engine, the list
the default file and `apollia-os config keys` are made from, so a key cannot
drift out of them. They cover every key, nested tables included. An entry of a
table array, such as `[[llm.backends]]` or `[[mcp.servers]]`, has its own
fields: the MCP one is documented in full below, the others are read from the
types they name.

### Rows the tables cannot qualify

The tables are derived from the key registry, and an entry states what a key
means, not whether anything reads it. Four things need a caveat the derivation cannot
carry.

**`[api]` is split between two loaders, and neither reads all of it.** The
daemon started by `apollia-os start` and the runtime embedded in the desktop
application build their listener from different fields of the same section.
Only `port` is read by both. Every key of the section takes effect at the next
start.

| Key | `apollia-os start` | The desktop application |
| --- | --- | --- |
| `bind` | read | ignored, the embedded listener is always `127.0.0.1` |
| `port` | read, after the `--port` flag | read, see below |
| `require_token` | read | ignored, the embedded TCP listener always requires the token |
| `unix_socket` | ignored, the daemon takes its socket from the `--socket` flag of `apollia-os start` and falls back to the same default | read |
| `tls_cert` | read | ignored, the embedded listener never terminates TLS |
| `tls_key` | read | ignored, the embedded listener never terminates TLS |

A key a loader ignores is still parsed and validated, then dropped: setting it
is silent, not an error. So `require_token = false` does not disarm the desktop
application, and `tls_cert` there is inert.

`[api] port` sets the port of the native API, 7771 by default. The daemon takes
the `--port` flag of `apollia-os start` first, then this key, then 7771. Once it
serves, the daemon records its port in the data folder; the desktop application
attaches to that port when a daemon serves, and otherwise listens on this key.
The port must differ from `[server] port`, the port of the compatible server: a
file that gives both the same port is refused, at the line of `[api] port`.

Local text inference runs through the bundled llama-server, which is tuned
from [Environment variables](/reference/environment-variables), not from this
file. The speech-to-text runner is chosen from the detected hardware, with no
key to override it.

<!-- BEGIN GENERATED: config-fields -->

### `[llm]`

LLM backends, routing, and the speech-to-text runner. The default file leaves this section commented: the backends are managed in the AI settings of the application, which keep them in the engine's database.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `llm.format_version` | integer | `1` | Version of the [llm] format; read as 1 when absent. |
| `llm.default` | string | not set | Name of the backend used when none is asked for; must name an entry of llm.backends. |
| `llm.backends` | table array | not set | OpenAI-compatible backends, migrated into the engine's database on a first start; required with llm.default. |
| `llm.observability.debug_log_prompt` | boolean | `false` | Log the full prompt at trace level. |
| `llm.routing` | table | not set | Backends answering precise and fast requests, and the hybrid routing ceiling. |
| `llm.guardrails.level` | one of disabled, lenient, balanced, strict, ceiling | `"balanced"` | How strictly a local model launch is held to the machine's memory; an unknown level holds to balanced. The organization policy can fix it. |
| `llm.guardrails.ceiling_gb` | float | not set | Largest estimate, in gigabytes, the ceiling level launches. The organization policy can fix it. |
| `llm.guardrails.bypass` | one of confirm, unrestricted | `"confirm"` | What loading a refused model anyway asks first: confirm, an explicit confirmation, or unrestricted, nothing. The organization policy can fix it. |
| `llm.residency.idle_unload_minutes` | integer | `60` | Minutes without a request after which a local model nothing holds is unloaded; 0 never unloads. The organization policy can fix it. |
| `llm.defaults.context` | one of custom, model_max | `"custom"` | Context window of a local model that neither APOLLIA_LLAMA_N_CTX, nor a preset, nor its backend fixes: custom, the figure of llm.defaults.context_tokens, or model_max, the length the model was trained for. The organization policy can fix it. |
| `llm.defaults.context_tokens` | integer | `0` | Default context window, in tokens, when llm.defaults.context is custom; 0 lets the engine size it to the memory of this machine, model by model, up to 32768. The organization policy can fix it. |
| `llm.images.limit_side` | boolean | `true` | Scale an image down before it is sent to a model when its largest side exceeds llm.images.max_side_px. The organization policy can fix it. |
| `llm.images.max_side_px` | integer | `2048` | Largest side, in pixels, of an image sent to a model; proportions are kept. The organization policy can fix it. |

### `[api]`

TCP listener, authentication, TLS, Unix socket.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `api.bind` | string | `"127.0.0.1"` | Address the daemon's TCP listener binds to. |
| `api.port` | integer | `7771` | TCP port of the native API, read at the next start; the --port option of apollia-os start wins over it, and it must differ from server.port. |
| `api.require_token` | boolean | `true` | Require a bearer token on every TCP request to the daemon. |
| `api.unix_socket` | path | `"~/.apollia/runtime.sock"` | Local socket of the embedded runtime; its folder must exist. |
| `api.tls_cert` | path | not set | PEM certificate chain for TLS on the daemon's TCP listener; set with api.tls_key. |
| `api.tls_key` | path | not set | PEM private key matching api.tls_cert. |

### `[runtime]`

EventBus and mailbox capacities.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `runtime.eventbus_capacity` | integer | `1024` | Events buffered by the event bus before a slow reader lags (64 to 65536). |
| `runtime.mailbox_capacity` | integer | `100` | Messages buffered by an actor mailbox (10 to 10000). |
| `runtime.mailbox_visibility_timeout_secs` | integer | `60` | Seconds a leased agent message stays invisible before redelivery. |
| `runtime.mailbox_message_ttl_secs` | integer | `86400` | Seconds an undelivered agent message is kept. |
| `runtime.mailbox_send_quota_per_run` | integer | `50` | Agent messages one run may send. |
| `runtime.mailbox_max_payload_bytes` | integer | `65536` | Largest agent message payload, in bytes. |
| `runtime.mailbox_audit_full_payload` | boolean | `false` | Copy the full agent message payload into the audit journal. |
| `runtime.startup_timeout_secs` | integer | `300` | Seconds the engine may take to start. |
| `runtime.agent_idle_unload_secs` | integer | `600` | Seconds without a task after which an agent module is unloaded. |

### `[hitl]`

Human-in-the-loop timeout and scan interval.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `hitl.timeout_hours` | integer | not set | Hours after which an unanswered approval cancels its task (1 to 168); none waits forever. |
| `hitl.scan_interval_secs` | integer | `60` | Seconds between two scans for expired approvals (10 to 3600). |

### `[tools]`

Native tools: limits, static disabling, and per-tool settings.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `tools.max_output_chars` | integer | `30000` | Largest tool output passed to the model, in bytes (10 to 1000000). |
| `tools.disabled` | string array | `[]` | Native tools removed from every agent. |
| `tools.web_search.backend` | one of auto, duck_duck_go, brave | `"auto"` | Search engine of the web_search tool. |
| `tools.web_search.require_configured` | boolean | `false` | Refuse a search when the chosen engine is not configured. |
| `tools.web_search.brave.api_key_env_var` | string | `"BRAVE_SEARCH_API_KEY"` | Environment variable holding the Brave Search key. |
| `tools.web_search.brave.timeout_secs` | integer | `15` | Seconds a Brave Search request may take (1 to 120). |
| `tools.web_search.brave.max_results` | integer | `10` | Results asked of Brave Search (1 to 20). |
| `tools.web_search.duckduckgo.timeout_secs` | integer | `15` | Seconds a DuckDuckGo request may take (1 to 120). |
| `tools.web_search.duckduckgo.max_response_kb` | integer | `1024` | Largest DuckDuckGo response read, in KiB (16 to 16384). |
| `tools.web_read.timeout_secs` | integer | `20` | Seconds a web_read request may take (1 to 120). |
| `tools.web_read.max_response_kb` | integer | `2048` | Largest page web_read reads, in KiB (64 to 32768). |
| `tools.web_read.ssrf_guard` | boolean | `true` | Refuse an address on the local network or the machine itself. |
| `tools.python_interpreter` | path | not set | Python interpreter used instead of the bundled one. |

### `[mcp]`

MCP client: approvals, tool loading, and the bridge that serves exposed agents and workflows.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `mcp.approval_ttl_hours` | integer | `24` | Hours an MCP tool approval stays valid; 0 never expires (0 to 8760). The organization policy can fix it. |
| `mcp.tool_loading` | one of eager, deferred | `"deferred"` | Load every MCP tool schema up front, or on first use. The organization policy can fix it. |
| `mcp.tool_search_limit` | integer | `20` | Results returned by the tool_search tool (1 to 500). The organization policy can fix it. |
| `mcp.server_enabled` | boolean | `false` | Let an MCP client reach the agents and workflows marked exposed, through apollia-os mcp server. The organization policy can fix it. |

### `[hooks]`

Lifecycle hook handlers. `PreToolUse` is applied best effort: a handler that times out, fails to deliver, or answers with something unparseable falls back to `allow`, so the tool call proceeds.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `hooks.handlers` | table array | `[]` | Lifecycle hook handlers, a command or an HTTP endpoint each. |

### `[chat]`

Chat session defaults.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `chat.plan_mode_default` | boolean | `false` | Start every new conversation in plan mode. |
| `chat.tool_turn_temperature` | float | not set | Model temperature of a turn that offers tools; none means 0.3. |

### `[models]`

The signed index of model cards, its mirror, the models folder, and the rule applied to an imported model file.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `models.registry_url` | string | `"https://registry.apollia.fr/models/stable/index.json"` | Address of the signed index of model cards. The organization policy can fix it. |
| `models.registry_mirror_url` | string | `"https://github.com/Apollia-OS/apollia-os/releases/latest/download/models-stable-index.json"` | Address tried when the index does not answer; empty disables it. The organization policy can fix it. |
| `models.registry_enabled` | boolean | `true` | Read the online index of model cards. The organization policy can fix it. |
| `models.dir` | path | `""` | Folder the language models, their projectors and the embedding models are kept in; empty means &lt;data dir&gt;/models. Changing it moves no file. The organization policy can fix it. |
| `models.import.require_verified` | boolean | `false` | Refuse an imported model file that matches no signed card. The organization policy can fix it. |

### `[workflows]`

The budget a workflow run starts with. It is shared: every agent a step runs and every workflow a step calls spend from it.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows.max_steps` | integer | `200` | Steps a workflow run may take. |
| `workflows.max_tool_calls` | integer | `400` | Tool calls a workflow run may make. |
| `workflows.wall_clock_secs` | integer | `3600` | Seconds a workflow run may last. |

### `[desktop]`

Settings of the desktop application that the engine reads.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `desktop.developer_mode` | boolean | `false` | Show the advanced surfaces of the application. The organization policy can fix it. |
| `desktop.link_preview_enabled` | boolean | `false` | Fetch the preview of a link shown in a conversation. The organization policy can fix it. |
| `desktop.update_channel` | one of stable, preview | `"stable"` | Update channel of the application: stable receives the published versions, preview the trial versions as well. The organization policy can fix it. |
| `desktop.background_service_allowed` | boolean | `true` | Allow the application to install the engine as a service of the user's session. The organization policy can fix it. |

### `[server]`

The compatible server, which answers the OpenAI and Anthropic formats on the loopback interface.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `server.enabled` | boolean | `false` | Start the compatible server (OpenAI and Anthropic formats) with the engine. The organization policy can fix it. |
| `server.port` | integer | `7772` | TCP port of the compatible server. The organization policy can fix it. |
| `server.require_auth` | boolean | `false` | Require a token of the compatible server on every request; read when the listener opens. The organization policy can fix it. |
| `server.serve_on_lan` | boolean | `false` | Open the compatible server to the local network, on every interface; the native API stays on the loopback interface. Read when the listener opens. The organization policy can fix it. |

### `[engines]`

Settings of the local inference engines.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `engines.log_level` | one of error, warning, info, debug | `"debug"` | Log level of the local engine, applied when a model is next loaded. The organization policy can fix it. |

### `[author]`

The publisher name of the agents exported from this machine.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `author.name` | string | `""` | Publisher name of the agents exported from this machine; empty asks at export. The organization policy can fix it. |

### `[observability]`

Trace capture and retention. Read by the daemon and by the desktop application.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `observability.max_input_bytes` | integer | `32768` | Largest run input kept, in bytes. |
| `observability.max_output_bytes` | integer | `32768` | Largest run output kept, in bytes. |
| `observability.max_tool_output_bytes` | integer | `10240` | Largest tool output kept, in bytes. |
| `observability.capture_thoughts` | boolean | `true` | Keep the model's reasoning in the run history. |
| `observability.capture_tool_args` | boolean | `true` | Keep tool arguments in the run history. |
| `observability.capture_tool_outputs` | boolean | `true` | Keep tool outputs in the run history. |
| `observability.capture_agent_logs` | boolean | `true` | Keep agent logs in the run history. |
| `observability.retention_days` | integer | `90` | Days the run history is kept. |

### Withdrawn keys

Accepted in a file written for an earlier version, with a `config.key.retired` warning, and read by nothing.

| Key | Instead |
| --- | --- |
| `llm.observability.log_token_usage` | The tokens of every model call are always logged; remove the key. |
| `llm.observability.log_latency` | The latency of every model call is always logged; remove the key. |
| `llm.observability.log_cost` | Nothing reads it; remove the key. |
| `llm.pricing_overrides` | Nothing reads it; remove the section. |
| `llm.cost_alert_threshold_usd` | Nothing reads it; remove the key. |
| `llm.runner` | The engine is chosen from the detected hardware; remove the section. |
| `llm.vertex` | The Google Vertex AI client was removed; declare an OpenAI-compatible server instead. |
| `memory` | Nothing reads it; remove the section. |
| `budget` | Nothing reads it; remove the section. |
| `a2a` | Nothing reads it; remove the section. |
| `oria` | Nothing reads it; remove the section. |
| `registry` | Nothing reads it; remove the section. |
| `permissions` | Nothing reads it; remove the section. |
| `triggers` | Use the desktop app or the API to manage it. |
| `notifications` | Use the desktop app or the API to manage it. |
| `stt` | Use the desktop app or the API to manage it. |
| `tools.file_path_extraction_pattern` | Nothing reads it; remove the key. |
| `filesystem` | The file tools reach the folders granted to each agent; the paths listed here were added once as folder connections, grant them on the agent's sheet. |
| `chat.default_workspace` | The folder was added once as a folder connection; grant it to an agent on its sheet. |
| `ui` | Moved: write desktop.link_preview_enabled; ui.link_preview_enabled is still read while it is absent. |
<!-- END GENERATED: config-fields -->

### The Unix socket, in full

The default of `[api].unix_socket` is `~/.apollia/runtime.sock`, and a socket
under the platform temporary directory when no home directory can be resolved.
The key does not expand `~`, so the default file writes the absolute path of
the socket under the home folder it was written in. The server sets the file to
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

### The folders an agent reaches

No key of this file says which folders an agent reaches. A folder of the
machine is a connection, added from the Connections screen with the system
picker, and its file tools are granted to an agent on the agent's sheet. The
file tools of an agent reach the union of the folders granted to it, the
built-in chat agent included: the reading tools every granted folder, the
writing tools only the folders granted for writing. The first folder granted,
by the date it was added, is where a relative path lands. An agent granted no
folder has file tools that refuse, saying how to grant one.

`[filesystem] trusted_paths` and `[chat] default_workspace` used to name these
folders. Both are withdrawn. At the first start of a version that knows folder
connections, each folder they named, the home folder excepted, was added once
as a connection, granted to no agent; a file that still carries them loads with
a `config.key.retired` warning.

### Sections that were withdrawn

`[a2a]`, `[oria]`, `[registry]`, `[permissions]`, `[memory]` and `[budget]` used
to be accepted. Each deserialized into a typed structure that nothing then
consulted, so writing a value there had no effect and produced no error either.
They are read by nothing now: a file that still carries one is accepted, with a
`config.key.retired` warning, and the table of withdrawn keys above names what
to do instead. Removing them changed no behaviour, because they had none.

Some keys were withdrawn for the same reason, because nothing read them. A file
that still carries one is accepted, with a `config.key.retired` warning naming
it, and `apollia-os config set` refuses to write it:

| Withdrawn key | What to do instead |
| --- | --- |
| `[llm.runner]`, and its `backend` key | Nothing: the speech-to-text runner is chosen from the detected hardware. |
| `[llm] pricing_overrides`, `cost_alert_threshold_usd` | Nothing: no running daemon applied them. `apollia-os llm costs` reads the cost history and no longer writes either. |
| `[llm.observability] log_token_usage`, `log_latency`, `log_cost` | Nothing: tokens and latency are always logged. |
| `[tools] file_path_extraction_pattern` | Nothing. |
| `[filesystem.journal]` | Nothing. |
| `[ui] link_preview_enabled` | Write `[desktop] link_preview_enabled`. Until it is written, the former key is still read, with a `config.key.moved` warning. |

`[autonomy]` is not a section: no loader reads it, and a file that carries one
is refused as an unknown key.

`[permissions]` is worth spelling out, since its name suggests otherwise: not
one of its four keys ever had a reader on an execution path. The governance that
does run, prefix rules and approvals, takes nothing from this section. See
[cross-cutting concepts](/architecture/crosscutting-concepts).

Sampling parameters are documented separately in
[Sampling defaults](/reference/sampling-defaults). The `[tools.web_search]` and
`[tools.web_read]` keys are also editable from the CLI with
`apollia-os tools config set <tool>.<key> <value>`.

## Trace capture (`[observability]`)

Read by the daemon started with `apollia-os start` and by the desktop
application. Editable from Settings, Observability, with
`apollia-os config set observability.<key> <value>`, or by hand.

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

The section is read when the runtime builds its router, so a change applies at
the next start. A `debug_log_prompt` written under `[observability]` is refused
as an unknown key: the two sections are two different types, and only the
`[llm.observability]` one reaches the router. The other keys that section used
to carry, `log_token_usage`, `log_latency` and `log_cost`, are withdrawn: the
tokens and the latency of every model call are always logged.

### Keys that are declared but not implemented

| Key | State |
|---|---|
| `capture_thoughts` on ORIA plan steps | Partial. Plan-step inputs and outputs are persisted with the compiled defaults, so the byte limits above do not apply to them. |
| `max_tool_output_bytes` | **Not implemented.** Never had a use site. |

These are listed rather than hidden because the settings page still shows some of
them. A switch that looks like a privacy control and does nothing is worse than
an absent one, so until they are implemented or removed, this table is the
authority.

## Load guardrails (`[llm.guardrails]`)

Before a local model is launched, its memory footprint is estimated with the
context window and the number of parallel slots it would really run with, and
graded against this machine: it fits, it might fit (split between a graphics
card and system memory), or it is too large. The level decides what is launched.

```toml
[llm.guardrails]
level = "balanced"   # disabled, lenient, balanced, strict or ceiling
ceiling_gb = 12.0    # read by the ceiling level only
```

| Level | Launches |
| --- | --- |
| `disabled` | every model, without an estimate |
| `lenient` | every model, with a warning in the log past "might fit" |
| `balanced` | up to "might fit". The default |
| `strict` | only a model that fits |
| `ceiling` | a model estimated at `ceiling_gb` gigabytes or less |

The section is read at every launch, so a change applies to the next model
loaded, without a restart. A model already loaded is not stopped. A refusal
names the estimate and the limit: `POST /api/v1/llm/chat` and
`POST /api/v1/llm/complete` answer it with status 409, the code
`guardrail_refused`, the level, `estimatedGb` and `budgetGb`. A section that cannot be read, an unknown
level, or a `ceiling` level without a positive `ceiling_gb` holds to `balanced`:
a typo never turns the guardrail off. A model whose footprint cannot be
estimated is launched.

Several models can stay loaded at once, as many as the guardrail admits: a
model is judged with the footprints of the models already loaded added to its
own. When that sum is refused, the loaded models nothing keeps (no open
conversation, no pin, no answer in progress) are unloaded, least recently used
first and only as many as needed; when even that would not be enough, nothing
is unloaded and the load is refused.

## Model residency (`[llm.residency]`)

A local model nothing uses gives its memory back on its own.

```toml
[llm.residency]
idle_unload_minutes = 60   # 0 keeps an unused model loaded
```

| Rule | Unloads |
| --- | --- |
| Idle delay | a model no request has used for `idle_unload_minutes`, checked every thirty seconds |
| Last conversation closed | a model at once, when the last open conversation that uses it closes |

A model the user pinned is unloaded by neither rule, and a model answering a
request is never unloaded. The pin is kept in `system.db` and survives a
restart. The key is read at every check, so a change applies without a restart.
An interface says which conversations are open with
`POST /api/v1/llm/leases` and `POST /api/v1/llm/leases/release`, and pins a
model with `PUT /api/v1/llm/pins`. When the event stream it opened with
`GET /api/v1/events?client=<id>` ends, the conversations of that client count
as closed. Each unload is published on the event bus as `LlmModelUnloaded`,
with its reason: `idle`, `last_conversation`, `requested` or `make_room`.

## Model defaults (`[llm.defaults]`, `[llm.images]`)

What a local model receives when nothing closer to it says otherwise. The
application shows these under Settings, Model defaults.

```toml
[llm.defaults]
context = "custom"       # or "model_max"
context_tokens = 16384   # 0 lets the engine size the window to this machine

[llm.images]
limit_side = true
max_side_px = 2048

[llm.guardrails]
bypass = "confirm"       # or "unrestricted"
```

The context window of a launch is the first of these that speaks:

| Order | Source |
| --- | --- |
| 1 | `APOLLIA_LLAMA_N_CTX` |
| 2 | the window of the model's preset |
| 3 | the `context_window` of the backend |
| 4 | `[llm.defaults]` |
| 5 | 32768 tokens, the value the engine is built with |

With `context = "custom"` and a figure, that figure applies. With
`context_tokens = 0`, the default, the engine sizes the window model by model:
the longest one, up to 32768 tokens and never under 4096, at which the estimate
used by the load guardrail still holds the model within the memory of the
machine. With `context = "model_max"`, the window is the length the model was
trained for. Every one of them is capped at the training length, and a model
whose header cannot be read falls to the last line of the table. The section is
read at every load, so a change applies to the next model loaded.

`[llm.images]` is read by the chat of the application: an image whose largest
side exceeds `max_side_px` is scaled down to it, proportions kept, before it is
sent to a model that reads images. With `limit_side = false` an image travels
at its own size, up to what the engine accepts.

`bypass` says what the application asks before it loads a model the guardrail
refused: `confirm` shows a confirmation that recalls the risk, `unrestricted`
loads at once. The guardrail itself is not weakened: the refusal is still
shown, and a request that does not confirm is still refused.

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

### `tool_families`

The action family of each tool, keyed by the tool's name on the server. The
desktop writes it when a connection is added from the curated selection; leave
it out for any other server.

- Type: table of tool name to family
- Families: `read`, `read_personal`, `write_or_delete`, `send_external`,
  `execute_code`
- Default: empty
- Bounds: at most `8192` entries, each key a valid tool name, no
  `unclassified` value

A tool listed here takes this family. A tool left out is classified from the
hints its server declares (`readOnlyHint`, `destructiveHint`): read-only reads,
or reads personal data when the connection holds an OAuth token or a stored
secret; destructive, or declared not read-only, writes or deletes. A tool with
no hint is unclassified and treated as sensitive. The hints come from an
untrusted server: they classify a tool but never let it run as read-only, and
no hint can classify a tool as sending outside the machine.

```toml
[[mcp.servers]]
name = "notion"
transport = "streamable-http"
url = "https://mcp.notion.com/mcp"

[mcp.servers.tool_families]
notion-search = "read_personal"
notion-create-pages = "write_or_delete"
```
