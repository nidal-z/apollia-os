# Apollia OS

> The sovereign runtime for autonomous AI agents.
> They run on your machine, you can prove everything they do,
> and they are as capable as the model you plug in.

Local-first. No cloud dependency. Sovereign by design.

**Apollia Desktop** is the application: you install it, then create, run and
supervise agents from its window, without writing code. **Apollia OS** is the
engine under it, with its `apollia-os` command line and its Python SDK: what
this repository builds, and what a developer drives directly.

[![CI](https://github.com/Apollia-OS/apollia-os/actions/workflows/ci.yml/badge.svg)](https://github.com/Apollia-OS/apollia-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Release](https://img.shields.io/github/v/release/Apollia-OS/apollia-os)](https://github.com/Apollia-OS/apollia-os/releases)
[![Docs](https://img.shields.io/badge/docs-docs.apollia.fr-blue.svg)](https://docs.apollia.fr)

---

## Why Apollia

The best model alone does not make a reliable agent. What decides the result is the system around it: the tools, the guardrails, the memory, the verification. Apollia is that system, and it runs on your machine.

- **Yours** - agents run locally; no data leaves your machine without an explicit action.
- **Auditable** - every action is recorded, so you can prove what an agent did.
- **Cost-bounded** - a step budget enforced by the runtime, never bypassable.
- **As powerful as you want** - a local model by default, or any model served by an inference server you designate.

Learn more at [apollia.fr](https://apollia.fr).

---

## What is Apollia OS?

Apollia OS is a Rust runtime that executes autonomous Python AI agents in an isolated, local environment. Agents run in-process: the PyO3 bridge translates async Rust futures to Python coroutines directly, with no per-agent subprocess.

Sovereign means self-contained, not feature-poor. The runtime is a single binary, the Python SDK has zero third-party dependencies, SQLite is vendored, the API binds to loopback by default, and there is no telemetry and no phone-home. Nothing is required from the cloud to run an agent. Every external host Apollia can reach (an inference server, a Google or Microsoft account, an MCP server) is a backend or connector you configure yourself.

**Key capabilities:**

- **Local-first LLM inference** - run GGUF models locally through the embedded `llama-server` (CPU, Metal, Vulkan, CUDA depending on the platform), or connect to an OpenAI-compatible server or to Ollama
- **Persistent memory** - three-tier SQLite store (episodic, semantic, procedural) with FTS5 full-text search per agent
- **Native tools** - bash (Linux PID/mount namespaces), path-confined file I/O, Python execution with per-agent venv isolation, HTTP fetch, web search, and more
- **Step budget** - `max_steps` / `max_tool_calls` / wall-clock timeout enforced at the runtime level, not bypassable by agent code
- **Circuit breaker** - per-tool resilience layer with exponential backoff and jitter
- **Triggers** - cron, interval, oneshot, file watch, and authenticated webhooks (HMAC-SHA256)
- **Multi-agent orchestration** - directors coordinate specialized workers over the A2A skill protocol, with human-in-the-loop suspension and resume
- **Human-in-the-Loop (HITL)** - any tool can require human approval before execution; the runtime suspends and resumes transparently
- **Apollia Desktop** - the desktop application, a native Tauri v2 + Svelte 5 UI, kept live for every subsystem by the Tauri event bus
- **REST API + CLI** - full management via the `apollia-os` CLI or HTTP on `127.0.0.1:7771`

---

## Install

Three ways in, and most people want the first.

**Install Apollia Desktop, the application.** Installers are attached to each GitHub
release: a `.dmg` for macOS Apple Silicon, an `.msi` or `.exe` for Windows
x86-64, an `.AppImage` or `.deb` for Linux x86-64: one installer per system.
No compiler, no checkout, no command line. The step by step,
including the checksum verification and the first-launch warnings, is in
[Install the desktop app](https://docs.apollia.fr/how-to/install-the-desktop-app/).

**Install the command-line runtime.** The same releases attach a self-contained
archive per platform preset (`apollia-os-macos-silicon.tar.gz`,
`apollia-os-linux-x86-cpu.tar.gz`, `apollia-os-windows-x86-cpu.zip`, and their
Vulkan and Linux ARM counterparts). Unpack it, put `apollia-os` on your `PATH`,
and `apollia-os update` handles later versions from the same feed.

Apollia publishes no package on crates.io or PyPI, so the third way in is a
source build, described below.

---

## Quickstart from source

This sequence takes you from a clean clone to a workflow run. The demo `echo`
package needs no model, so it runs on any machine. Run every command from the
repository root.

**Prerequisites:** a Rust toolchain (stable), Python 3.13 available as `python3`
(the checkout pins `3.13.13` in `.python-version`, the version the release
bundles embed), and `git`.

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

# 5. Build and install the demo package: one workflow, no agent, no model.
apollia-os package build clients/examples/echo
apollia-os package install echo-0.1.0.apo

# 6. Restart the runtime: it reads the workflows of the installed packages
#    when it starts. Stop it here, then start it again in the first terminal
#    with the command of step 4.
apollia-os stop

# 7. Run the workflow and follow it to its end.
apollia-os workflow run echo/echo --input message="hello from Apollia" --wait

# 8. Stop the runtime (graceful drain).
apollia-os stop
```

Expected output for step 7 (the run id and the timestamps are fresh each time):

```
Run        : 96e20c50-...
Workflow   : echo/echo
Status     : completed
Started    : 2026-10-07T12:03:57.612Z
Ended      : 2026-10-07T12:03:57.665Z
Output     : {"echo":"hello from Apollia"}

  #    STEP                     STATUS       STARTED                    ENDED
  0    echo                     completed    2026-10-07T12:03:57.663Z   2026-10-07T12:03:57.664Z
```

macOS note: PyO3 must find the right interpreter at build time. If your default
`python3` is not the one you want, export it before building, for example
`export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13`.

For an agent that generates text, configure a model backend (see [LLM Backends](#llm-backends)).
Full instructions, including local GGUF inference, are in
[the install guide](https://docs.apollia.fr/how-to/install-and-run/).

---

## Architecture

Apollia OS is built around independent Tokio actors communicating over channels. Zero shared mutable state between actors.

```mermaid
flowchart TB
    subgraph rt["Apollia OS runtime"]
        sup["Supervisor · AgentRegistry · TaskRouter · EventBus · APIServer"]
        coord["ExecutionCoordinator<br/>one per active agent"]
        oria["ORIA engine · Observer, Reasoner, Actor<br/>Direct / Orchestrated · StepBudget · ResilienceLayer"]
        tools["Tool registry<br/>+ sandbox"]
        mem["Memory engine<br/>SQLite / FTS5"]
        llm["LLM router<br/>embedded engine · designated server"]
        side["TriggerEngine · NotificationEngine · PermissionsEngine"]
        aip["AIP bridge (PyO3)<br/>Rust and Python async · ToolProxy · MemoryInterface"]
    end
    agent["Package Python<br/>@tool / @workflow"]

    sup --> coord --> oria
    oria --> tools
    oria --> mem
    oria --> llm
    oria --> aip
    aip -- "AIP contract" --> agent
```

Full architecture documentation: [the arc42 architecture section](https://docs.apollia.fr/architecture/)

---

## Platform Support

Apollia runs on three operating systems, over the four platform couples the
release pipeline actually builds (`packaging/artifacts.json` is the contract).
The local inference engine is the upstream `llama-server` binary, pinned and
checksum-verified; the release stages the backend upstream publishes for each
couple. The CUDA engine is in no installer: it is published apart, under a
signed manifest, and the application downloads it on demand on a machine with
an NVIDIA card.

| Platform | Local inference | Tool sandbox | Notes |
|----------|-----------------|--------------|-------|
| macOS Apple Silicon | Metal | `setrlimit` | Desktop `.dmg` and CLI archive |
| Linux x86_64 | Vulkan, CPU, plus CUDA downloaded on demand by the desktop app | PID + mount namespaces, `setrlimit` | Strongest isolation of the four |
| Windows x86_64 | Vulkan, CPU, plus CUDA downloaded on demand by the desktop app | none | `bash_executor` needs a POSIX shell on `PATH` (Git Bash, WSL or MSYS2) |
| Linux aarch64 | CPU | PID + mount namespaces, `setrlimit` | CLI archive only, no desktop bundle |

Not built: macOS Intel (`x86_64-apple-darwin`), Windows ARM64, and the
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

Apollia serves local GGUF models through an embedded `llama-server` that the
daemon spawns and supervises, and reaches two kinds of external server: any
OpenAI-compatible endpoint at an address you designate (LM Studio, vLLM, a
self-hosted gateway) and Ollama. There is no dedicated backend per cloud
vendor. A local model is the default; an external server is a choice you make,
per agent or per conversation.

Declaring a backend:
[configuration reference](https://docs.apollia.fr/reference/configuration/).
Getting the most out of a local model:
[accelerate local inference](https://docs.apollia.fr/how-to/accelerate-local-inference/).

## Writing an Agent

An Apollia agent is a record, not a program: a mission, a model, the tools it
is granted, the sources it may read, and its rights. It is written as an
`[[agents]]` entry in the `apollia.toml` manifest of a package, or created in
the desktop application without a line of code. Code is optional: the Python of
a package brings tools (`@tool`) and workflows (`@workflow`), and nothing else.

### An agent and its tool

```toml
# coach/apollia.toml
[package]
name = "coach"
version = "0.1.0"
publisher = "Example"
description = "A friendly product coach."
sdk = ">=0.3, <0.4"

[[agents]]
id = "coach"
name = "Coach"
mission = """
You are a product coach. Answer briefly, and count the words of a draft with
your tool before you comment on its length.
"""
examples = ["Is this release note too long?"]

[agents.defaults]
access = ["pkg:coach/word_count"]
autonomy = "assisted"

[[tools]]
name = "word_count"
module = "tools/word_count.py"
description = "Count the words of a text."
action = "read"
inputs = { type = "object", properties = { text = { type = "string", description = "The text to count." } }, required = ["text"], additionalProperties = false }
```

```python
# coach/tools/word_count.py
from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx


@tool("word_count", "Count the words of a text.", action="read")
async def word_count(
    ctx: Ctx,
    text: Annotated[str, "The text to count."],
) -> dict[str, Any]:
    return {"words": len(text.split())}
```

- **`[[agents]]`** is the agent. Its `mission` is its instructions, and
  `[agents.defaults]` is what the package recommends: the person who installs it
  can grant or revoke a tool, change the model or the autonomy tier, and the
  local choice wins.
- **`@tool`** declares a tool any agent can be granted. `[[tools]]` is its
  descriptor, written by `python -m apollia describe coach`, so the runtime
  reads the manifest without running the code.

`apollia-os package new coach` writes this starting folder. Build the archive,
install it, and talk to the agent:

```bash
apollia-os package build coach
apollia-os package install coach-0.1.0.apo
apollia-os run coach "Is this release note too long? ..."
```

The runtime refuses a package whose manifest does not validate: an unknown key
is named with its full path instead of being dropped in silence.

### Beyond a conversation

An agent and a workflow differ by who decides the next step: the model for an
agent, the author for a workflow. A **workflow** is an async function under
`@workflow` whose steps are declared ahead; a step can run an agent, a tool or
another workflow.

Inside a tool or a workflow, `ctx` is the whole runtime surface: `ctx.llm` for
the model, `ctx.memory` for what is remembered, `ctx.tools` for the native tools
(filesystem, shell, HTTP, search), `ctx.logger` for the trace. Every tool call
goes through the permission engine and lands in the audit journal.

- [Your first agent](https://docs.apollia.fr/tutorials/your-first-agent/)
- [Package and distribute an agent](https://docs.apollia.fr/how-to/package-and-distribute-an-agent/)
- [SDK reference](https://docs.apollia.fr/reference/sdk/), including the full
  `ctx` contract and the native tools


## Configuration

Configuration lives in `~/.apollia/apollia.toml`, and every key is also
reachable from `apollia-os config`. Nothing is required to start: the defaults
run a local model against a local workspace.

Every key, its default and its effect:
[configuration reference](https://docs.apollia.fr/reference/configuration/).

## Triggers

An agent can be woken by a schedule, a file that changes, or a webhook, rather
than by you. Triggers are declared in the desktop application or through
`apollia-os trigger`, and each one carries what happens when the agent is
already busy.

The trigger types and their options:
[`apollia-os trigger`](https://docs.apollia.fr/reference/cli/).

## Human-in-the-Loop

An agent that reaches a boundary stops and asks. Filesystem writes outside its
workspace, a tool marked as requiring approval, a question it cannot answer on
its own: each one becomes a request you answer, in the application or from the
CLI, and each answer is recorded in the audit journal.

How approvals are asked, scoped and revoked:
[human in the loop](https://docs.apollia.fr/how-to/human-in-the-loop/).

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

## Apollia Desktop

Apollia Desktop, the Tauri v2 + Svelte 5 desktop application, provides a native UI for all runtime subsystems. Build the UI once, then launch it through the repository recipe (requires the `cargo tauri` CLI):

```bash
just desktop-ui-install          # npm ci in crates/apollia-desktop/ui
just desktop-dev                 # links PyO3 against the bundled interpreter, then `cargo tauri dev`
```

Use the recipe rather than `cargo tauri dev` on its own. `just desktop-dev`
first points `PYO3_PYTHON` and `RUSTFLAGS` at the interpreter bundle under
`target/`, which is the one the application sets `PYTHONHOME` to at run time.
Without that step the two interpreters differ and every agent fails at boot
with `ModuleNotFoundError`, which the application reports as a log warning
rather than as a startup error.
The recipe needs that bundle to exist: build it once with
`bash packaging/build-python-bundle.sh <target-triple> target/python-bundle/<target-triple>`.

**Screens:** Home · Agents (with their batch runs) · Chat · Inbox · Connections · LLM models · Library · Automations · Memory · Transcriptions · Observability · Organization policy · Permissions · Journal · Settings

Views update in real time over the Tauri event bus, not over SSE; the HTTP API is where SSE lives, on `GET /api/v1/tasks/{id}/stream`. The system tray shows the pending approval count and supports graceful quit.

**Updates.** Releases are published on [GitHub Releases](https://github.com/Apollia-OS/apollia-os/releases). Apollia Desktop checks that feed only when you ask it to, from Settings, and never in the background. Until a release is published the check reports that there is nothing newer, rather than failing.

---

## CLI Reference

`apollia-os` covers the same surface as the desktop application: agents, tasks,
chat, memory, triggers, permissions, models, connectors and the audit journal.
Every command answers `--json` for scripting, and exit codes are part of the
contract (0 success, 1 usage, 2 runtime unavailable).

The full command tree: [CLI reference](https://docs.apollia.fr/reference/cli/).

## Project Structure

A Cargo workspace of Rust crates, a Python SDK under `sdk/`, the desktop
application under `crates/apollia-desktop/`, and the documentation site under
`docs/site/`.

What each crate holds and why it exists:
[repository layout](https://docs.apollia.fr/explanation/repository-layout/).

## Roadmap

What shipped, what is being built, and the versions planned after it:
[ROADMAP.md](ROADMAP.md).

## Contributing

Apollia OS is maintained by the Apollia team. How to report a bug, request a
feature or ask a question, and what the project does with a pull request, is in
[CONTRIBUTING.md](CONTRIBUTING.md).

- Found a bug? [Open an issue](https://github.com/Apollia-OS/apollia-os/issues/new?template=bug_report.yml).
- Have a feature idea? [Open an issue](https://github.com/Apollia-OS/apollia-os/issues/new?template=feature_request.yml).
- Usage question? [Discussions Q&A](https://github.com/Apollia-OS/apollia-os/discussions/categories/q-a).
- Security vulnerability? [Private advisory](https://github.com/Apollia-OS/apollia-os/security/advisories/new).

## Project

- [CHANGELOG.md](CHANGELOG.md) - what changed in each release.
- [ROADMAP.md](ROADMAP.md) - where the next releases are heading.
- [GOVERNANCE.md](GOVERNANCE.md) - who maintains the project and how decisions
  are made.
- [Code of Conduct](.github/CODE_OF_CONDUCT.md) - the rules for every space tied
  to the repository.
- [SECURITY.md](SECURITY.md) - supported versions, private reporting, and how to
  verify a release.

---

## Support Apollia OS

Apollia OS is built in the open, under a permissive license. There is no cloud
backend to upsell and no telemetry to monetize. If the project is useful to
you, you can back the work:

- **[Patreon](https://patreon.com/apollia)** - recurring support, with patron-only
  development updates and a vote on what ships next.
- **[Ko-fi](https://ko-fi.com/apollia)** - a one-time tip, no account or
  subscription required.
- **[apollia.fr/soutenir](https://apollia.fr/soutenir)** - the support page,
  with every way to help in one place.

Funding goes straight into the work: cross-platform CI, vision support, and the
foundations for distributing community agents.

This is separate from the commercial side. If you need a custom agent built for
your own workflow, that is a paid engagement, see [apollia.fr](https://apollia.fr).

---

## License

Apollia OS is dual-licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

The third-party components the release packages redistribute, and their
licenses, are listed in [NOTICE](NOTICE).

at your option. This aligns with the de facto standard for the Rust ecosystem.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
