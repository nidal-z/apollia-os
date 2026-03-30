# Apollia OS

> Open-source Rust runtime for sovereign autonomous AI agents.
> Local-first. Zero cloud. One binary.

[![CI](https://github.com/nidal-z/apollia-os/actions/workflows/ci.yml/badge.svg)](https://github.com/nidal-z/apollia-os/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)

---

## What is Apollia OS?

Apollia OS is a Rust runtime that executes autonomous AI agents (LangGraph, CrewAI, AutoGen, or custom) in an isolated, local environment. No data leaves your machine. No cloud dependency. No subprocess spawning — the PyO3 bridge translates async Rust futures to Python coroutines directly.

**Key capabilities:**

- **Local-first LLM inference** — run GGUF models on CPU or Apple Silicon Metal GPU, or connect to Anthropic / OpenAI-compatible APIs
- **Persistent memory** — three-tier SQLite store (episodic, semantic, procedural) with FTS5 full-text search per agent
- **Native tools** — sandboxed bash, file I/O, and Python execution with per-agent venv isolation
- **Step budget** — `max_steps` / `max_tool_calls` / wall-clock timeout enforced at the runtime level, not contournable by agent code
- **Circuit breaker** — per-tool resilience layer with exponential backoff and jitter
- **Triggers** — cron, interval, file watch, and authenticated webhooks (HMAC-SHA256)
- **Multi-agent pipelines** — topological execution with per-step conditions, HITL suspension, and fallback paths
- **Human-in-the-Loop (HITL)** — any tool can require human approval before execution; runtime suspends and resumes transparently
- **Desktop app** — native Tauri v2 + Svelte 5 UI with live SSE dashboards for all subsystems
- **REST API + CLI** — full management via `apollia-os` CLI or HTTP on `127.0.0.1:7771`

---

## Quickstart

**Prerequisites:** Rust 1.75+, Python 3.11+. See [docs/INSTALL.md](docs/INSTALL.md) for full installation instructions.

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

Apollia OS is built around independent Tokio actors communicating over channels. Zero shared mutable state between actors.

```
+------------------------------------------------------------------------+
|                          APOLLIA OS RUNTIME                            |
|                                                                        |
|  Supervisor · AgentRegistry · TaskRouter · EventBus · APIServer        |
|                          |                                             |
|             +------------v------------+                                |
|             |   ExecutionCoordinator  |  one per active agent          |
|             +------------+------------+                                |
|                          |                                             |
|             +------------v------------+                                |
|             |      ORIA ENGINE        |  Observer - Reasoner - Actor   |
|             |  Direct / Orchestrated  |  StepBudget · ResilienceLayer  |
|             +-------+----------+------+                                |
|                     |          |                                       |
|  +------------------v-+  +-----v----------+  +--------------------+   |
|  |   TOOL REGISTRY    |  |  MEMORY ENGINE |  |    LLM ROUTER      |   |
|  |   + SANDBOX        |  |  SQLite/FTS5   |  |  local · cloud     |   |
|  +--------------------+  +----------------+  +--------------------+   |
|                                                                        |
|  TriggerEngine · PipelineEngine · NotificationEngine                  |
|                                                                        |
|  +------------------------------------------------------------------+  |
|  |                    AIP BRIDGE (PyO3)                             |  |
|  |             Rust ↔ Python async — ToolProxy · MemoryInterface   |  |
|  +------------------------------+-----------------------------------+  |
+-------------------------------- | --------------------------------------+
                                  | AIP contract
                      +-----------v-----------+
                      |     PYTHON AGENT      |
                      |  LangGraph  CrewAI    |
                      |  AutoGen    custom    |
                      +-----------------------+
```

Full architecture documentation: [docs/Architecture-Vue-Ensemble.md](docs/Architecture-Vue-Ensemble.md)

---

## Platform Support

| Platform | CPU | GPU | Status |
|----------|-----|-----|--------|
| Linux x86_64 | ✅ Tested | CUDA — planned | Primary development target |
| macOS Apple Silicon | ✅ Tested | ✅ Metal tested | No Xcode required |
| macOS Intel | ✅ Should work | — | Not explicitly tested |
| Windows x86_64 | Planned | CUDA — planned | Not yet tested |
| Linux (ROCm / AMD GPU) | — | Not planned | No timeline |

> **Note:** Windows and CUDA builds require community testing before being marked stable.
> If you test on a platform not listed above, please open an issue with your results.

---

## LLM Backends

Apollia OS ships three backend types. The default binary (`cargo build --release`) supports only API backends. Local inference requires a feature flag.

### Embedded — local GGUF model

```bash
# CPU (Linux, macOS, Windows)
cargo build --release --features local

# Apple Silicon GPU (Metal — no Xcode required)
cargo build --release --features local-metal

# NVIDIA GPU (CUDA — Linux and Windows, not yet tested)
cargo build --release --features local-cuda
```

Place any GGUF model in `~/.apollia/models/` and configure `apollia.toml`:

```toml
[llm]
default = "local"

[[llm.backends]]
type         = "embedded"
name         = "local"
model_path   = "~/.apollia/models/example.gguf"
device       = "metal"     # "cpu" | "metal" | "cuda"
quantization = "q8_0"
```

### Cloud — Anthropic

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

```toml
[llm]
default = "anthropic"

[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

### Cloud — OpenAI-compatible

Any OpenAI-compatible endpoint (OpenAI, Mistral, Ollama, LM Studio, vLLM, etc.):

```toml
[[llm.backends]]
type        = "api"
name        = "openai"
api_url     = "https://api.openai.com/v1"
model       = "gpt-4o-mini"
api_key_env = "OPENAI_API_KEY"
```

Multiple backends can coexist. `default` selects which one agents use unless they override it in their manifest.

---

## Writing an Agent

An Apollia agent is any Python object with two methods — no base class, no inheritance:

```python
# my_agent.py
class MyAgent:
    def manifest(self):
        return {
            "name":                  "my-agent",
            "version":               "1.0.0",
            "description":           "My first Apollia agent",
            "tools_required":        [],          # e.g. ["file_io", "bash_executor"]
            "max_concurrent_tasks":  1,
            "execution_mode":        "direct",    # "direct" | "orchestrated"
        }

    async def run(self, task, ctx):
        parts = task.get("input", {}).get("parts", [])
        user_input = parts[0]["text"] if parts else ""
        return {
            "task_id": task["task_id"],
            "status":  "completed",
            "output":  [{"type": "text", "text": f"Hello: {user_input}"}],
        }

agent = MyAgent()
```

```bash
apollia-os agent start ./my_agent.py
apollia-os run my-agent "Hello world"
```

### ReAct agents — `BaseReActAgent`

For agents that need to reason and call tools in a loop, inherit from `BaseReActAgent` in `agents/apollia_base.py`. It implements the full ReAct cycle (`REASON → ACT → OBSERVE`) on top of `ctx.llm` and `ctx.tools`, with built-in HITL support and conversation history persistence across suspensions:

```python
from apollia_base import BaseReActAgent, AIPResult, resume_pending_tool

class CodeReviewer(BaseReActAgent):
    SYSTEM_PROMPT = "You are an expert code reviewer."
    MAX_STEPS = 8

    def manifest(self):
        return {
            "name":                    "code-reviewer",
            "version":                 "1.0.0",
            "description":             "Reviews code and writes a report",
            "tools_required":          ["bash_executor", "file_io"],
            "tools_requiring_approval": ["file_io"],   # HITL before writes
            "execution_mode":          "direct",
            "dangerous_tools_allowed": False,
        }

    async def run(self, task, ctx):
        user_msg = task["input"]["parts"][0]["text"]
        pending  = resume_pending_tool(task)              # HITL resume
        result   = await self.react(task, ctx, user_msg, pending_tool=pending)
        if isinstance(result, dict):
            return result                                 # input_required / failed
        return AIPResult.completed(result)

agent = CodeReviewer()
```

### Runtime context (`ctx`)

| Attribute | Type | Description |
|---|---|---|
| `ctx.llm` | `LlmProxy \| None` | Call `await ctx.llm.complete(messages)` |
| `ctx.tools` | `ToolProxy \| None` | Call `await ctx.tools.call("tool_name", args)` |
| `ctx.memory` | `MemoryInterface \| None` | `record`, `recall`, `search`, `forget` |

All three attributes degrade gracefully to `None` — always check before use.

### Native tools

| Tool | Description |
|---|---|
| `bash_executor` | Execute shell commands (timeout, Linux namespace sandbox) |
| `file_io` | Read / write / list / exists on the local filesystem |
| `python_executor` | Execute Python 3 code in an isolated per-agent venv |

---

## Configuration

All runtime behaviour is controlled by `apollia.toml` in the working directory. Paths support `~` expansion. Annotated reference:

```toml
[runtime]
socket                = "/tmp/apollia.sock"
port                  = 7771
log_level             = "info"
drain_timeout_seconds = 30

[memory]
path             = "~/.apollia/data/memory.db"
max_size_mb      = 512
episode_ttl_days = 90
fts5_enabled     = true

[tools]
sandbox                = false          # Linux namespaces — macOS dev: false
venv_base_path         = "~/.apollia/data/venvs"
bash_timeout_seconds   = 30
python_timeout_seconds = 60

[budget]
max_steps               = 20
max_tool_calls          = 50
wall_clock_timeout_secs = 300

[agents]
startup = ["agents/hello_agent.py"]    # auto-started when the API is ready

[notifications]
events = ["task.input_required", "task.failed", "agent.degraded"]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"   # native OS notifications — "desktop" | "webhook"
enabled = true
```

See the fully annotated `apollia.toml` at the root of this repository for all options including LLM backends, triggers, and pipelines.

---

## Triggers

Triggers fire tasks automatically based on a schedule or an external event. Declared in `apollia.toml`:

```toml
# Cron — every day at 09:00
[[triggers]]
id             = "daily-report"
agent          = "standup-scribe"
enabled        = true
on_busy        = "drop"             # "drop" | "queue" | "error"
input_template = "Daily standup for {{date_iso}}"

[triggers.source]
type     = "cron"
schedule = "0 9 * * *"

# File watch — new file in an import folder
[[triggers]]
id             = "import-docs"
agent          = "document-analyst"
enabled        = true
on_busy        = "queue"
input_template = "Analyse {{filename}} ({{size_bytes}} bytes)"

[triggers.source]
type   = "file_watch"
path   = "~/.apollia/imports/"
events = ["create"]

# Webhook — authenticated HTTP POST
[[triggers]]
id             = "ci-hook"
agent          = "code-reviewer"
enabled        = true
on_busy        = "error"
input_template = "{{webhook_body}}"

[triggers.source]
type   = "webhook"
secret = "replace-with-a-strong-secret-min-32-chars"
```

Call `POST http://127.0.0.1:7771/webhooks/ci-hook` with header `X-Apollia-Signature: <hmac-sha256>`.

Hot reload without restart:

```bash
apollia-os trigger reload
apollia-os trigger list
apollia-os trigger fire daily-report
apollia-os trigger logs daily-report
```

**Source types:** `cron` · `interval` · `oneshot` · `file_watch` · `webhook`

---

## Multi-agent Pipelines

Pipelines chain agents with explicit dependencies. Steps in the same topological layer run in parallel; downstream steps wait for their `depends_on` to complete.

```toml
[[pipelines]]
id          = "review-and-report"
description = "Review code then write a summary"
on_failure  = "fail"    # "fail" | "continue"

[[pipelines.steps]]
id    = "review"
agent = "code-reviewer"
input = "{{trigger.payload}}"

[[pipelines.steps]]
id         = "report"
agent      = "standup-scribe"
input      = "Summarise this review: {{steps.review.output}}"
depends_on = ["review"]
```

```bash
apollia-os pipeline run review-and-report "path/to/repo"
apollia-os pipeline runs review-and-report
apollia-os pipeline status <run-id>
```

A pipeline suspended for HITL approval is resumed automatically once the approval is submitted via the CLI or the desktop app.

---

## Human-in-the-Loop (HITL)

Declare sensitive tools in `tools_requiring_approval`. The runtime suspends the task before execution and waits for a human decision:

```python
def manifest(self):
    return {
        ...
        "tools_requiring_approval": ["file_io", "bash_executor"],
    }
```

Approve or reject from the CLI:

```bash
apollia-os task list --pending-approval
apollia-os task resume <task-id> --approve
apollia-os task resume <task-id> --reject --reason "Too broad a command"
```

The agent receives the decision in `task["input_response"]` and resumes exactly where it stopped. Conversation history is persisted across the suspension via the memory engine.

A configurable `TimeoutWatcher` auto-rejects approvals that exceed a deadline.

---

## Desktop App

The Tauri v2 + Svelte 5 desktop application provides a native UI for all runtime subsystems. Launch it with:

```bash
cd crates/apollia-desktop
cargo tauri dev
```

**10 routes:** Agents · Tasks · Approvals · LLM · Triggers · Pipelines · Memory · Notifications · Observability · Settings

All views update in real time via SSE streams. The system tray shows pending approval count and supports graceful quit.

---

## CLI Reference

### Level 1 — Daily operations

| Command | Description |
|---|---|
| `apollia-os start` | Start the runtime daemon |
| `apollia-os stop` | Graceful shutdown (30s task drain) |
| `apollia-os status` | Overview: agents, active tasks, tool health |
| `apollia-os run <agent> "<input>"` | Submit a task and stream the result |

### Level 2 — Full management

| Command | Description |
|---|---|
| `apollia-os agent list\|start\|stop\|info` | Manage registered agents |
| `apollia-os task list\|status\|cancel\|resume` | Manage tasks and HITL approvals |
| `apollia-os tools list\|describe` | Inspect available tools |
| `apollia-os memory inspect <ns>` | Memory namespace overview |
| `apollia-os audit list\|stats` | Tool call audit log |
| `apollia-os trigger list\|fire\|enable\|disable\|logs\|reload` | Manage triggers |
| `apollia-os pipeline list\|run\|runs\|status` | Manage pipelines |
| `apollia-os llm status\|ping\|chat` | LLM backend health and interactive chat |
| `apollia-os model list` | List GGUF models in `~/.apollia/models/` |
| `apollia-os notify test\|list\|logs` | Notification channels management |

**Global flags:** `--json` (machine output) · `-q/--quiet` · `-v/--verbose` · `--debug` · `--socket <path>`

**Exit codes:** `0` success · `1` usage error · `2` runtime error · `3` task failed · `4` timeout · `5` canceled

---

## Project Structure

```
crates/
  apollia-core/          # Shared types (AgentManifest, AIPTask, AIPResult, RuntimeEvent)
  apollia-runtime/       # Runtime Core (Supervisor, AgentRegistry, TaskRouter, APIServer)
  apollia-oria/          # ORIA Engine (Observer, StepBudget, ResilienceLayer, Reasoner)
  apollia-tools/         # Tool Registry + native tools (file_io, bash_executor, python_executor)
  apollia-memory/        # Memory Engine (SQLite, FTS5, episodic/semantic/procedural)
  apollia-aip/           # AIP Bridge (PyO3, ToolProxy, MemoryInterface, LlmProxy)
  apollia-llm/           # LLM Router + backends (embedded GGUF, Anthropic, OpenAI-compatible)
  apollia-triggers/      # Trigger Engine (cron, interval, file_watch, webhook)
  apollia-pipelines/     # Pipeline Engine (topological execution, HITL, fallback)
  apollia-notifications/ # Notification Engine (desktop, webhook channels)
  apollia-desktop/       # Desktop App (Tauri v2 + Svelte 5)
  apollia-cli/           # CLI binary (clap v4)
agents/                  # Example agents (hello_agent.py, apollia_base.py, ...)
docs/                    # Architecture and design documentation
tests/                   # End-to-end integration tests
```

---

## Contributing

1. Read [docs/Architecture-Principes.md](docs/Architecture-Principes.md) — eight non-negotiable principles.
2. Read [CLAUDE.md](CLAUDE.md) for Rust, Git, and testing conventions.
3. Create a branch: `feature/<STORY-NNN>-short-description`
4. Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before committing.
5. Open a pull request against `main`.

Bug reports and feature requests: open a [GitHub issue](https://github.com/nidal-z/apollia-os/issues).

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE-APACHE).
