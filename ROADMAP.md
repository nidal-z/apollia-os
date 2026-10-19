# Apollia OS roadmap

This document states the direction of Apollia OS at a level above the
[changelog](CHANGELOG.md): what shipped, what is being built, and what comes
after. It carries no dates. Items move, get reordered, and sometimes drop; the
changelog is the record of what actually landed.

## Shipped

### v0.1.0-preview

The first public preview.

- A local-first Rust runtime for autonomous AI agents.
- A native PyO3 bridge for Python agents, with no subprocess.
- Model Context Protocol (MCP) support: a curated catalog, stdio and HTTP
  transports, and custom server installation from the desktop app.
- An LLM router with a local backend (the embedded `llama-server`, upstream
  llama.cpp) and optional Anthropic and OpenAI providers.
- The ORIA orchestration engine for multi-step plans with replanning.
- A desktop app (Tauri 2) and a CLI (`apollia-os`) sharing the same runtime.
- Built-in agents: an onboarding guide, and a companion that answers from the
  documentation shipped with the binary.
- Workspace tooling: sandboxed shell, file access, web search, web read, HTTP
  fetch.
- An audit trail, human approvals, and scoped permission rules.

### v0.2.0, the second preview

Its content is listed in the [changelog](CHANGELOG.md).

## In progress

### v0.3.0, a turnkey product

The largest update since the first preview, and the end of preview tags: from
v0.3.0 on, releases are tagged without the `-preview` suffix.

The previews showed that the product works and is still too hard to set up and
to apply to a real use case. v0.3.0 makes installing an agent, connecting a
tool and running a business case a matter of minutes.

- **A redesigned desktop app**: fewer screens, ranked by use; one vocabulary
  across the interface; a single developer mode in place of the Operator and
  Builder split; a first launch that ends on an agent at work.
- **Agents**: an agent is a sheet (mission, model, accesses, sources, rights)
  that can be created without code; agent packages; batch processing;
  workflows; a new SDK contract.
- **Models**: one screen for models, an engine chosen from the model format
  (llama.cpp for GGUF, MLX for MLX models), condensed model cards from a signed
  online registry, a recommendation that reads the hardware, and presets for
  loading and inference settings.
- **A local server**: an OpenAI-compatible server for other applications on the
  machine or the local network, with loaded-model management.
- **Multiple user profiles.**
- **Governance**: consent on sensitive actions, a readable journal, and an
  organisation policy that an administrator signs and installs.
- **Data**: local hybrid search over documents, office formats, and IMAP
  mailboxes.

## Planned

### v0.4.0, agentic orchestration

v0.3.0 made an agent a sheet anyone can set up; its batches showed where a
single plan written in advance stops. v0.4.0 brings the engine to long,
complex work.

- **Agentic orchestration** (major): an agent that keeps and revises its own
  task list over long sequences instead of one fixed plan; sub-agents with an
  isolated context and a budget of their own, each on a precise task; agents
  created on the fly when none fits; workflows used as skills; A2A as the way
  agents call one another; context kept across very long runs; reliability
  measured on real tasks, local models first.
- **Speech-to-text** (minor): Whisper and its models optimised and brought to
  the level of the language models (engines, registry, settings).
- **Text-to-speech** (minor): agents that speak, and accessibility.

### v0.5.0, vision and proactive agents

- **Vision models and tooling**: reading the screen and external visual
  sources.
- **Proactive agents**: agents that observe their environment and take the
  initiative, rather than only reacting to triggers.

### v0.6.0, CLI and SDK at production grade

- Every Apollia capability reachable from the CLI and the SDK. Parity
  progresses with each desktop release; v0.6.0 closes the gaps that remain.

### v0.7.0, Apollia Buddy

- Use your Apollia from another device.

## How priorities move

Priorities respond to friction reported by real usage, to the requirements of
real agent deployments, and to upstream changes in the Rust async ecosystem,
Tauri, llama.cpp, MLX and the MCP specification. Feedback is welcome through
[issues and discussions](CONTRIBUTING.md).

The roadmap is not a backlog.
