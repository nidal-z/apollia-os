# crates/apollia-runtime/AGENTS.md

> Local rules for the runtime crate. Read after `docs/agents/INDEX.md` and
> before editing this crate. Pair with `docs/agents/ARCHITECTURE.md` §C and
> `docs/agents/RUST-PATTERNS.md` §2.

`apollia-runtime` is the actor mesh : `EventBus`, `AgentRegistry`,
`TaskRouter`, `Supervisor`, `APIServer`, `LlmRouter` host, and the HTTP
endpoints. Most of the project's day-to-day Rust changes land here.

---

## 1. The actor mesh

| Actor | Owns | Channel kind | Default capacity |
|---|---|---|---|
| `Supervisor` | the JoinSet of all actors, shutdown coordination | mpsc | 32 |
| `EventBus` | broadcast of `RuntimeEvent` | broadcast | 1024 |
| `AgentRegistry` | inventory of agents, manifests, state | mpsc | 1024 |
| `TaskRouter` | dispatches tasks to agents | mpsc | 1024 |
| `ExecutionCoordinator` | per-agent ORIA driver | mpsc | 256 |
| `APIServer` | axum router, HTTP requests | tokio task per request | n/a |
| `LlmRouter` | backend selection per agent | mpsc | 256 |
| `TriggerEngine` | cron + filewatch + webhook -> task spawn | mpsc | 256 |
| `NotificationEngine` | desktop notify + webhook | mpsc | 256 |
| `ChatSessionManager` | chat sessions + FTS5 persistence | mpsc | 256 |
| `SttEngine` | whisper audio pipeline | mpsc | 64 |
| `AuditTrail` | append-only event ledger | mpsc | 1024 |
| `TimeoutWatcher` | per-task timeouts | mpsc | 256 |

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
`broadcast::Receiver::recv`. Subscribers must log at `WARN`,
`resubscribe()`, and continue. Never panic on lag.

Adding a new variant is a wire-format change. Document it in
`docs/wiki/Reference-EventBus.md` (post-L2b) and bump the
`EVENTBUS_SCHEMA_VERSION` constant.

---

## 3. HTTP API

axum router. Routes split by domain in `src/api/routes_*.rs`.

```
src/api/
├── mod.rs              # router assembly
├── routes_agent.rs     # /agent/*
├── routes_task.rs      # /task/*
├── routes_tool.rs      # /tool/*
├── routes_trigger.rs   # /trigger/*
├── routes_notify.rs    # /notify/*
├── routes_auth.rs      # /auth/*
├── routes_mcp.rs       # /mcp/*
├── routes_audit.rs     # /audit/*
└── routes_chat.rs      # /chat/*
```

Rules :
- Route style : `resource/verb`, singular, lowercase
  (`/agent/list`, `/task/read`).
- JSON wire : `camelCase` via `#[serde(rename_all = "camelCase")]`.
- Bind targets : Unix socket (`~/.apollia/runtime.sock`) and TCP 7771.
  Both served by the same router.
- Authentication : implicit local trust on Unix socket; TCP requires
  the local bearer token from `~/.apollia/auth.toml`.
- Error responses : `{"error": {"code": "...", "message": "...", "details": ...}}`,
  HTTP status reflecting the error class.

Adding a route :
1. Define the request/response types in the same `routes_*.rs` file.
2. Implement the handler.
3. Mount in `mod.rs`.
4. Add an `axum-test` integration test.
5. Document in `docs/wiki/Reference-API.md`.
6. Add CLI consumption if applicable.

---

## 4. Supervisor and shutdown

The `Supervisor` actor owns the `JoinSet` of all spawned tasks and a
`CancellationToken`. Shutdown sources :

1. SIGINT / SIGTERM caught by the signal listener.
2. `RuntimeEvent::ShutdownRequested` from another actor.
3. External RPC : `apollia runtime stop`.

Sequence :

```
cancel.cancel()
  -> every actor's select! shutdown branch wins
  -> each actor flushes its outbound queue
  -> actor's run() returns
  -> JoinSet collects the JoinHandle
  -> Supervisor awaits all
  -> exit
```

Rules :
- Every actor honors the `CancellationToken`. No exceptions.
- Cleanup work that must complete during shutdown runs under
  `cancel.run_until_cancelled(...)` so it cannot be cancelled itself.
- Timeout : 30s default. Past that, the JoinSet is `abort_all` and the
  process exits with code 5.

---

## 5. Persistence

SQLite + `rusqlite` + FTS5. WAL journal mode.

Databases :
- `~/.apollia/runtime.db` : tasks, audit trail, sessions.
- `~/.apollia/agents.db` : agent registry (ADR-026).
- `~/.apollia/governance.db` : permissions, audit (ADR-015).
- `~/.apollia/memory/<agent>.db` : per-agent memory (`apollia-memory`).

Migration pattern : `CREATE TABLE IF NOT EXISTS` at first connection
plus a `schema_version` table. Renaming a column requires a numbered
migration step. Never `DROP COLUMN` without an explicit upgrade path.

Connection pool : `r2d2` with `r2d2-sqlite`, sized at 8 connections per
DB by default.

FTS5 : used for chat session summaries and memory recall. The match
syntax follows SQLite's FTS5 (`tag:value`, `+required`, `-excluded`).

---

## 6. Configuration

Runtime config : `~/.apollia/config.toml`. Parsed via `serde` into the
`RuntimeConfig` struct. Reloadable on SIGHUP (tracked) for a documented
subset (tracing level, MCP servers, triggers).

Rules :
- Never read `APOLLIA_*` env vars in production code for raw secrets.
  Selectors only.
- Defaults documented in `docs/wiki/Reference-Config.md`.
- Validation runs at startup. Invalid config fails fast.

---

## 7. Forbidden in this crate

- `Arc<Mutex<T>>` shared across actors. Messages or nothing.
- Calling into `apollia-cli`. The CLI calls us, not the reverse.
- Holding a SQLite connection across an `await`. Use `spawn_blocking`
  or acquire a connection from the pool inside the handler.
- Direct PyO3 calls. Route through `apollia-aip`.
- Publishing events without a corresponding handler test.

---

## 8. Testing

- Unit tests inline per file, GIVEN/WHEN/THEN.
- Integration tests in `tests/` exercise the actor wiring with a real
  EventBus.
- HTTP : `axum-test::TestServer` mounts the router without a real bind.
- Each actor has a test that drives it through its lifecycle :
  spawn, normal traffic, shutdown.

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
