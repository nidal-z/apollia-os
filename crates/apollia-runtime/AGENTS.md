# crates/apollia-runtime/AGENTS.md

> Local rules for the runtime crate. Read after the root `AGENTS.md` and
> before editing this crate. Pair with `docs/agents/ARCHITECTURE.md` §C and
> `docs/agents/RUST-PATTERNS.md` §2.

`apollia-runtime` is the actor mesh : `EventBus`, `AgentRegistry`,
`TaskRouter`, `Supervisor`, `APIServer`, `LlmRouter` host, and the HTTP
endpoints. Most of the project's day-to-day Rust changes land here.

---

## 1. The actor mesh

| Actor | Owns | Channel kind | Capacity | Source |
|---|---|---|---|---|
| `Supervisor` | ordered startup, shutdown rollback, watchdog | no channel of its own; awaits `ShutdownRequested` on the bus | n/a | `src/supervisor/mod.rs` |
| `EventBus` | broadcast of `RuntimeEvent` | broadcast | 1024, bounds `[64, 65536]` | `src/eventbus.rs` |
| `AgentRegistry` | inventory of agents, manifests, state | mpsc | 256 | `src/registry.rs` |
| `TaskRouter` | dispatches tasks to agents | mpsc | given by the caller, 256 recommended | `src/router.rs` |
| `ExecutionCoordinator` | per-agent ORIA driver | no channel; a `Semaphore` bounds concurrency | n/a | `src/coordinator.rs` |
| `APIServer` | axum router, HTTP requests | tokio task per request | n/a | `src/api/server.rs` |
| `ChatSessionManager` | chat sessions + FTS5 persistence | mpsc | 256 | `src/chat/manager/handle.rs` |
| `PlanActor` | the chat plan, per session | mpsc | 64 | `src/chat/plan_actor.rs` |
| `TodoActor` | the chat todo list, per session | mpsc | 64 | `src/chat/todo_actor.rs` |
| `AgentMailboxHandle` | durable inter-agent inbox | mpsc | 256 | `src/mailbox.rs` |
| `SttEngine` | whisper audio pipeline | mpsc | 32 | `src/stt/engine.rs` |
| `spawn_state_recorder` | the last state of each connection in `system.db` (`connection_states`): turns each `McpServerHealthChanged` into one of the four states, keeps it with its date, announces it as `ConnectionStateChanged`; spawned before the MCP manager starts, stops on `ShutdownRequested` | no channel of its own; a bus subscriber, one change at a time | n/a | `src/connections/state.rs` |
| `TimeoutWatcher` | expiry of `input_required` tasks | no channel; a `tokio::time::interval` scan loop | n/a | `src/timeout_watcher.rs` |
| `BatchActor` | progress of every running batch: starts items up to their concurrency, counts their ends, interrupts past the failure threshold, pauses, resumes, stops and retries a batch, records and withdraws its grants, publishes waiting items and the end of a batch, resumes running batches at start | two mpsc | 64 commands, 256 item ends | `src/batch/actor.rs` |
| `CompatServerHandle` | the compatible server's listener: opens it at start when `[server] enabled` says so, starts, stops and restarts it on request (binding the loopback interface, or every interface when `[server] serve_on_lan` says so) and writes the switch, keeps the last start error, closes it on `ShutdownRequested` without touching the switch; its routes run an exposed agent as a batch of one item through `BatchActor`, and an exposed workflow through `WorkflowActor`, and stop the run when the client goes away; each request it receives leaves a `ServerCall` entry in the audit journal, through the journal's own handle (`src/compat_server/journal.rs`) | mpsc | 64 | `src/compat_server/actor.rs` |
| `GovernanceActor` | what every tool call is decided from: the organisation policy read at boot, the fine rules and standing consents of `governance.db`, the developer mode; publishes it as one state on a `watch` channel that a decision reads without waiting, writes an "always" answered at a prompt as a standing consent, rereads the files when another process changed them | mpsc | 64 | `src/governance/actor.rs` |
| `ServerTokensHandle` | the tokens of the compatible server in `system.db`: creates one (`apo_` value answered once, only its SHA-256 kept), lists and revokes them, counts those in use, checks a presented token in constant time and stamps its last use, names the token a request presented for the journal; spawned by `CompatServerHandle`, stops when its handles drop | mpsc | 256 | `src/compat_server/tokens.rs` |
| `ServerLogsHandle` | the logs of the compatible server: a ring of the last 2000 lines (one per request it answered, one per line a local `llama-server` printed), oldest out first; streams them to each subscriber at its level, then the new ones, through a bounded channel of 256 that drops what a slow reader cannot hold; clears the ring and nothing else; writers never wait (`try_send`); spawned by `CompatServerHandle`, stops on `ShutdownRequested` | mpsc | 1024 | `src/compat_server/logs.rs` |
| `ConfigReloadHandle` | the configuration in force: applies a saved or changed `apollia.toml`, sends its live sections on one `watch` channel each, lists the keys waiting for a restart, publishes `ConfigReloaded` and `ConfigInvalid`, journals each change with its origin; stops on `ShutdownRequested` | mpsc | 16 | `src/config_reload/actor.rs` |
| `WorkflowActor` | the workflow runs in progress and their cancellation tokens: launches a run after checking its inputs, stops one and answers once it ended, fails at start the runs a stopped process left running, takes the workflows of an uninstalled package out of its catalogue; keeps the questions runs asked the user (`ctx.user.ask`) with the channel of each answer, lists them, hands an answer or a refusal to the run that waits and forgets the questions of a run that ended or stopped; each run reports its steps itself, to `hitl.db` and the bus | mpsc | 64 | `src/workflows/actor.rs` |

