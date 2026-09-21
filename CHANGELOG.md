# Changelog

All notable changes to Apollia OS are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning 2.0.0](https://semver.org/).

## [Unreleased]

Nothing yet. Every change made so far lands in the initial preview below.

## [0.2.0-preview] - Unreleased

Changes since `0.1.0-preview`: schema-constrained LLM output, typed
human-in-the-loop pauses, and the defects found by driving those paths against a
real daemon. The date and the tag land when the release is published.

The version is a minor bump: the preview adds public surface (`schema=` on
`ctx.llm`, the typed pause and its answer, new API parameters and CLI flags) and
moves `hitl.db` to schema v2. Nothing is removed from the public API.

### Added

- `ctx.llm.complete(..., schema=...)` and `ctx.llm.chat(..., schema=...)`
  constrain generation, validate the answer against the same JSON Schema, and
  resolve to the validated value as a native Python object. A failure raises
  `apollia.errors.StructuredOutputError`, which carries the JSON path of the
  offending node and a `kind`: the schema cannot become a constraint, the
  backend has no structured output mode, or the answer is refused. Nothing is
  retried. A constrained call charges one step against the `StepBudget`, and the
  journal records that generation was constrained and a fingerprint of the
  schema, never the schema. `MockLlmProxy` in `apollia.testing` accepts
  `schema=` and returns the value, and gains `stream`.
- In `apollia-llm`, `json_schema_to_gbnf`, `schema_validate` and
  `CompletionRequest.response_schema`. The embedded llama-server receives GBNF,
  every other OpenAI-compatible provider receives `response_format`, and a
  backend with no structured mode (Anthropic, Vertex) answers
  `StructuredOutputUnavailable` instead of dropping the constraint in silence.
  On a reasoning model a constrained call reads `reasoning_content` and
  `content` as one channel.
- Typed pauses. An agent pauses on a question (`genre`: `choix`, `source`,
  `seuil`, `definition`, `confirmation`, with propositions) or on an approval of
  a named `geste` with its `risque` (`apollia_core::HitlPayload`). It reads the
  answer through `ctx.is_resumed` and `ctx.input_response` (`approved`,
  `reason`, `answer`, `payload`, `context`) and can pause several times in one
  task. `NeedHumanInput` and `AIPResult.input_required` take a `payload` typed
  by `apollia.hitl`. An invalid payload fails the task with
  `INVALID_INPUT_PAYLOAD`.
- `POST /api/v1/tasks/{id}/resume` takes `answer`, checked against the pending
  payload (422 `INVALID_ANSWER` leaves the task paused). `POST /api/v1/tasks`
  takes an optional `skill_id`. `GET /api/v1/tasks` finds `input_required` tasks
  and lists agent, skill, `created_at`, prompt and payload, and
  `GET /api/v1/approvals/pending` carries `payload` and `skill_id`.
  `just hitl-e2e` proves the path on a real daemon.
- A structured MCP tool result reaches the agent with a `structured` key holding
  the object as the server built it; `content` is unchanged.
- `GET /api/v1/audit/journal?agents=<name>,<name>` and `apollia-os audit journal
  --agent` narrow the journal to the runs of those agents.
- `A2AInvocationResult` carries `run_id`, the key the chained journal keeps an
  invocation under.
- `apollia-os mcp add` and `mcp update` take a repeatable `--arg` for the
  command's arguments, and `--transport`. A URL alone means `streamable-http`.
- Desktop: the operator chooses the model download folder, in onboarding (LLM
  and STT) and in the Model Hub, for machines whose default `~/.apollia/models`
  drive has no room.

### Changed

- The interpreter agents run on is the bundled one on all three systems. A
  system interpreter is used only when the operator names it in
  `[tools] python_interpreter`, checked by `apollia-os config set` and by the
  Advanced settings page: it must exist, start and report the bundled minor
  version, or it is dropped with a warning.
- A declined pause that carries a payload resumes the agent with `approved:
  false`; a prompt-only pause keeps failing with `REJECTED`. `hitl.db` goes to
  schema v2.
- The Google connector executors moved from `apollia-desktop` to
  `apollia-runtime`, next to the Microsoft ones; the desktop keeps the consent
  half that opens a browser.
- The Python API client covers every operation the spec declares, including
  `list_audit_journal`, the registry reads and the STT reload. `regen.sh` pins
  both generator versions and refuses to run without `ruff`.
- Documentation: the human-in-the-loop how-to (both locales) and the SDK
  reference describe the typed pause; the MCP pages and the connection wizard
  say the per-server approval level governs agent tasks, not chat.
- The file-watch trigger tests wait for the watch to be registered instead of a
  fixed delay.

### Fixed

- Apollia Desktop failed to start on Windows with `python313.dll` not found when
  another Python was installed and the user had no administrator rights. The DLL
  is now staged beside the executable. Not declared fixed until an installation
  on such a machine confirms it.
- A configured system proxy no longer swallows requests to loopback backends.
  The embedded llama-server, a local Ollama, the STT runner and local MCP servers
  failed with a silent transport error on networks that require a proxy, and the
  llama-server startup health check went through the proxy too, so the engine
  reported `did not become healthy within 180s` while it was running. On
  Windows the proxy bypass list usually names `localhost` and not `127.0.0.1`,
  which is why one spelling worked and the other timed out.
- The name `localhost` in an endpoint the operator configured resolves to IPv4
  first, then IPv6, instead of following the OS resolver order. On Windows a
  local Ollama, which binds `127.0.0.1`, was unreachable under
  `http://localhost:11434`. The Ollama default is now `http://127.0.0.1:11434/v1`
  in the CLI, the router and the desktop form.
- An MCP server declared `requires_approval` gated no call on the task path; it
  now pauses on an approval naming `<server>/<tool>`, runs once when approved and
  raises `ToolApprovalDenied` when declined. An agent requiring a tool of such a
  server now installs (`POST /api/v1/agents` answered 400) and every run of it is
  gated, including the CLI chat-agent runner.
- A task with no `skill_id` reached only `@on_message`, so an agent whose one
  entry point is a `@skill` failed `NO_HANDLER` when a trigger fired it. A lone
  skill is now reached.
- `AIPResult.input_required` returned from a skill paused nothing.
- `POST /api/v1/a2a/invoke` answered `result.task_id: ""`.
- A journaled `ctx.llm` call and the chat agent loop named
  `<resolved-by-router>` or an empty model instead of the model that answered.
- A Google connector declared by an installed agent (`gmail.send`) answered
  `UnknownTool` under `apollia-os start`.
- The block that injects past session summaries into the first message of a free
  chat is framed as background not to act on, after a local model re-issued a
  stale tool call from an unrelated session.

## [0.1.0-preview] - Unreleased

Initial public preview. Local-first Rust runtime for autonomous AI agents,
single-maintainer. The date and the tag land when the release is published.

The whole tree carries this version: the twenty-one Rust crates, the Python
SDK, the built-in agents, and the desktop bundle. The desktop bundle files are
stamped `0.1.0-1` instead, on all three platforms (`tauri.conf.json` names the
`.dmg`, the `.deb`, the `.AppImage` and the Windows installers alike), because
the MSI product version has to be numeric.

The `Changed` and `Fixed` entries below describe work done inside the preview
cycle, before anything was published.

### Added

**Runtime core**

- Tokio-based agent runtime with `EventBus`, `AgentRegistry`, `TaskRouter`,
  `ExecutionCoordinator`, `Supervisor`, and `ShutdownController` (graceful
  drain).
- `APIServer` (axum) on Unix socket and TCP port 7771.
- REST: `POST /tasks`, `GET /tasks/:id`, `GET /agents`, `POST /agents`, and
  related endpoints.
- Server-sent events at `GET /tasks/:id/stream` for real-time progress.
- ORIA orchestration engine: observer-driven classification (`Direct` vs
  `Orchestrated`), tri-dimensional step budget (steps, tool calls, wall
  clock), resilience layer with circuit breakers and exponential retry.

**PyO3 bridge (AIP)**

- Native async Rust to Python bridge via `pyo3` and `pyo3-async-runtimes`.
- Decorator-based agent contract: a class carrying `@agent` plus at least one
  `@skill` or `@on_message` method. The bridge refuses an object without
  `__apollia_dispatch__`.
- `ToolProxy`, `MemoryInterface`, and `AIPBridge` exposing the Rust runtime
  to Python.
- `AgentLoader` trait decoupling the runtime from PyO3.

**LLM**

- LLM router serving local GGUF models through an embedded `llama-server`
  (upstream llama.cpp), alongside Anthropic and OpenAI providers.
- Local speech to text through whisper in the `apollia-runner` sidecar, on
  the card through Metal on macOS and Vulkan on Windows, on the processor
  elsewhere; the daemon picks the runner its bundle carries.
- Meta planner for next-step suggestions, plan caching, and orchestrated
  decision points.
- Token budget tracking per session with hard and soft limits.

**Tools**

- Tool registry actor with at-startup resolution of required and optional
  tools.
- Native tools: `bash_executor` (Linux namespaces, macOS dev mode),
  `python_executor` (per-agent venv isolation), file IO suite
  (`file_read`, `file_write`, `file_edit`, `file_glob`, `file_grep`),
  `web_search`, `web_read`, `http_fetch`, `memory_search`, `notebook_*`.
- Audit trail with SQLite WAL, fire-and-forget logging, SHA-2 input hashes.
- HITL approval store with permission rules scoped to session, project, or
  agent.

**Memory**

- Multi-layer memory: episodic (events + importance), semantic (facts +
  confidence), procedural (procedures + triggers).
- SQLite with FTS5 and BM25 for full-text search across layers.
- Namespace isolation, lazy store opening, access-level enforcement.

**MCP**

- Model Context Protocol client: 18-entry catalog of curated servers,
  stdio and HTTP transports. Inbound MCP server (stdio) exposes native tools.
- Custom MCP server installation through the desktop UI or CLI.
- OAuth orchestration for MCP servers that require it.

**Connectors**

- Google: Gmail, Calendar, Drive (workspace), Sheets, Tasks, Docs, Forms,
  Slides, YouTube.
- Microsoft: Outlook mail, Outlook calendar, OneDrive.

**Triggers and notifications**

- Trigger engine: cron, interval, oneshot, file watch, webhook sources.
- Notification engine with desktop and webhook channels, per-budget alerts.

**Desktop**

- Tauri 2 application sharing the runtime and Python interpreter with the
  CLI.
- Svelte 5 frontend with TypeScript, Vite, Tailwind, Bits UI.
- Built-in onboarding agent, and a companion that answers from the
  documentation shipped with the binary.

**CLI**

- `apollia-os` binary at near-parity with the Desktop (40+ subcommands).
- `--json` and `--quiet` global flags on every command.
- POSIX exit codes: 0 success, 1 usage, 2 runtime, 3 task failed, 4 timeout,
  5 canceled.
- End-to-end smoke suite (`tests/cli/cli-e2e.sh`): 296 assertions across the
  OFFLINE and RUNTIME tracks (167 and 129), plus 7 structural captures in the
  LLM CAPTURE track, which is gated on a real model.

**SDK**

- Python `apollia` package: `@agent`, `@skill`, `@on_message`,
  `@orchestrated` decorators.
- Context Protocols: `ctx.llm`, `ctx.memory`, `ctx.a2a`, `ctx.tools`,
  `ctx.notify`, `ctx.logger`.
- Testing helpers and mock proxies for unit testing agents in isolation.

**Build and packaging**

- Workspace MSRV: Rust 1.89.
- Cross-compilation hints in `Cross.toml`.
- `deny.toml` for license, banned crates, and advisory checks.
- `cargo deny` green at release time (advisories, bans, licenses, sources),
  on every pull request and again in the weekly deep audit.

**Documentation**

- Public documentation site (Docusaurus) with a capstone end-to-end walkthrough.
- Operator help corpus.
- `AGENTS.md` rulebook and the `docs/agents/` engineering corpus.

**Verification**

- Concurrency verification: abstract Loom models of the runtime actor
  algorithms in the standalone `apollia-loom-models` crate, and a Miri
  undefined-behavior suite over the FFI-adjacent pure helpers in `apollia-aip`.
  Both are dev-only (no runtime dependency) and run as advisory nightly CI jobs.
- Formal verification: bounded Kani symbolic proofs of the two cardinal
  invariants, the non-bypassable `StepBudget` (`apollia-oria`) and the mailbox
  lease/ack fence (`apollia-runtime`), each with an in-tree proptest mirror that
  runs under `cargo test`. Dev-only; an advisory nightly `kani` CI job runs the
  proofs (Kani links its own toolchain via rustup, so it is CI-only).

### Changed

- Timing-dependent runtime tests (registry actor-death, router degraded-agent
  event, router cancel-vs-late-completion) now use event-driven awaits and
  bounded poll-until-condition instead of fixed sleeps, removing flakiness.

### Fixed

- Mailbox lease exclusivity (C9-F4): `ack`/`nack` are now fenced on the leasing
  `run_id` via a new `lease_owner` column, so a stale consumer whose lease was
  re-leased to another run can no longer delete or requeue the message the new
  owner is processing.
- `StepBudget` step/tool-call increment used a checked `+` that could panic in
  debug at `u32::MAX`; it now saturates.

### Security

- `cargo deny check advisories` green at release time. There is no
  `cargo audit` job: `cargo deny` is the single advisory gate.
- Twenty-two advisories are suspended in `deny.toml`, each with its lift
  condition. Twenty are unmaintained-crate notices on transitive dependencies;
  two are pyo3 0.24 soundness advisories, RUSTSEC-2026-0176 (out-of-bounds
  read) and RUSTSEC-2026-0177 (missing `Sync` bound), whose fix lands in
  pyo3 0.29. Their exploit paths (`new_closure`, the `nth` iterator adapter)
  are on no Apollia code path. Lift condition: the pyo3 0.29 migration.
- Private vulnerability reporting via GitHub Security Advisories.

[Unreleased]: https://github.com/Apollia-OS/apollia-os/commits/main
[0.2.0-preview]: https://github.com/Apollia-OS/apollia-os/releases/tag/v0.2.0-preview
[0.1.0-preview]: https://github.com/Apollia-OS/apollia-os/releases/tag/v0.1.0-preview
