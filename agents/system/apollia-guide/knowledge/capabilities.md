# Apollia OS, capabilities

Apollia OS is a sovereign, local-first runtime for autonomous AI agents. It
runs any Python agent locally, in isolation, with tools, and without a cloud
dependency.

## Core features

- **Agents**: run Python agents built with the Apollia AgentKit SDK. An agent
  is a class decorated with `@agent`; it handles chat with `@on_message` or
  exposes callable skills with `@skill`.
- **Local inference**: llama.cpp GGUF models run on-device (Metal / CUDA). You
  can also point Apollia at Ollama, Anthropic, or OpenAI backends.
- **Memory**: cross-session semantic, episodic, and procedural memory in local
  SQLite. Agents read and write it at their own initiative through `ctx.memory`;
  it is never injected automatically.
- **Tools**: native tools include web search (`web_search`), web read
  (`web_read`), HTTP fetch (`http_fetch`), file access (`file_read`,
  `file_write`, `file_glob`), and a sandboxed `bash_executor`.
- **Triggers**: cron, interval, file-watch, and webhook triggers automate agent
  runs.
- **Orchestration (ORIA)**: a ReAct engine with an enforced, non-bypassable step
  budget and a resilience layer, plus post-run verification.
- **A2A**: agents delegate to one another (director / worker) through
  `ctx.a2a`, with a recursion guard and a chain deadline.
- **Audit**: every run is recorded in a tamper-evident, hash-chained journal you
  can verify and export.
- **MCP**: a native MCP client connects tool servers over stdio, HTTP, or SSE.
- **Connectors**: OAuth connectors for Google and Microsoft, gated per scope.
- **Surfaces**: a Unix-socket + TCP HTTP API (token auth, optional native TLS),
  a `apollia-os` CLI, and a Tauri desktop app.

## The 8 principles

1. Local-first: no user data leaves the machine without an explicit action.
2. Zero external dependency: the binary runs on a clean machine with no prior
   install.
3. Minimal contract: an agent is a decorated Python class, nothing more.
4. Fail-fast: any startup-detectable error is detected at startup.
5. One actor, one responsibility: no shared state between runtime actors.
6. Memory at the agent's initiative: context is never injected automatically.
7. Non-bypassable safeguards: the step budget is enforced by the runtime.
8. Human CLI, machine API: `--json` everywhere, TTY auto-detected.