Hosted from other crates, started by this one :

| Actor | Owns | Channel kind | Capacity | Source |
|---|---|---|---|---|
| `TriggerEngine` | cron + filewatch + webhook -> task spawn | two mpsc | 256 events, 64 commands | `crates/apollia-triggers/src/engine.rs` |
| `NotificationEngine` | desktop notify + webhook | mpsc | 8 | `crates/apollia-notifications/src/engine.rs` |
| `LlmRouter` | backend selection per agent | none; a plain struct, called synchronously | n/a | `crates/apollia-llm/src/router.rs` |

Capacities are sized for the steady state. Burst absorption relies on
backpressure surfacing as `try_send` errors that callers handle.

If you add a new actor, document it here with its responsibility,
channel kind, default capacity, and the events it consumes or
produces.

---

## 2. EventBus contract

`broadcast::Sender<RuntimeEvent>`. Past-tense variant names. Carry typed
fields. Never blob `String`.

Source : `src/eventbus.rs`. Capacity validated in `[64, 65536]`, default
1024 via `EventBus::new()`. Override via `EventBus::with_capacity(n)`.

Lag handling : subscribers receive `Lagged(n)` from
`broadcast::Receiver::recv`. The rule is log at `WARN`, `resubscribe()`,
continue, never panic, and it lives in one place:
`apollia_core::events::subscribe_resilient` (or `resilient`, when the
receiver is handed in). Subscribe through it; do not restate the rule at a
call site. The server-sent-event routes are the named exception, since a
`BroadcastStream` owns its receiver and cannot resubscribe; they keep the
`WARN`. `scripts/check_eventbus_variants.py` holds both halves.

Adding a new variant is a wire-format change, and so is removing one. The
catalogue is published at `docs/site/docs/reference/events.md`, generated by
`docs/site/scripts/gen_events_ref.py` from the enum and from the desktop
bridge's `categorize`; run `docs/site/regen.sh` after the change. Give the
variant a category the interface already reads, or add the listener in the
same commit: a variant in an unread category reaches the webview and is
dropped, which no compiler and no test can see.

---

## 3. HTTP API

axum router. Routes split by domain in `src/api/routes_*.rs`.

```
src/api/
├── mod.rs                    # router assembly
├── routes_agents.rs          # /api/v1/agents/*
├── routes_tasks.rs           # /api/v1/tasks/*
├── routes_batches.rs         # /api/v1/batches/*
├── routes_workflows.rs       # /api/v1/workflows/*, /api/v1/workflow-runs/*
├── routes_tools.rs           # /api/v1/tools/*
├── routes_triggers.rs        # /api/v1/triggers/*
├── routes_notifications.rs   # /api/v1/notifications/*
├── routes_messages.rs        # /api/v1/messages/*   (inter-agent mailbox)
├── routes_mcp.rs             # /api/v1/mcp/*
├── routes_audit.rs           # /api/v1/audit/*
└── routes_chat.rs            # /api/v1/chat/*
```

