# Apollia OS - Capabilities

Apollia OS is a local-first runtime for autonomous AI agents.

## Core features

- **Agents**: Run any Python agent (LangGraph, CrewAI, custom) locally via the AIP bridge.
- **Memory**: Cross-session semantic and episodic memory stored in SQLite.
- **Tools**: Native tools - web search, web read, file operations, bash executor, HTTP fetch.
- **Triggers**: Cron, interval, file watch, and webhook triggers to automate agent runs.
- **Pipelines**: DAG-based pipelines with fan-out/fan-in, HITL approval steps, and fallback.
- **A2A**: Director/Worker delegation pattern for multi-agent collaboration.
- **Agent Packages**: Distributable agent packages described by `agent.toml` (ADR-026).
- **LLM backends**: llama.cpp (local), Ollama, Anthropic Claude, OpenAI.
- **MCP**: Native MCP client for tool servers (stdio, HTTP, SSE).
- **Desktop app**: Tauri-based desktop app with Svelte UI for all operations.

## Principles

1. Local-first - no user data leaves the machine without explicit action.
2. Zero external dependencies - binary runs on any Linux without installation.
3. Minimal contract - agents need only `manifest()` + `async run()`.
