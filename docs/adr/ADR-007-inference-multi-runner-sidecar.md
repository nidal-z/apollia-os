# ADR-007: Inference runtime, multi-runner sidecar

- Status: Accepted
- Date: 2026-06-04

> **Amended by [ADR-050](ADR-050-embedded-llama-server-replaces-in-tree-runner.md).**
> Passages below describing local LLM inference as an in-tree runner built on
> `llama-cpp-2` record the state at the time of writing. That crate is no longer
> a workspace dependency: local inference runs on the upstream `llama-server`
> binary, supervised by `apollia-runtime`. The sidecar pattern this ADR establishes still holds for speech-to-text.

## Context

Apollia runs local inference for both the LLM and speech-to-text. The
acceleration backend (`local-cuda`, `local-rocm`, `local-vulkan`, `local-metal`,
`local-cpu`) is selected at compile time, and these features are mutually
exclusive because llama.cpp cannot host two GPU backends in one build (GGML
symbol conflicts, double kernel registration, libstdc++ collisions). Linking the
inference engine straight into the main daemon therefore forces one binary per
operating system, architecture, and accelerator combination, which creates three
concrete pains.

First, a user downloading Apollia does not know which variant to take, and
browser-side GPU auto-detection does not cover every case. Second, any incident
on one backend (a CVE, a llama.cpp bug) requires rebuilding and re-signing
several binaries per operating system. Third, there is no way to add a new
backend (Intel oneAPI, a remote runner) without shipping a new binary and forcing
a re-download. A GPU kernel segfault in an in-process engine also takes down the
whole daemon along with memory, A2A, tools, and MCP. The industry (Ollama, LM
Studio, llama.cpp server with workers) has converged on a multi-runner sidecar.

## Decision

We use a multi-runner sidecar architecture. The main daemon does not load the
inference engine itself. At boot it detects the GPU and spawns an
`apollia-runner` child process that holds the `llama-cpp-2` binding (and
`whisper-rs` for STT) compiled with the right backend. The daemon talks to the
runner over loopback HTTP/JSON.

### Architecture

- The daemon (`apollia-os`) keeps the axum REST API, the tray and GUI, the agent
  registry, A2A, memory, tools, and MCP. At boot it detects the GPU, then a
  `RunnerSupervisor` spawns, health-checks, and restarts the child runner, and a
  `RunnerProxy` forwards every LLM and STT call to it.
- The runner (`apollia-runner-{cuda|rocm|vulkan|metal|cpu}`) is a small embedded
  axum HTTP server that loads `llama-cpp-2` and `whisper-rs` with a single
  compiled backend and exposes `/llm/complete`, `/llm/stream`, and
  `/stt/transcribe`. Models are loaded on demand and held in memory (no eviction
  yet).

### Technical choices

- Transport: HTTP/JSON over `127.0.0.1`, debuggable with curl, with negligible
  loopback latency (tens of microseconds per call).
- Serialization: JSON via serde, since the Rust types are already serde-annotated
  and the payloads stay readable in logs.
- Port: the runner auto-binds `127.0.0.1:0` and reports the chosen port to the
  daemon over its stdout, avoiding user port conflicts.
- Lifecycle: one runner spawned at boot after GPU detection, living for the whole
  session, so there is no per-request cold start.
- Crash recovery: the supervisor restarts a dead runner transparently; the
  in-flight task fails with a clear runner-crash error and the user retries.

The Python SDK surface is unchanged: `ctx.llm.complete()`, `ctx.llm.stream()`,
and `ctx.stt.transcribe()` keep the same shape, and the bridge now points at the
`RunnerProxy` instead of an in-process router. CLI commands and the desktop GUI
are unchanged.

## Alternatives considered

### Multi-binary launcher (rejected)
- Pros: one download per operating system, minimal refactor.
- Cons: a large bundle carrying every backend, no crash isolation (a llama.cpp
  segfault takes the whole daemon down), and dead code for the users who only
  have one GPU.

### Status quo, one installer per accelerator (rejected)
- Pros: already shipped, smallest bundle per variant.
- Cons: several SKUs per operating system, no runtime GPU auto-detection, and
  multiplied release and support work when users pick the wrong binary.

### Chosen: multi-runner sidecar
- Pros: one download per operating system with runtime GPU auto-detection, crash
  isolation of GPU kernels from the daemon, an extensible backend surface, and a
  path to remote or shared runners.
- Trade-offs: a larger installer carrying the backends, a small per-call IPC
  overhead, and two processes to monitor instead of one.

## Consequences

- Positive: simpler download UX, robustness against GPU kernel crashes, easy
  addition of new backends, runner-local metrics that do not pollute the daemon,
  and a future path to multi-tenant runners.
- Negative / trade-off: tens of microseconds of IPC per call (negligible against
  inference time but worth watching on many short calls), a larger installer, and
  more operational surface to monitor and to aggregate logs from.
- Watch: loopback behavior behind host firewalls (notably on Windows), runner
  cold-start time at boot, and runner memory footprint with an unload path.

## Architectural principles

- Principle #1 (Local-first): preserved, everything stays local, now two local
  processes instead of one.
- Principle #2 (Zero external dependency): preserved, the runner ships inside the
  installer.
- Principle #4 (Fail fast): improved, the daemon can detect an unhealthy runner
  within seconds and notify the user.
- Principle #5 (One actor, one responsibility): reinforced, the runner does
  inference only and the daemon orchestrates.
- Principle #7 (Non-negotiable safeguards): preserved, the `StepBudget` stays
  enforced in the daemon; the runner only sees individual calls.

## Related

- [ADR-005](ADR-005-oria-execution-model.md) the engine whose calls the
  `RunnerProxy` forwards.
- [ADR-008](ADR-008-llm-backends-model-management.md) the LLM backends and model
  management that run inside the runner.
- [ADR-009](ADR-009-speech-to-text.md) the speech-to-text engine hosted in the
  same runner.