The module names are plural (`routes_agents.rs`, not `routes_agent.rs`).
The list above is representative, not exhaustive : `src/api/` also holds
`routes_a2a`, `routes_approvals`, `routes_llm`, `routes_review`,
`routes_stt`, `routes_timeline`, `routes_webhooks`, and others. There is
no `routes_auth.rs`; auth is enforced in middleware, not a route module.

Rules :
- Route style : RESTful, plural resource, versioned under `/api/v1/`
  (`/api/v1/agents`, `/api/v1/tasks/{id}`).
- JSON wire : `camelCase` via `#[serde(rename_all = "camelCase")]`.
- Bind targets : Unix socket (`~/.apollia/runtime.sock`, from
  `apollia_core::paths::socket_path_or_temp`) and TCP 7771. Both served by
  the same router.
- Authentication : implicit local trust on the Unix socket; TCP requires the
  local bearer token, generated once and read from `<data_dir>/api-token`
  with mode `0600` (`load_or_generate_token`, `src/api/middleware.rs`).
  There is no `auth.toml`.
- Error responses : `{"error": {"code": "...", "message": "...", "details": ...}}`,
  HTTP status reflecting the error class.

Exemption, the compatible endpoints. The compatible server is a second
listener on its own port (`[server] port`, 7772 by default), separate from the
native API above, and the paths it serves are not Apollia's: they follow the
OpenAI and Anthropic formats, so they live under `/v1/...`, use those APIs'
`snake_case` field names, and answer errors in the envelope of their family:
OpenAI's `{"error": {"message", "type", "code"}}`, or Anthropic's
`{"type": "error", "error": {"type", "message"}}` on `/v1/messages` and
`/v1/messages/count_tokens` (`src/compat_server/errors.rs`). They serve the
local models only, relayed to the engine that runs them; a remote backend is
never relayed. A response or a message that brings an MCP server of its own
(`[server] allow_request_mcp`, off by default) is not relayed either: the
runtime connects to the server over HTTP for the length of the call, through
`McpClientManagerHandle::add_request_server`, and runs the tool loop through
the agent executor under the agent `request-mcp`, which is stored nowhere
(`src/compat_server/request_mcp_run.rs`). Its tools are unclassified, the run
is unattended, and the connection is removed when the call ends or its caller
goes away. Their authentication is
not the native API's: with `[server] require_auth`, a request presents a token
of the server (`apo_...`, `Authorization: Bearer` or `x-api-key`), and the
internal token never opens them. Each request carries its `CallOrigin`
(loopback or local network, read from the peer address at accept), and a call
to an agent or a workflow from the local network goes through
`admit_agent_call`, which wants a token in use whatever `require_auth` says.
The organisation policy closes three of their settings and only closes
(`serve_on_lan`, `require_auth`, `allow_request_mcp`): the actor and
`LocalModels` resolve them under the `PolicySettings` read at boot, never under
`PolicySettings::none()`, and a server the file had left open against the policy
leaves one `PolicySettingEnforced` entry per key (`src/compat_server/policy.rs`).
None of the rules of this section
applies to them. They live in `src/compat_server/`, never in `src/api/` and
never under `/api/v1/`, so `scripts/check_openapi_routes.py` does not read them
and `clients/openapi.json` does not declare them. Their contract is the public
page `docs/site/docs/reference/compatible-endpoints.md`, and
`scripts/check_compat_routes.py` holds every path the module's router serves
against that page's table, in both languages. Only the routes that start, stop, restart and
read the server, and those that create, list and revoke its tokens, live in the
native API (`src/api/routes_server.rs`, `/api/v1/server`), under its rules.

Adding a route :
1. Define the request/response types in the same `routes_*.rs` file.
2. Implement the handler.
3. Mount in `mod.rs`.
4. Add an `axum-test` integration test.
5. Annotate the handler with `#[utoipa::path]` and regenerate
   `clients/openapi.json`; `scripts/check_openapi_routes.py` refuses a route
   the spec omits and a spec path no router registers. The HTTP reference of
   the documentation site is generated from that file by `docs/site/regen.sh`,
   so there is no page under `docs/site/docs/reference/` to edit by hand.
6. Add CLI consumption if applicable.

---

## 4. Supervisor and shutdown

