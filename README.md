# Apollia OS

> The sovereign runtime for autonomous AI agents.
> They run on your machine, you can prove everything they do,
> and they are as capable as the model you plug in.

Local-first. No cloud dependency. Sovereign by design.

[![CI](https://github.com/Apollia-OS/apollia-os/actions/workflows/ci.yml/badge.svg)](https://github.com/Apollia-OS/apollia-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

---

## Why Apollia

The best model alone does not make a reliable agent. What decides the result is the system around it: the tools, the guardrails, the memory, the verification. Apollia is that system, and it runs on your machine.

- **Yours** - agents run locally; no data leaves your machine without an explicit action.
- **Auditable** - every action is recorded, so you can prove what an agent did.
- **Cost-bounded** - a step budget enforced by the runtime, never bypassable.
- **As powerful as you want** - a local model by default, any model you choose on demand.

![The audit trail of the desktop application: every tool call an agent made, with its agent, duration and status](docs/site/static/img/operator-help/observabilite-consulter-l-audit-trail-1.png)

*The audit trail. Every tool an agent invoked, immutable, with the agent that
invoked it and what it cost.*

Learn more at [apollia.fr](https://apollia.fr).

---

## What is Apollia OS?

Apollia OS is a Rust runtime that executes autonomous Python AI agents in an isolated, local environment. Agents run in-process: the PyO3 bridge translates async Rust futures to Python coroutines directly, with no per-agent subprocess.

Sovereign means self-contained, not feature-poor. The runtime is a single binary, the Python SDK has zero third-party dependencies, SQLite is vendored, the API binds to loopback by default, and there is no telemetry and no phone-home. Nothing is required from the cloud to run an agent. Every external host Apollia can reach (Anthropic, OpenAI, Google, Microsoft, and so on) is a backend or connector you configure yourself.

**Key capabilities:**

- **Local-first LLM inference** - run GGUF models locally through the embedded `llama-server` (CPU, Metal, Vulkan, CUDA depending on the platform), or connect to Anthropic / OpenAI-compatible APIs
- **Persistent memory** - three-tier SQLite store (episodic, semantic, procedural) with FTS5 full-text search per agent
- **Native tools** - bash (Linux PID/mount namespaces), path-confined file I/O, Python execution with per-agent venv isolation, HTTP fetch, web search, and more
- **Step budget** - `max_steps` / `max_tool_calls` / wall-clock timeout enforced at the runtime level, not bypassable by agent code
- **Circuit breaker** - per-tool resilience layer with exponential backoff and jitter
- **Triggers** - cron, interval, oneshot, file watch, and authenticated webhooks (HMAC-SHA256)
- **Multi-agent orchestration** - directors coordinate specialized workers over the A2A skill protocol, with human-in-the-loop suspension and resume
- **Human-in-the-Loop (HITL)** - any tool can require human approval before execution; the runtime suspends and resumes transparently
- **Desktop app** - native Tauri v2 + Svelte 5 UI, kept live for every subsystem by the Tauri event bus
- **REST API + CLI** - full management via the `apollia-os` CLI or HTTP on `127.0.0.1:7771`

---

## Install

Three ways in, and most people want the first.

**Install the desktop application.** Installers are attached to each GitHub
release: a `.dmg` for macOS Apple Silicon, an `.msi` or `.exe` for Windows
x86-64, an `.AppImage` or `.deb` for Linux x86-64, plus CUDA-engine variants for
Linux and Windows. No compiler, no checkout, no command line. The step by step,
including the checksum verification and the first-launch warnings, is in
[Install the desktop app](docs/site/docs/how-to/install-the-desktop-app.md).

**Install the command-line runtime.** The same releases attach a self-contained
archive per platform preset (`apollia-os-macos-silicon.tar.gz`,
`apollia-os-linux-x86-cpu.tar.gz`, `apollia-os-windows-x86-cpu.zip`, and their
Vulkan and Linux ARM counterparts). Unpack it, put `apollia-os` on your `PATH`,
and `apollia-os update` handles later versions from the same feed.

Apollia publishes no package on crates.io or PyPI, so the third way in is a
source build, described below.

---

## Quickstart from source

This sequence takes you from a clean clone to a running agent. The demo `echo`
agent needs no model, so it runs on any machine. Run every command from the
repository root.

**Prerequisites:** a Rust toolchain (stable), Python 3.13 available as `python3`
(the checkout pins the exact version in `.python-version`), and `git`.

```bash
# 1. Clone and build the daemon.
#    Build only the CLI crate: the default workspace build excludes the heavy
#    Tauri desktop crate, so `--workspace` is not what you want here.
git clone https://github.com/Apollia-OS/apollia-os.git
cd apollia-os
cargo build -p apollia-cli --release

# 2. Put the binary on your PATH.
#    The crate is `apollia-cli` but the binary it produces is `apollia-os`.
export PATH="$PWD/target/release:$PATH"

# 3. Install the Python SDK for your own shell: editors, tests, and the
#    `apollia` command. From this checkout the runtime does NOT read this venv:
#    it walks up from the binary until it finds `sdk/apollia/__init__.py` and
#    prepends that directory to the agent's `sys.path`.
#    Use a virtual environment. Homebrew, Debian and Fedora ship Python as an
#    externally managed environment (PEP 668), where a bare `pip install` stops
#    with `error: externally-managed-environment`.
python3 -m venv .venv
source .venv/bin/activate          # Windows: .venv\Scripts\activate
pip install -e ./sdk

# 4. Start the runtime. It runs in the FOREGROUND, so leave this terminal
#    running and open a second one for the remaining commands.
apollia-os start --port 7771

# --- in a second terminal, from the same directory ---

# A fresh shell inherits neither the PATH of step 2 nor the venv of step 3.
export PATH="$PWD/target/release:$PATH"
source .venv/bin/activate          # Windows: .venv\Scripts\activate

# 5. Install, enable, and run the no-LLM demo agent.
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os agent enable echo
apollia-os run echo "hello from Apollia"

# 6. Stop the runtime (graceful drain).
apollia-os stop
```

Expected output for step 5's `run` (the task id is a fresh UUID each time):

```
  -> Task 6f2a1c8e-... submitted to echo
echo: hello from Apollia
  * Completed in 0.2s
```

macOS note: PyO3 must find the right interpreter at build time. If your default
`python3` is not the one you want, export it before building, for example
`export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13`.

For an agent that generates text, configure a model backend (see [LLM Backends](#llm-backends)).
Full instructions, including local GGUF inference, are in
[the install guide](docs/site/docs/how-to/install-and-run.md).

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
|  TriggerEngine · NotificationEngine · PermissionsEngine               |
|                                                                        |
|  +------------------------------------------------------------------+  |
|  |                    AIP BRIDGE (PyO3)                             |  |
|  |          Rust <-> Python async · ToolProxy · MemoryInterface    |  |
|  +------------------------------+-----------------------------------+  |
+-------------------------------- | --------------------------------------+
                                  | AIP contract
                      +-----------v-----------+
                      |     PYTHON AGENT      |
                      |  @agent / @on_message |
                      |     (duck-typed)      |
                      +-----------------------+
```

Full architecture documentation: [the arc42 architecture section](docs/site/docs/architecture/)

---

## Platform Support

Apollia runs on three operating systems, over the four platform couples the
release pipeline actually builds (`packaging/artifacts.json` is the contract).
The local inference engine is the upstream `llama-server` binary, pinned and
checksum-verified; the release stages the backend upstream publishes for each
couple, and builds the CUDA engine itself for the two desktop bundles that carry
one.

| Platform | Local inference | Tool sandbox | Notes |
|----------|-----------------|--------------|-------|
| macOS Apple Silicon | Metal | `setrlimit` | Desktop `.dmg` and CLI archive |
| Linux x86_64 | Vulkan, CPU, plus CUDA in a separate desktop bundle | PID + mount namespaces, `setrlimit` | Strongest isolation of the four |
| Windows x86_64 | Vulkan, CPU, plus CUDA in a separate desktop bundle | none | `bash_executor` needs a POSIX shell on `PATH` (Git Bash, WSL or MSYS2) |
| Linux aarch64 | CPU | PID + mount namespaces, `setrlimit` | CLI archive only, no desktop bundle |

Not built for 0.1.0: macOS Intel (`x86_64-apple-darwin`), Windows ARM64, and the
AMD ROCm engine. `apollia-os update` says so for the first: it has no artifact to
offer a macOS x86-64 host.

Two asymmetries are worth stating rather than discovering. Tool subprocesses get
OS-level confinement only where the OS provides it: namespaces and resource
limits on Linux, resource limits on macOS, neither on Windows. And the shell tool
assumes POSIX semantics, because the command validator that guards it was written
for them; on Windows it uses a POSIX shell from `PATH` and refuses clearly if
there is none, instead of silently switching to a shell with different quoting
and a different injection surface. `apollia-os doctor` reports the sandbox
posture it detects and whether per-process rlimits are active; it does not probe
for the POSIX shell, so on Windows the first `bash_executor` call is what tells
you whether one is there.

---

## LLM Backends

The `apollia-os` binary talks to Anthropic, OpenAI, Mistral, Ollama and any
other OpenAI-compatible endpoint (LM Studio, vLLM, a self-hosted gateway), and
serves local GGUF models through an embedded `llama-server` (upstream
llama.cpp) that the daemon spawns and supervises.

Google Vertex AI is the exception to that list. Its backend exists and the
router loads it, but `--provider` has no `vertex` value, so it is configured from
`apollia.toml` alone, in a `[llm.vertex]` section authenticated by Application
Default Credentials. It does not stream.

Ollama needs no API key and runs anywhere you can reach over HTTP, including
another machine on your network:

```bash
apollia-os llm backends create ollama-local --provider ollama --model qwen2.5:14b --default
apollia-os llm backends create ollama-remote --provider ollama --model qwen2.5:14b \
  --base-url http://192.168.1.20:11434/v1
```

### Cloud backends

Configure a provider from the CLI:

```bash
apollia-os llm backends create prod --provider anthropic \
  --model claude-sonnet-4-6 --api-key "$ANTHROPIC_API_KEY" --default
apollia-os llm status
```

Cloud API backends can also be declared in `apollia.toml`. Only `type = "api"`
backends are file-representable:

```toml
[llm]
default = "anthropic"

[[llm.backends]]
name        = "anthropic"
type        = "api"
provider    = "anthropic"
model       = "claude-haiku-4-5"
api_url     = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
```

Mind the base URL: the Anthropic client appends `/v1/messages` itself, so its
`api_url` stops at the host. Every OpenAI-compatible provider is the opposite,
its base must already end in `/v1` because `/chat/completions` is appended to
it. The desktop settings dialog prefills the right shape per provider.

Any OpenAI-compatible endpoint (OpenAI, Mistral, Ollama, LM Studio, vLLM, and so on)
works the same way with `provider = "openai"` and the matching `api_url`.

### Local GGUF inference

Local models are not configured with a `type = "embedded"` block; they are
registered through the CLI, and the daemon serves them through the embedded
`llama-server` (upstream llama.cpp) over its OpenAI-compatible HTTP API, with
native tool calling (`--jinja`) and continuous batching. The provider name is
`llama-cpp`. Register the model:

```bash
apollia-os llm setup --local --model /path/to/model.gguf
apollia-os llm reload
```

A packaged build stages `llama-server` automatically. On a source build the
daemon looks for `llama-server` on your `PATH`; the repository provides a recipe
to run one for local testing:

```bash
just llama-server /path/to/model.gguf
```

An upstream install works too (for example `brew install llama.cpp` on macOS, or
a llama.cpp build on Linux). Place any `.gguf` file under `~/.apollia/models/`. If
a local backend is configured but no `llama-server` is reachable, LLM calls fail
with a `503 BackendUnavailable`.

Multiple backends can coexist. `default` selects which one agents use unless they
override it in their manifest.

---

## Writing an Agent

An Apollia agent is an ordinary Python class. The SDK decorators declare the
manifest and the entry points; no base class, no inheritance. Every agent module
ends with `agent = MyClass()`, which is what the runtime loads. Use absolute
imports (`from apollia import ...`), never relative ones.

### Conversational agent

```python
# coach.py
from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="coach",
    version="0.1.0",
    description="Friendly product coach.",
)
class Coach:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": "You are a helpful coach."},
                *history,
                {"role": "user", "content": message},
            ],
        )
        return response.content
```

- **`@agent(...)`** declares the manifest. `name`, `version`, and `description` are required.
- **`@on_message`** marks the single conversational entry point. Its signature is fixed: `(self, message, history, ctx)` returning the reply as a string.

Install, enable, and talk to it:

```bash
apollia-os agent install ./coach.py
apollia-os agent enable coach
apollia-os run coach "How does the Director pattern work?"
```

### Workers and directors

- A **worker** exposes typed capabilities as A2A skills with `@skill`, invocable by any director.
- A **director** orchestrates workers by calling `react(...)`, the ReAct (Reason + Act) loop utility exported from `apollia`:

```python
from apollia import agent, on_message, react


@agent(name="director", version="0.1.0", description="Coordinates workers.")
class Director:
    @on_message
    async def chat(self, message, history, ctx):
        return await react(
            ctx,
            system="You are a director agent.",
            user=message,
            tools=[
                await ctx.a2a.skill_as_tool("pdf.read_text"),
                await ctx.a2a.skill_as_tool("web.search"),
            ],
            max_steps=10,
        )
```

`react` delegates the `LLM -> tool(s) -> LLM -> ... -> final answer` cycle to the
runtime, enforces an explicit `max_steps` budget, and returns the final answer as
a string. Full tutorials: [Your first agent](docs/site/docs/tutorials/your-first-agent.md)
and the how-to guides for [workers](docs/site/docs/how-to/write-a-worker.md) and
[directors](docs/site/docs/how-to/write-a-director.md).

### Runtime context (`ctx`)

The `ctx` object exposes the runtime's typed services to your handler. The most
common ones:

| Attribute | Description |
|---|---|
| `ctx.llm` | Text generation: `await ctx.llm.complete(messages)` |
| `ctx.tools` | Native tool calls: `await ctx.tools.call("tool_name", args)`. `None` unless the agent declares `tools_required=("tool_name", ...)` in `@agent(...)` |
| `ctx.memory` | Persistence: `record`, `recall`, `search`, `forget` (opt in per agent) |
| `ctx.a2a` | Call other agents' skills |
| `ctx.logger` | Structured logging routed to the runtime tracer |

Several services degrade to `None` when the agent does not opt into them (for
example `ctx.memory` without a `memory_namespace`); check before use. The full
contract is documented in the [SDK / ctx reference](docs/site/docs/reference/sdk/).

### Native tools

The runtime ships a set of native tools that agents call through `ctx.tools`.
Run `apollia-os tools list` for the live catalog with feature-flag and credential
status. The current set:

| Category | Tools |
|---|---|
| Shell / code | `bash_executor`, `python_executor` |
| Files | `file_read`, `file_write`, `file_list`, `file_edit`, `file_glob`, `file_grep` |
| Notebooks | `notebook_read`, `notebook_edit` |
| Web | `http_fetch`, `web_search`, `web_read` |
| Memory | `memory_search` |
| Permissions | `permission_rule_add`, `permission_rule_list`, `permission_rule_remove` |
| Human input | `ask_user` |

---

## Configuration

Runtime behaviour is controlled by an `apollia.toml` file. The CLI resolves it in
this order: an explicit `--config` override, then `./apollia.toml` in the working
directory, then `$XDG_CONFIG_HOME/apollia/apollia.toml` (defaulting to
`~/.config/apollia/apollia.toml`). Runtime state (the API token, SQLite databases,
downloaded models) lives separately under `~/.apollia/`. Paths in the file support
`~` expansion.

The recognized top-level sections are `[llm]`, `[runtime]`, `[tools]`, `[api]`,
`[hitl]`, `[mcp]`, `[hooks]`, `[chat]`, and `[filesystem]`. Any other section is
rejected by `config set`, and a file that still carries one logs a warning at
startup rather than dropping it silently.

`[memory]`, `[budget]`, `[a2a]`, `[oria]`, `[registry]` and `[permissions]` used
to be accepted and are not. `[memory]` and `[budget]` never had a field to
deserialize into at all; the other four did, and that structure was then never
consulted, so a value written in any of the six never had an effect.
`[permissions]` is the one worth naming, since it reads as though it governs
something: the governance path that does run is the prefix-rule engine, and it
takes nothing from this file.

Triggers, notifications, speech-to-text, and installed agents are managed
through the CLI and the desktop app (persisted in SQLite), not through this
file.

Inspect and edit the live config with the `config` command:

```bash
apollia-os config show
apollia-os config get llm
```

The full section-by-section surface is in the
[configuration reference](docs/site/docs/reference/configuration.md) and the
[CLI reference](docs/site/docs/reference/cli/).

---

## Triggers

Triggers fire tasks automatically on a schedule or an external event. They are
managed through the CLI (and the desktop app), which persists them:

```bash
apollia-os trigger create daily-report --agent reporter --kind cron --detail '0 9 * * *'
apollia-os trigger list
apollia-os trigger fire daily-report
apollia-os trigger enable daily-report
apollia-os trigger logs daily-report
apollia-os trigger reload
```

**Source types:** `cron` · `interval` · `oneshot` · `file_watch` · `webhook`

A webhook trigger authenticates with an HMAC-SHA256 signature. Call
`POST http://127.0.0.1:7771/webhooks/<trigger-id>` with header
`X-Apollia-Signature: sha256=<hex-digest>`. The `sha256=` prefix is part of the
value: without it the runtime answers 401.

---

## Human-in-the-Loop (HITL)

A tool named in the agent manifest's `tools_requiring_approval` suspends the task
before that tool runs and waits for a human decision. The gate is enforced on the
orchestrated path, step by step, before ORIA executes a step whose tool is in the
list, plus on `mailbox:send` wherever it is called.

Set it in the manifest, not in Python: the `@agent` decorator has no
`tools_requiring_approval` parameter today, so the field is written in the
`manifest.json` of an agent package and read from there by the runtime.
`apollia-os agent validate` echoes back what it found, which is the way to check
that the declaration took.

```json
{
  "name": "reviewer",
  "version": "0.1.0",
  "description": "Reviews and edits code.",
  "tools_requiring_approval": ["bash_executor", "file_write"]
}
```

Approve or reject from the CLI:

```bash
apollia-os task list --pending-approval
apollia-os task resume <task-id> --approve
apollia-os task resume <task-id> --reject --reason "Too broad a command"
```

The agent resumes exactly where it stopped; conversation history is persisted
across the suspension by the memory engine. A configurable timeout watcher
auto-rejects approvals that exceed a deadline.

---

## Security model

Apollia OS is local-first and defends in layers. Be precise about what is and is
not enforced:

- **Network.** The API binds to `127.0.0.1` by default (loopback only). Binding
  to `0.0.0.0` is opt-in. No telemetry, no phone-home.
- **Bash isolation.** On Linux, `bash_executor` runs each command under PID and
  mount namespaces (`unshare --pid --mount --fork`). On macOS and Windows there
  is no namespace isolation: the executor logs a dev-mode warning on every call,
  and production deployments are expected to run on Linux. There is no seccomp
  syscall filtering and no network namespace, so a shell command can still reach
  the network. Treat bash as an isolated process tree, not an untrusted-code
  container.
- **File tools.** File access is confined to a canonicalized sandbox root; any
  path that resolves outside the root is rejected (path-traversal safe).
- **Step budget.** `max_steps`, `max_tool_calls`, and a wall-clock timeout are
  enforced by the runtime and cannot be bypassed by agent code.
- **Human-in-the-Loop.** Sensitive tools can require explicit human approval
  before execution.
- **Secrets.** Credentials are stored through the OS keychain or an encrypted
  age file, never in plaintext config.

For the threat model, scope, and private reporting, see [SECURITY.md](SECURITY.md).

---

## Desktop App

The Tauri v2 + Svelte 5 desktop application provides a native UI for all runtime subsystems. Build the UI once, then launch it through the repository recipe (requires the `cargo tauri` CLI):

```bash
just desktop-ui-install          # npm ci in crates/apollia-desktop/ui
just desktop-dev                 # links PyO3 against the bundled interpreter, then `cargo tauri dev`
```

Use the recipe rather than `cargo tauri dev` on its own. `just desktop-dev`
first points `PYO3_PYTHON` and `RUSTFLAGS` at the interpreter bundle under
`target/`, which is the one the application sets `PYTHONHOME` to at run time.
Without that step the two interpreters differ, every agent dies at boot on
`ModuleNotFoundError`, and the failure surfaces only as a warning nobody reads.
The recipe needs that bundle to exist: build it once with
`bash packaging/build-python-bundle.sh <target-triple> target/python-bundle/<target-triple>`.

**Main routes:** Dashboard · Agents · Projects · Tasks · Chat · Inbox · Connections · LLM · Automations · Memory · Transcriptions · Notifications · Observability · Settings

Views update in real time over the Tauri event bus, not over SSE; the HTTP API is where SSE lives, on `GET /api/v1/tasks/{id}/stream`. The system tray shows the pending approval count and supports graceful quit.

**Updates.** Releases are published on [GitHub Releases](https://github.com/Apollia-OS/apollia-os/releases). The desktop app checks that feed only when you ask it to, from Settings, and never in the background. Until a release is published the check reports that there is nothing newer, rather than failing.

---

## CLI Reference

### Level 1 - Daily operations

| Command | Description |
|---|---|
| `apollia-os start` | Start the runtime (foreground) |
| `apollia-os stop` | Graceful shutdown (task drain) |
| `apollia-os status` | Overview: agents, active tasks, tool health |
| `apollia-os run <agent> "<input>"` | Submit a task and print the result |
| `apollia-os doctor` | Diagnose the local environment (no runtime required) |

### Level 2 - Full management

| Command | Description |
|---|---|
| `apollia-os agent list\|install\|enable\|disable\|start\|stop\|show` | Manage agents |
| `apollia-os task list\|status\|cancel\|resume\|approvals` | Manage tasks and HITL approvals |
| `apollia-os a2a skills\|invoke` | Discover and invoke worker skills |
| `apollia-os tools list\|show\|enable\|disable\|credentials` | Inspect and govern native tools |
| `apollia-os memory` | Memory management |
| `apollia-os audit list\|stats\|verify\|export` | Tool-call audit log |
| `apollia-os trigger list\|fire\|enable\|disable\|logs\|reload` | Manage triggers |
| `apollia-os llm status\|ping\|chat\|backends\|reload` | LLM backend health and interactive chat |
| `apollia-os model list\|search\|show\|delete` | Local GGUF model files in `~/.apollia/models/` |
| `apollia-os mcp list\|add\|remove\|test` | Manage MCP servers |
| `apollia-os notify test\|list\|logs\|events` | Notification channel management |
| `apollia-os config show\|set` | Inspect and edit `apollia.toml` |

**Global flags:** `--json` (machine output) · `-q/--quiet` · `-v/--verbose` · `--debug` · `--socket <path>`

**Exit codes:** `0` success · `1` usage error · `2` runtime error · `3` task failed · `4` timeout · `5` interrupted (`start` stopped by Ctrl+C)

Every flag on every command is in the [CLI reference](docs/site/docs/reference/cli/).

---

## Project Structure

```
crates/
  apollia-core/          # Shared types + config schema (AgentManifest, AIPTask, AIPResult, RuntimeEvent)
  apollia-runtime/       # Runtime core (Supervisor, AgentRegistry, TaskRouter, EventBus, axum API)
  apollia-oria/          # ORIA engine (Observer, Reasoner, Actor, StepBudget, ResilienceLayer)
  apollia-aip/           # AIP bridge (PyO3, ctx services, ToolProxy, MemoryInterface, LlmProxy)
  apollia-llm/           # LLM router (embedded llama-server for local GGUF + Anthropic, OpenAI-compatible, Vertex cloud)
  apollia-runner/        # Out-of-process speech-to-text runner sidecar (whisper)
  apollia-tools/         # Native tool registry + sandbox
  apollia-memory/        # Memory engine (SQLite, FTS5, episodic/semantic/procedural)
  apollia-triggers/      # Trigger engine (cron, interval, oneshot, file_watch, webhook)
  apollia-notifications/ # Notification engine (desktop, webhook channels)
  apollia-mcp/           # MCP client transports
  apollia-stt/           # Speech-to-text (whisper)
  apollia-permissions/   # Permissions engine (safelist, injection detection)
  apollia-workspace/     # Workspace inspection and initialization
  apollia-auth/          # OAuth2 PKCE authentication
  apollia-connectors/    # Native SaaS connectors (Google Workspace, Microsoft 365)
  apollia-prompts/       # Unified prompt corpus
  apollia-eval/          # Evaluation harness
  apollia-cli/           # CLI binary (clap v4, produces the `apollia-os` binary)
  apollia-desktop/       # Desktop app (Tauri v2 + Svelte 5)
sdk/                     # Python SDK (the `apollia` package)
clients/                 # Generated client SDKs + example agents (echo_agent.py, demo_driver.py)
agents/                  # Example agents (examples/hello/agent.py)
docs/                    # Documentation (Docusaurus site, LLM rulebook)
tests/                   # End-to-end integration tests
scripts/                 # Tooling and desktop E2E automation
```

---

## Contributing

Apollia OS is a single-maintainer preview. **Issues are
welcome, pull requests are auto-closed by policy.** See
[CONTRIBUTING.md](CONTRIBUTING.md) for the full rationale and the right
channel for each kind of feedback.

- Found a bug? [Open an issue](https://github.com/Apollia-OS/apollia-os/issues/new?template=bug_report.yml).
- Have a feature idea? [Open an issue](https://github.com/Apollia-OS/apollia-os/issues/new?template=feature_request.yml).
- Usage question? [Discussions Q&A](https://github.com/Apollia-OS/apollia-os/discussions/categories/q-a).
- Security vulnerability? [Private advisory](https://github.com/Apollia-OS/apollia-os/security/advisories/new).

---

## Support Apollia OS

Apollia OS is built in the open, under a permissive license. There is no cloud
backend to upsell and no telemetry to monetize. If the project is useful to you,
or you want to see it reach a stable v1.0, recurring support is what keeps the
work going.

- **[Patreon](https://patreon.com/apollia)** - recurring support, with patron-only
  development updates and a vote on what ships next.
- **[Ko-fi](https://ko-fi.com/apollia)** - a one-time tip, no account or
  subscription required.

GitHub Sponsors is not open yet: the organisation has no Sponsors profile, so
the button GitHub renders from `.github/FUNDING.yml` leads nowhere. This list
gains the rail when the profile goes live.

Funding goes straight into the work: cross-platform CI, vision support, and the
foundations for distributing community agents. Supporters are listed in
[SPONSORS.md](SPONSORS.md).

This is separate from the commercial side. If you need a custom agent built for
your own workflow, that is a paid engagement, see [apollia.fr](https://apollia.fr).

---

## License

Apollia OS is dual-licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option. This aligns with the de facto standard for the Rust ecosystem.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
