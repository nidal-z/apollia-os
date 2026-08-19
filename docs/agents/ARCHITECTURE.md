# ARCHITECTURE

> The 8 non-negotiable principles, the patterns Apollia uses to enforce them,
> and pointers into the codebase. Read this first when starting on the project.

Authoritative source for the principles : `docs/site/docs/explanation/the-8-principles.md`.
This file is the English LLM-facing summary plus the canonical patterns, and it
points at where the decisions in force are stated (Section F).

---

## Section A : The 8 principles

### 1. Local-first

**Rule.** No byte of user data leaves the machine without an explicit user
action.

**Why.** Apollia started as a SaaS. Customer feedback was unambiguous : "we
want to try, but our customer data cannot leave our premises." The fix was
not better contracts. It was making the cloud technically unnecessary.

**How.** SQLite (FTS5 + sqlite-vec) replaces PostgreSQL + Qdrant + Redis. No
telemetry. No phone home. Every external service is optional and degrades
gracefully.

### 2. Zero external dependency

**Rule.** The binary runs on any clean Linux with no prior install.

**Why.** Previous tooling required Docker, Node, Python, and three
infrastructure services. Each one was install friction, attack surface, and
maintenance burden.

**How.** Single Rust binary. PyO3 embeds the Python interpreter. Linux
namespaces replace Docker. `cargo install apollia-os` is the install.

### 3. Minimal contract, zero friction

**Rule.** An existing Python agent runs in Apollia with under 10 lines of
adaptation.

**Why.** Frameworks that impose a paradigm have a steep adoption curve.
Apollia solves infrastructure, not modeling. If the solution requires
rewriting the agent, it generates as much work as it saves.

**How.** Duck typing. `hasattr(agent, "manifest") and hasattr(agent, "run")`
is enough. AgentKit decorators (`@agent`, `@skill`) are optional sugar over
this contract. See `sdk/AGENTS.md`.

### 4. Fail fast

**Rule.** Every startup-detectable error is detected at startup.

**Why.** An agent that starts successfully and crashes on its third step of
its second task because a tool is missing is a production disaster.

**How.** Manifest validation, `tools_required` resolution, Python package
install, and MCP server reachability all happen in `INITIALIZING`. The agent
transitions to `ACTIVE` only when everything is ready. `DEGRADED` vs
`STOPPED` is the explicit fork for missing optional vs required pieces.

### 5. One actor, one responsibility

**Rule.** The runtime is not an internal monolith. Each responsibility is a
distinct Tokio actor with private state and a message channel.

**Why.** Shared state across async tasks deadlocks and defeats reasoning.

**How.** `EventBus`, `AgentRegistry`, `TaskRouter`, `ExecutionCoordinator`,
`APIServer`, `LlmRouter`, `TriggerEngine`, `NotificationEngine`,
`ChatSessionManager`, `SttEngine`, `AuditTrail`, `TimeoutWatcher`, ... Each
is an actor. See Section C and `docs/agents/RUST-PATTERNS.md` §2.

### 6. Memory at agent initiative

**Rule.** The runtime never automatically injects memory context into an agent's
prompt.

**Why.** Auto-injected memory pollutes the prompt with stale or irrelevant
content, costs tokens, and surprises the agent author.

**How.** The agent calls `ctx.memory.recall(...)` when it wants context. The
runtime provides the capability, never the policy.

**Scope of the rule.** Say "an agent's prompt", not "a prompt". The built-in
conversational assistant injects in two places, and neither is reachable from an
agent execution path: a user-persona brief at the `long_autonomous` tier, and up
to three past session summaries on the first message of a free chat session.
Writing the rule as an unqualified absolute is what let six files state something
the code contradicts.

### 7. Non-negotiable safeguards

**Rule.** `StepBudget` is enforced by the runtime, not by the agent code.

**Why.** A guard-rail an agent can bypass is not a guard-rail. Cost control,
loop prevention, and runaway containment require runtime authority.

**How.** ORIA enforces the budget on every step. `crates/apollia-oria/`.
Crossing the budget terminates the task with `task.budget_exceeded` and a
final event on the EventBus.