The `Supervisor` (`src/supervisor/`) starts the actors in a strict sequence
(`EventBus` -> `AgentRegistry` -> `ToolRegistry` -> `TaskRouter` -> `APIServer`),
and if one fails to start it stops the previous ones in reverse order. After
startup the model is fail-fast then degrade: there is no restart-on-crash, and
a crashed actor leaves the runtime degraded until an explicit shutdown. The
inference sidecar is the exception, restarted by its own `RunnerSupervisor`.

Shutdown is driven by `ShutdownController` (`src/shutdown.rs`). Sources :

1. SIGINT / SIGTERM caught by the signal listener.
2. `RuntimeEvent::ShutdownRequested` from another actor, or from the
   `POST /api/v1/shutdown` handler (`src/api/server/router.rs`).
3. `apollia-os stop`, which calls that route.

Sequence, in the order `src/shutdown.rs` implements it :

```
broadcast ShutdownRequested
  -> the API server stops accepting connections
  -> in-progress tasks drain (ShutdownConfig::drain_timeout_secs, default 30)
  -> the remaining ones are cancelled and logged
  -> agents transition to Stopped
  -> the NotificationEngine stops
  -> the MCP client manager stops (child processes killed, no zombies)
  -> TaskRouter then AgentRegistry stop
```

Rules :
- Every actor honors the shutdown signal. No exceptions.
- The drain timeout is a bound on waiting, not an abort of the process: there
  is no `JoinSet::abort_all` and no dedicated shutdown exit code. Exit code 5
  is the CLI's interrupt code, not the daemon's.
- Cleanup that must complete during shutdown runs before the drain, inside the
  controller, not in an actor's own select arm.

## 5. Persistence

SQLite + `rusqlite` + FTS5. WAL journal mode.

Databases : one file per subsystem under `~/.apollia/`, catalogued by
`apollia_core::paths::DataFile`, whose `file_name()` is the single source for
the names. There is no `runtime.db`. The catalogue holds `agents.db`,
`artifacts.db`, `audit_journal.db`, `chat.db`, `governance.db`,
`hitl.db`, `llm_calls.db`, `mailbox.db`, `mcp.db`,
`notifications.db`, `plan_cache.db`, `plans.db`, `projects.db`,
`runtime_events.db`, `sidechains.db`, `stt_transcriptions.db`, `system.db`,
`triggers.db`, `triggers_def.db` and `user_memory.db`. Per-agent memory lives
outside the catalogue, one file per namespace under `~/.apollia/memory/`
(`apollia-memory`).

Adding a database means adding a `DataFile` variant, not joining a literal
onto the data directory. `scripts/check_data_layout.py` holds that rule.

Migration pattern : `apollia_core::schema::open_versioned(conn, name,
schema_version, migrations)` at first connection. It reads `PRAGMA
user_version`, refuses a file written by a newer binary
(`SchemaError::NewerThanBinary`), refuses a declared version that does not
match the number of migrations supplied
(`SchemaError::MigrationCountMismatch`), and applies the missing ones in
order. A schema change is a new `Migration` appended plus the version
bumped, never an edit of an existing one. Never `DROP COLUMN` without an
explicit upgrade path. `scripts/check_sqlite_schema_versioning.py` holds the
rule.

Permission invariant (chat manager) : a code executor (`bash_executor`,
`python_executor`, i.e. `apollia_permissions::CODE_EXECUTOR_TOOLS`) is never
blanket-authorized by name. The chat "always allow" path
(`src/chat/manager/libre.rs`, `src/chat/manager/exchange/tool_decision.rs`,
`src/chat/manager/exchange/outcome.rs`, `src/chat/manager/exchange/dispatch.rs`)
must not persist a no-prefix allow rule for one, seed it into
`pre_authorized_tools`, or insert it into `session.authorized_tools`; each
invocation keeps its per-call HITL. The matching side of the same invariant
lives in `apollia-permissions` (`prefix_rule_engine`). When adding a new
arbitrary-code executor tool, add its name to `CODE_EXECUTOR_TOOLS` (a
drift-guard test in `apollia-tools` checks the descriptor names stay in sync).

Connections : no pool crate. Each SQLite-backed actor owns a single
`rusqlite::Connection` on its own thread and serializes access through its
message loop; one-shot handlers open a fresh `Connection` inside
`tokio::task::spawn_blocking`. Never hold a connection across an `await`.

FTS5 : used for chat session summaries and memory recall. The match
syntax follows SQLite's FTS5 (`tag:value`, `+required`, `-excluded`).

---

## 6. Configuration

