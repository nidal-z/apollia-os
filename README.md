# Apollia OS

> Open-source Rust runtime for sovereign autonomous AI agents.
> Local-first. Zero cloud. One binary.

[![CI](https://github.com/nidal-z/apollia-os/actions/workflows/ci.yml/badge.svg)](https://github.com/nidal-z/apollia-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

---

## What is Apollia OS?

Apollia OS is a Rust runtime that executes autonomous AI agents (LangGraph, CrewAI, AutoGen, or custom) in an isolated, local environment — no cloud dependency, no data leaving your machine. It provides each agent with persistent memory (SQLite + FTS5), sandboxed tool execution, a circuit-breaker resilience layer, and a step budget enforced at the runtime level. Expose your agents over a local REST API or drive them entirely from the CLI.

---

## Quickstart

**Prerequisites:** Rust 1.75+, Python 3.11+. See [docs/INSTALL.md](docs/INSTALL.md) for details.

```bash
# 1. Build the workspace
cargo build --workspace --release

# 2. Start the runtime (background daemon)
apollia-os start

# 3. Deploy the demo agent
apollia-os agent start agents/hello_agent.py

# 4. Run a task
apollia-os run hello-agent "Bonjour"

# 5. Stop the runtime
apollia-os stop
```

Expected output for step 4:

```
  -> Task t-001 submitted to hello-agent
  Executing...
  Done in 0.3s (1 step, 0 tool calls)

  RESULT
  Bonjour ! J'ai recu : Bonjour
```

---

## Architecture

Apollia OS is built around six components communicating via Tokio actor channels. Zero shared mutable state between actors. The PyO3 bridge translates async Rust futures to Python coroutines — no subprocess spawning.

```
+---------------------------------------------------------------------+
|                         APOLLIA OS RUNTIME                          |
|                                                                     |
|  +------------------------------------------------------------+     |
|  |                       RUNTIME CORE                         |     |
|  |  Supervisor  AgentRegistry  TaskRouter  EventBus  APIServer|     |
|  +---------------------+--------------------------------------+     |
|                         |                                           |
|              +----------v----------+                                |
|              |  ExecutionCoord.    |  one per active agent          |
|              +----------+----------+                                |
|                         |                                           |
|              +----------v----------+                                |
|              |    ORIA ENGINE      |  Observer - Reasoner - Actor   |
|              |  Direct / Orchestr. |  StepBudget + ResilienceLayer  |
|              +------+----------+---+                                |
|                     |          |                                    |
|  +------------------v-+  +-----v----------+                        |
|  |   TOOL REGISTRY    |  |  MEMORY ENGINE |                        |
|  |   + SANDBOX        |  |  (SQLite/FTS5) |                        |
|  +--------------------+  +----------------+                        |
|                                                                     |
|  +------------------------------------------------------------+     |
|  |                   AIP BRIDGE (PyO3)                        |     |
|  |              Rust <-> Python async bridge                  |     |
|  +--------------------------+---------------------------------+     |
+---------------------------- | --------------------------------------+
                               | AIP
                   +-----------v-----------+
                   |     PYTHON AGENT      |
                   |  LangGraph  CrewAI    |
                   |  AutoGen    custom    |
                   +-----------------------+
```

Full architecture documentation: [docs/Architecture-Vue-Ensemble.md](docs/Architecture-Vue-Ensemble.md)

---

## Writing an Agent

An Apollia agent is any Python object with two methods — no base class required (duck typing):

```python
# my_agent.py
class MyAgent:
    def manifest(self):
        return {
            "name": "my-agent",
            "version": "1.0.0",
            "description": "My first Apollia agent",
            "tools_required": [],          # e.g. ["file_io", "bash_executor"]
            "max_concurrent_tasks": 1,
        }

    async def run(self, task, ctx):
        parts = task.get("input", {}).get("parts", [])
        user_input = parts[0]["text"] if parts else ""
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Hello: {user_input}"}],
        }

agent = MyAgent()
```

Deploy it:

```bash
apollia-os agent start ./my_agent.py
apollia-os run my-agent "Hello world"
```

See [agents/hello_agent.py](agents/hello_agent.py) for the minimal demo agent.

---

## CLI Reference

### Level 1 — Daily operations

| Command | Description |
|---|---|
| `apollia-os start` | Start the runtime daemon |
| `apollia-os stop` | Graceful shutdown (30s task drain) |
| `apollia-os status` | Overview: agents, active tasks, tool health |
| `apollia-os run <agent> "<input>"` | Submit a task and wait for the result |

### Level 2 — Full management

| Command | Description |
|---|---|
| `apollia-os agent list` | List registered agents and their state |
| `apollia-os agent start <file.py>` | Deploy a Python agent |
| `apollia-os agent stop <name>` | Drain and stop an agent |
| `apollia-os agent info <name>` | Agent details (tools, memory stats, budget) |
| `apollia-os task list` | List recent tasks |
| `apollia-os task status <id>` | Task state and result |
| `apollia-os task cancel <id>` | Cancel a running task |
| `apollia-os tools list` | List available tools |
| `apollia-os tools describe <tool>` | Tool schema and parameters |
| `apollia-os memory inspect <ns>` | Memory namespace overview |
| `apollia-os audit` | Tool call audit log |
| `apollia-os audit stats` | Aggregated stats (success rate, avg duration) |

**Global flags:** `--json` (machine output) · `-q/--quiet` · `-v/--verbose` · `--debug` · `--socket <path>`

**Exit codes:** `0` success · `1` usage error · `2` runtime error · `3` task failed · `4` timeout · `5` canceled

---

## Project Structure

```
crates/
  apollia-core/     # Shared types (AgentManifest, AIPTask, AIPResult, ProcessState)
  apollia-runtime/  # Runtime Core (Supervisor, AgentRegistry, TaskRouter, APIServer)
  apollia-oria/     # ORIA Engine (Observer, StepBudget, ResilienceLayer)
  apollia-tools/    # Tool Registry + native tools (file_io, bash_executor, ...)
  apollia-memory/   # Memory Engine (SQLite, FTS5, episodic/semantic/procedural)
  apollia-aip/      # AIP Bridge (PyO3, ToolProxy, MemoryInterface)
  apollia-cli/      # CLI binary (clap v4)
agents/             # Example agents
docs/               # Architecture and design documentation
tests/              # End-to-end integration tests
```

---

## Contributing

1. Read [docs/Architecture-Principes.md](docs/Architecture-Principes.md) — eight non-negotiable principles.
2. Read [CLAUDE.md](CLAUDE.md) for coding rules (Rust, Git, testing conventions).
3. Create a branch: `feature/<STORY-NNN>-short-description`
4. Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before committing.
5. Open a pull request against `main`.

Bug reports and feature requests: open a [GitHub issue](https://github.com/nidal-z/apollia-os/issues).

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