### 8. Human CLI, machine API

**Rule.** `--json` is a global flag. TTY is auto-detected. The same command
serves humans and scripts.

**Why.** A CLI that only works in interactive mode is a CLI that scripts
shell-out around. A CLI that only emits JSON is a CLI that humans hate.

**How.** clap v4 with global `--json`, `--quiet`, `--socket`. Exit codes
0 success / 1 general / 2 runtime / 3 task / 4 timeout / 5 interrupt.

---

## Section B : System overview

```
┌─────────────────────────────────────────────────────────────┐
│                       apollia-cli                            │
│       (noun-verb, exit codes 0-5, --json global)            │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP over Unix socket / TCP 7771
┌──────────────────────────▼──────────────────────────────────┐
│                      apollia-runtime                         │
│    Actor mesh : Registry, Router, Supervisor, EventBus,     │
│    LlmRouter, TriggerEngine, NotificationEngine, ...        │
└─────┬───────┬───────┬─────────┬─────────┬─────────┬─────────┘
      │       │       │         │         │         │
      ▼       ▼       ▼         ▼         ▼         ▼
   apollia- apollia- apollia- apollia- apollia- apollia-
   oria    memory  tools   permissions auth    mcp
   (StepB) (FTS5)  (sandb) (3-layer)  (OAuth) (JSON-RPC)
                                       (SecretStore)
                           │
                           ▼
                     apollia-aip
                     (PyO3 bridge)
                           │
                           ▼
                  Python agents + SDK
                  (@agent, @skill, @orchestrated)
```

The desktop UI (`crates/apollia-desktop/ui/`) consumes the same HTTP API
through Tauri IPC commands defined in `crates/apollia-desktop/src/commands/`.

`apollia-core` holds shared types (`AgentId`, `TaskId`, `StepBudget`) and is
the only crate every other workspace member depends on. Keep it small.
Anti-bloat policy : new types belong in the consuming crate unless they are
genuinely shared across three or more crates.

---

## Section C : Canonical patterns

### Tokio actor

`Handle` + `Actor` + `Message` enum + `run` loop with `tokio::select! biased`.
Bounded `mpsc::channel`. `CancellationToken` from `tokio-util` for
coordinated shutdown. See `docs/agents/RUST-PATTERNS.md` §2 for the snippet.

Source : `crates/apollia-runtime/src/registry.rs`,
`crates/apollia-runtime/src/supervisor/`.

### EventBus

`broadcast::Sender<RuntimeEvent>` singleton per `Supervisor`. Capacity
validated in `[64, 65536]`, default 1024. Lagged subscribers detected via
`broadcast::error::RecvError::Lagged(n)`. Events are past-tense :
`TaskCompleted`, `AgentStarted`.

Source : `crates/apollia-runtime/src/eventbus.rs`.

### ContextProvider

`trait ContextProvider: Send + Sync` exposes `async fn collect(...)` with a
fail-silent contract. The provider returns `Option<Vec<ContextSnippet>>` and
swallows recoverable errors internally. TTL cache and priority ordering at
the runtime layer. Implementations : `Git`, `Folder`, `Workspace`, ...

Source : `crates/apollia-core/src/context.rs`.

### StepBudget enforcement

`StepBudgetConfig` from `apollia-core`. Applied at each step by
`apollia-oria`. Non-bypassable. Distinct from per-step timeouts (handled by
`TimeoutWatcher`).

Source : `crates/apollia-core/src/budget.rs`,
`crates/apollia-oria/src/engine.rs`.

### Permissions

Live in the shipped runtime :

1. `PrefixRuleEngine` : prefix-scoped rules at three scopes (session, project,
   global).
2. `executor_guard` : code executors are never blanket-authorised, and a prefix
   rule only matches a single simple command.
3. HITL approval for everything not auto-approved.

Present but not wired : `PermissionEngine`, which aggregates `SafeList` and
`InjectionDetector`. No caller installs it, so those two never run. Details and
rationale in `docs/agents/SECURITY.md`.

Audit log is SQLite, append-only, no deletes. Each tool invocation produces
a decision record.

Source : `crates/apollia-permissions/`.

