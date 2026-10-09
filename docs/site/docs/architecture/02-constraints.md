---
sidebar_position: 2
title: 2. Constraints
---

# 2. Constraints

The architecture is bounded by a small set of non-negotiable rules. They are not
preferences; they are the reason the system is shaped the way it is. Most derive
from the eight principles that govern the project.

## The eight principles as constraints

1. **Local-first.** Zero user data leaves the machine without an explicit
   action. This forbids silent telemetry and shapes every default toward the
   local path.
2. **Zero external runtime dependency.** The binary runs on a clean Linux
   machine with nothing pre-installed. Inference, storage, and the API are all
   in-process or sidecar, never a required external service.
3. **Minimal contract.** An agent is a record: a mission, a model, tools,
   sources and rights, declared in the manifest of a package. Code is optional:
   a package brings tools and workflows, and nothing else.
4. **Fail fast.** Any error detectable at startup is detected at startup, not
   mid-run.
5. **One actor, one responsibility.** The runtime core is a set of Tokio actors
   with no shared mutable state between them. They communicate only by message.
6. **Memory at agent initiative.** The runtime never auto-injects memory context
   into an agent's prompt, and writes nothing to memory on its behalf. An agent
   recalls with the tool `memory_search` and keeps a note with `memory_write`,
   when it chooses to; checking Memory on its sheet grants both. The built-in
   conversational assistant is the one place that does inject, in two ways, and
   [the eight principles](/explanation/the-8-principles) says which.
7. **Non-negotiable safeguards.** A step budget is enforced by the runtime and
   cannot be bypassed by an agent.
8. **Human CLI, machine API.** The CLI is for people (TTY-aware, a global
   `--json`); the API is for programs.

The authoritative statement of the principles lives in the project rulebook
(`AGENTS.md`). This section treats them as fixed inputs.

## Technical constraints

<!-- claim:daemon-binds-tcp-by-default -->
The transport entry below is the one an integrator most often reads backwards:
the daemon binds TCP on every start, and only the embedded runtime defaults to
Unix-socket-only.

- **Language and runtime.** The core is Rust (1.89+) on Tokio. Errors use
  `thiserror` enums, not `anyhow`, so failures stay typed and map to exit codes
  and structured traces. No `unwrap`, `panic`, or `println` in production paths.
- **Python bridge.** The Python a package brings (its tools and its workflows)
  is executed in-process through a PyO3 bridge with `pyo3-async-runtimes`. The
  Rust side owns the process; Python is the guest. The SDK source floor is
  Python 3.10, and the runtime embeds its own CPython 3.13.
- **Inference.** Local inference is the embedded `llama-server` (upstream
  llama.cpp), on GGUF models, over its OpenAI-compatible HTTP API. The backend is
  not fixed, it is chosen per published artifact: Metal on macOS, and CPU, Vulkan
  or CUDA on Linux and Windows. On macOS Apple Silicon a second embedded engine,
  vllm-mlx, serves models in the MLX format. Local speech-to-text is `whisper`.
- **Persistence.** SQLite with FTS5, in WAL mode. No external database.
- **Transport.** The HTTP API is served on a Unix socket and on TCP with a
  bearer token. `apollia-os start` binds both, taking port 7771 when `--port` is
  omitted. Unix-socket-only is the default of the embedded runtime, not of the
  daemon. Windows has no Unix socket: the local transport there is the named
  pipe `\\.\pipe\apollia-runtime-<user>`, which carries the bearer token
  because a pipe takes a default security descriptor rather than the socket's
  `0600`.
- **No unjustified dependency.** Every third-party dependency, Rust or Python, is
  a sovereignty surface and is added only with an architecture decision behind
  it. The Python SDK is standard-library-only.

## Organizational constraints

- **Documentation is code-derived.** The API, CLI, and SDK references are
  generated from the source of truth and never hand-written. This architecture
  section links to them rather than restating them.
- **Decisions are recorded.** Structural choices are stated in the present
  tense, one section per subject under a stable anchor. See
  [Architecture decisions](/architecture/decisions).