Runtime config : `apollia.toml`, resolved as an explicit `--config` override,
otherwise `apollia_core::paths::config_file()`, that is
`~/.apollia/apollia.toml`, the one file every surface reads. The working
directory and `~/.config/apollia/` are never read. Parsed via `serde` into the
`RuntimeConfig` struct (`crates/apollia-core/src/config/runtime.rs`).

There is no `SIGHUP` handler. The runtime follows `apollia.toml` itself:
`ConfigReloadHandle` (`src/config_reload/`) holds the last valid
configuration, reads a text the save route wrote or the file changed on disk
(`ApolliaConfigWatcher`, `apollia-triggers`), and sends each changed section
whose keys are `Reloadable` in the key registry on a `tokio::sync::watch`
channel of its own. A consumer holds that section as a `Live<T>`: it reads
`get()` at each use, or applies `changed()` between two messages. The keys of
class `Restart` wait for the next start and are listed by `GET /api/v1/config`.
Each text applied is written to the signed journal as `ConfigurationChanged`
(keys, origin, fingerprints, never a value), once the supervisor has handed
the journal to the reloader with `attach_journal`; a code path that writes the
file itself shows the written text to `ConfigReloadHandle::saved` with its
origin, otherwise the watcher finds it and the journal says `external_file`.
A new consumer of a live section takes a `Live<T>` (a plain value converts,
for the tests), never a copy made at boot. The subsystems with a reload of
their own keep it: `POST /api/v1/triggers/reload` (503 when the runtime was
started without a config file), the STT reload route and the desktop reload
command, both swapping a shared cell rather than restarting the actor.

Inter-agent mailbox caps live on `RuntimeConfig` as `mailbox_*` fields,
mapped to `MailboxConfig` at boot :

| Field | Meaning | Default |
|---|---|---|
| `mailbox_capacity` | max queued messages per inbox | 100 |
| `mailbox_visibility_timeout_secs` | lease before an unacked message is redelivered | 60 |
| `mailbox_message_ttl_secs` | message time-to-live | 86400 |
| `mailbox_send_quota_per_run` | max sends per agent run | 50 |
| `mailbox_max_payload_bytes` | payload cap per message | 65536 |
| `mailbox_audit_full_payload` | journal the full payload vs a redacted snapshot | false |

Rules :
- Never read `APOLLIA_*` env vars in production code for raw secrets.
  Selectors only.
- Defaults documented in `docs/site/docs/reference/`.
- Validation runs at startup. Invalid config fails fast.

---

## 7. Forbidden in this crate

- `Arc<Mutex<T>>` shared across actors. Messages or nothing.
- Calling into `apollia-cli`. The CLI calls us, not the reverse.
- Holding a SQLite connection across an `await`. Use `spawn_blocking`;
  there is no pool crate to acquire from.
- Direct PyO3 calls. Route through `apollia-aip`.
- Publishing events without a corresponding handler test.

---

## 8. Testing

- Unit tests inline per file, GIVEN/WHEN/THEN.
- Integration tests in `tests/` exercise the actor wiring with a real
  EventBus.
- HTTP : the route tests build the `Router` in-process and drive it with
  `tower::ServiceExt::oneshot`, without a real bind. `axum-test` is not a
  dependency of this crate, so there is no `TestServer`; each `routes_*.rs`
  carries its own `test_router()` helper instead.
- Each actor has a test that drives it through its lifecycle :
  spawn, normal traffic, shutdown.
- Actor concurrency invariants (registry eviction, coordinator semaphore,
  router terminal-status guard, mailbox lease, plan-gate resume, shutdown
  drain/latch) have abstract Loom models in the standalone
  `crates/apollia-loom-models` crate. They prove the algorithm, not the exact
  Tokio code; each cites the prod `file:line` it mirrors. See
  `docs/agents/TESTING.md` section 8c. Never add `loom` to this crate:
  `--cfg loom` poisons Tokio.
- Timing-dependent tests use bounded poll-until-condition or event-driven
  awaits, never a fixed `sleep` to "wait for propagation".

---

## 9. When the rules block you

- New actor that wants shared state with another : you almost
  certainly want a third actor that owns the state, not a shared
  `Arc<Mutex>`.
- HTTP endpoint that needs to call into another endpoint : refactor
  the shared logic into a service module and call from both handlers.
- Performance issue with a bounded channel : measure first. If the
  channel is genuinely undersized, document the new capacity in this
  file before changing it.