### SecretStore

`trait SecretStore: Send + Sync` with two backends :
- `KeyringSecretStore` : OS keyring.
- `AgeFileSecretStore` : file-encrypted with age. Selected via
  `APOLLIA_TOKEN_STORAGE=file`.

Source : `crates/apollia-auth/src/secret_storage.rs`. See
`docs/agents/SECURITY.md` for usage.

### McpClientManager

Actor that owns the pool of MCP clients (stdio, HTTP, SSE backends). Hot
reload through SQLite triggers.

Source : `crates/apollia-mcp/`.

### LlmRouter

Config-driven backend selection. `[llm.routing]` in the agent TOML drives
routing. Backends : the embedded `llama-server` (local, upstream llama.cpp),
Anthropic, OpenAI, Ollama, Vertex.

Source : `crates/apollia-llm/src/router.rs`.

### SQLite persistence

- WAL journal mode workspace-wide.
- `CREATE TABLE IF NOT EXISTS` migrations applied at first connection.
- FTS5 for full-text search (chat sessions, memory).
- Schema versioning per database file (`schema_version` table).

Source : `crates/apollia-runtime/src/chat/repository.rs`,
`crates/apollia-permissions/src/prefix_rule_engine.rs`.

### PyO3 bridge

`Bound<'py, T>` everywhere on the boundary. `pyo3-async-runtimes` for
async interop. `RuntimeContext` type contract in `sdk/apollia/types.py`
plus `sdk/apollia/context/*.py`.

Source : `crates/apollia-aip/`.

---

## Section D : The `apollia-core` anti-bloat policy

`apollia-core` is the shared types crate. Every other workspace member
depends on it. Keep it small.

Rules :
- A type belongs in `apollia-core` only if it is used by three or more
  workspace crates.
- No business logic. Pure types, traits, and small helpers.
- No async runtime imports. Stay framework-agnostic.

When in doubt, the type belongs in the consuming crate.

---

## Section E : When a change has to be written down first

Update the decisions chapter before coding if any of the following applies :

- Architectural pattern change (new actor, new trait, new persistence backend).
- New cross-crate type added to `apollia-core`.
- New third-party dependency above ~100k LoC or with a non-permissive license.
- Breaking change to a public API in `apollia-core`, `apollia-runtime`,
  `apollia-cli`, or `sdk/apollia/`.
- Security boundary change (new permission scope, new secret kind, new audit
  event type).
- Deviation from any principle in Section A.

A decision that changes the shape of the system is written into the
architecture chapter of `docs/site/`, in the present tense, under a stable
anchor. There is no separate numbered record: a decision worth keeping is worth
stating as what the system does, and a decision that has been reversed is worth
deleting rather than superseding.

---

## Section F : Where the decisions live

`docs/site/docs/architecture/08-decisions.md` states every structural decision
in force, one section per subject, each with a stable anchor. Read the section
for the area you touch before changing it. If your change contradicts what that
page says, the page is what has to change first, in the same commit.

The English page and its French mirror carry identical anchors, so a link from
either locale resolves.

---

## Section G : Where to look in the code

| Looking for | Read |
|---|---|
| Shared types and IDs | `crates/apollia-core/src/{events,result,budget,context}.rs` |
| HTTP API surface | `crates/apollia-runtime/src/api/routes_*.rs` |
| EventBus implementation | `crates/apollia-runtime/src/eventbus.rs` |
| Actor supervisor | `crates/apollia-runtime/src/supervisor/` |
| Step budget enforcement | `crates/apollia-oria/src/engine.rs` |
| Permissions engine | `crates/apollia-permissions/src/` |
| SecretStore backends | `crates/apollia-auth/src/secret_storage.rs` |
| MCP client manager | `crates/apollia-mcp/src/` |
| LLM router | `crates/apollia-llm/src/router.rs` |
| PyO3 bridge | `crates/apollia-aip/src/` |
| CLI commands | `crates/apollia-cli/src/commands/` |
| SDK decorators | `sdk/apollia/agent.py`, `sdk/apollia/skills.py` |
| Desktop UI | `crates/apollia-desktop/ui/src/` |
