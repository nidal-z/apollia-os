# Changelog

All notable changes to Apollia OS are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning 2.0.0](https://semver.org/).

## [Unreleased]

## [0.1.0-preview] - 2026-06-03

Initial public preview. Local-first Rust runtime for autonomous AI agents,
single-maintainer source-available.

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
- Duck-typed agents: any Python object exposing `manifest()` and an async
  `run()` is AIP-compatible.
- `ToolProxy`, `MemoryInterface`, and `AIPBridge` exposing the Rust runtime
  to Python.
- `AgentLoader` trait decoupling the runtime from PyO3.

**LLM**

- LLM router with local quantized backend (`llama-cpp-2`), Anthropic, and
  OpenAI providers.
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

- Full Model Context Protocol support: 18-entry catalog with curated
  servers, stdio and HTTP transports.
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
- Built-in onboarding agent and Apollia coach.

**CLI**

- `apollia-os` binary at near-parity with the Desktop (40+ subcommands).
- `--json` and `--quiet` global flags on every command.
- POSIX exit codes: 0 success, 1 usage, 2 runtime, 3 task failed, 4 timeout,
  5 canceled.
- End-to-end smoke suite (`tests/cli/cli-e2e.sh`): 271 assertions across a
  daemon-off phase and a daemon-on phase.

**SDK**

- Python `apollia` package: `@agent`, `@skill`, `@on_message`,
  `@orchestrated` decorators.
- Context Protocols: `ctx.llm`, `ctx.memory`, `ctx.a2a`, `ctx.tools`,
  `ctx.notify`, `ctx.logger`.
- Testing helpers and mock proxies for unit testing agents in isolation.

**Build and packaging**

- Workspace MSRV: Rust 1.85.
- Cross-compilation hints in `Cross.toml`.
- `deny.toml` for license, banned crates, and advisory checks.
- Cargo audit and Cargo deny green at release time.

**Documentation**

- Public book (mdBook) with capstone E2E walkthrough.
- Operator help corpus.
- Architecture Decision Records.
- `AGENTS.md` rulebook for AI coding assistants working in the repo.

### Security

- No known vulnerabilities at release time (`cargo audit` clean).
- Documented advisory exceptions in `deny.toml` for transitive dependencies
  awaiting upstream patches.
- Private vulnerability reporting via GitHub Security Advisories.

[Unreleased]: https://github.com/Apollia-OS/apollia-os/compare/v0.1.0-preview...HEAD
[0.1.0-preview]: https://github.com/Apollia-OS/apollia-os/releases/tag/v0.1.0-preview
