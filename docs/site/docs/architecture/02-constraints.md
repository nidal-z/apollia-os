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
3. **Minimal contract.** An agent is enough if it duck-types a `manifest()` and
   an async `run()`. The runtime does not impose a framework on the agent.
4. **Fail fast.** Any error detectable at startup is detected at startup, not
   mid-run.
5. **One actor, one responsibility.** The runtime core is a set of Tokio actors
   with no shared mutable state between them. They communicate only by message.
6. **Memory at agent initiative.** The runtime never auto-injects memory context
   into a prompt. An agent recalls when it chooses to.
7. **Non-negotiable safeguards.** A step budget is enforced by the runtime and
   cannot be bypassed by an agent.
8. **Human CLI, machine API.** The CLI is for people (TTY-aware, a global
   `--json`); the API is for programs.

The authoritative statement of the principles lives in the project rulebook
(`AGENTS.md`). This section treats them as fixed inputs.

## Technical constraints

- **Language and runtime.** The core is Rust (1.89+) on Tokio. Errors use
  `thiserror` enums, not `anyhow`, so failures stay typed and map to exit codes
  and structured traces. No `unwrap`, `panic`, or `println` in production paths.
- **Python bridge.** Agents are Python (3.12+), executed through a PyO3 bridge
  with `pyo3-async-runtimes`. The Rust side owns the process; Python is the
  guest.
- **Inference.** Local inference is `llama.cpp` via FFI, on GGUF models, with
  Metal and CUDA backends. Local speech-to-text is `whisper`.
- **Persistence.** SQLite with FTS5, in WAL mode. No external database.
- **Transport.** The HTTP API is served on a Unix socket and, when explicitly
  enabled, on TCP with a bearer token. The embedded default is Unix-socket-only.
- **No unjustified dependency.** Every third-party dependency, Rust or Python, is
  a sovereignty surface and is added only with an architecture decision behind
  it. Agents and workers are standard-library-only by default.

## Organizational constraints

- **Documentation is code-derived.** The API, CLI, and SDK references are
  generated from the source of truth and never hand-written. This architecture
  section links to them rather than restating them.
- **Decisions are recorded.** Structural choices are captured as numbered
  architecture decision records. See [Architecture decisions](/architecture/decisions).
