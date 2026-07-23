# Project Layout

A 30-second tour of the repository.

## Top-level directories

| Path | Audience | Content |
|---|---|---|
| `crates/` | Rust developers | The Rust workspace: 18 crates that make up the runtime, CLI, desktop app, and supporting layers. |
| `sdk/` | Python agent authors | The Python SDK (`apollia` package) consumed by agents. Stubs, decorators, helpers, and test fixtures. |
| `agents/examples/` | New users | Reference agents that mirror the book chapters. Copy-paste starting points. |
| `book/` | All readers | The mdBook tutorial. Built and served by the book toolchain. |
| `docs/` | All readers | Public-facing documentation: ADRs, wiki, help, agent rulebook, this layout file. |
| `tests/` | Rust developers | Workspace-level integration tests, including the CLI end-to-end suite. |
| `.github/` | CI and triage | Issue templates, workflows, dependency configuration. |

## Rust crates (under `crates/`)

| Crate | Role |
|---|---|
| `apollia-core` | Shared types, traits, IDs, budgets, errors. The foundation everything builds on. |
| `apollia-runtime` | The agent runtime: session manager, chat backbone, A2A, observability. |
| `apollia-aip` | Apollia Interop Protocol: PyO3 bridge, context object, Python provider. |
| `apollia-cli` | The `apollia-os` command-line interface. |
| `apollia-desktop` | The Tauri desktop application (Rust side). |
| `apollia-desktop/ui` | The Svelte frontend (TypeScript + Vite). |
| `apollia-llm` | LLM router, backends (local via the embedded llama-server, Anthropic, OpenAI), meta planner. |
| `apollia-runner` | Speech-to-text runner sidecar process (whisper). No longer the local LLM engine. |
| `apollia-oria` | ORIA orchestration: plan execution, replanning, step resilience. |
| `apollia-memory` | Semantic + procedural + episodic memory layer (SQLite + FTS5 + vectors). |
| `apollia-mcp` | Model Context Protocol client and server registry. |
| `apollia-tools` | Native agent tools: bash, file IO, web search, HTTP fetch, etc. |
| `apollia-triggers` | Trigger engine: cron, interval, file watch, webhook, oneshot. |
| `apollia-notifications` | Notification engine and channels. |
| `apollia-auth` | OAuth + secret store + MCP token orchestration. |
| `apollia-connectors` | Google and Microsoft SaaS connectors (Gmail, Drive, Calendar, Outlook, etc.). |
| `apollia-permissions` | Permission rules and HITL gating. |
| `apollia-workspace` | Project workspace context. |
| `apollia-stt` | Speech-to-text integration. |

## Python SDK (under `sdk/`)

The `apollia` package is what agent authors import. Decorators (`@agent`,
`@skill`, `@on_message`, `@orchestrated`), context Protocols (`ctx.llm`,
`ctx.memory`, `ctx.a2a`, etc.), tool schemas, and testing helpers.

## Documentation (under `docs/`)

| Path | Content |
|---|---|
| `docs/adr/` | Architecture Decision Records, append-only. |
| `docs/site/` | Docusaurus public documentation (en + fr), the canonical public docs. |
| `docs/agents/` | Rulebook for AI coding assistants working in this repo. |
| `docs/PROJECT-LAYOUT.md` | This file. |

## Where to start

- **You want to use Apollia OS as a power user**: read the `README` quickstart,
  then the book chapters 1 to 3.
- **You want to write an agent**: read book chapters 4 to 7, then look at
  `agents/examples/`.
- **You want to understand the architecture**: read `docs/agents/INDEX.md`
  and the ADRs.
- **You found a bug or have a feature idea**: open an issue. Pull requests are
  closed by default for this project; see `CONTRIBUTING.md`.
