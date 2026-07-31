# ADR-050: Embedded llama-server replaces the in-tree llama-cpp-2 runner

- Status: Accepted
- Date: 2026-07-31

## Context

Local inference was carried by an in-tree runner built on the `llama-cpp-2`
crate, a Rust binding over llama.cpp, running as a sidecar process
([ADR-007](ADR-007-inference-multi-runner-sidecar.md)). That arrangement asked
the project to track llama.cpp through a third-party binding, which turned every
upstream capability into a two-step wait: llama.cpp ships it, the binding exposes
it, Apollia can use it.

Three things forced the question. The binding had to be pinned at `=0.1.146`
because a later release removed the tool-calling API the runtime depends on, so
the project was frozen on an old llama.cpp anyway. Tool calling through a local
model needs the chat template embedded in the GGUF, which upstream exposes with
`--jinja` and the binding did not surface. And the features that decide local
throughput, continuous batching, flash attention, KV cache quantization, prefix
reuse, are upstream server flags rather than binding APIs.

Meanwhile upstream publishes a `llama-server` binary per platform, with an
OpenAI-compatible HTTP surface Apollia already speaks for cloud backends.

The migration was carried out over several commits without an ADR. This record
exists so the decision is written down where the corpus looks for it, and so the
four ADRs that still describe the old arrangement can point somewhere.

## Decision

We adopt the upstream `llama-server` binary as the local inference engine, driven
over its OpenAI-compatible HTTP surface, and we remove `llama-cpp-2` from the
workspace.

- `crates/apollia-runtime/src/llama_server/` supervises the process: it picks a
  free port, launches the binary with `--jinja`, waits for `/health`, and
  respawns on death.
- The binary is pinned by checksum and staged into packaged builds
  (`packaging/fetch-llama-server.sh`, `packaging/llama-server-checksums.txt`). A
  source build resolves it from `PATH`.
- Engine parameters are settable per launch through `APOLLIA_LLAMA_*`
  environment variables, see the environment variable reference.
- `crates/apollia-runner` stays, narrowed to a single job: the speech-to-text
  sidecar over `whisper-rs`. Its `backends/` directory holds `whisper.rs` and
  nothing else.

## Alternatives considered

### Keep `llama-cpp-2` and wait for the binding (rejected)
- Pros: no process to supervise, no HTTP hop, one language in the stack.
- Cons: pinned to `=0.1.146` because a later version removed the tool-calling
  API. No `--jinja`, so tool calling on a local model stays unreliable. Every
  upstream capability arrives late or not at all.

### Require the operator to run their own llama-server (rejected)
- Pros: nothing to bundle, nothing to supervise.
- Cons: breaks principle 2, zero external dependency. A desktop user should not
  have to install and run a server before the product works.

### Chosen: embed and supervise the upstream binary
- Pros: upstream capabilities the day they ship, `--jinja` and therefore
  workable local tool calling, throughput knobs exposed as flags, one HTTP
  contract shared with cloud backends.
- Trade-offs: a child process to supervise and to package per platform, a
  checksum to keep current, and an HTTP hop on the local path.

## Consequences

- Positive: local tool calling works, because the chat template embedded in the
  GGUF is honoured. This was the blocking defect of the previous arrangement.
- Positive: the local and cloud paths share one request contract, so the router
  treats them alike.
- Positive: the workspace no longer compiles llama.cpp, which shortens a clean
  build considerably.
- Negative: the runtime now owns a child process lifecycle, with the failure
  modes that come with it (port already taken, binary absent from `PATH` on a
  source build, death under memory pressure).
- Negative: packaging gained a per-platform binary to fetch and verify.
- Watch: the pinned upstream version. A `llama-server` older or newer than the
  pin may accept different flags. `APOLLIA_LLAMA_EXTRA_ARGS` is the escape hatch,
  and it is unvalidated by design.

## Architectural principles

- Principle #2 (zero external dependency): upheld. The binary is staged in
  packaged builds, so a user installs nothing. A source build is the one case
  that expects `llama-server` on `PATH`, and says so.
- Principle #1 (local-first): unchanged. The engine binds to `127.0.0.1` and no
  inference leaves the machine.
- Principle #4 (fail fast): the supervisor waits for `/health` before declaring
  the engine ready, so a broken launch surfaces at startup rather than on the
  first call.

## Related

- [ADR-007](ADR-007-inference-multi-runner-sidecar.md) established the multi
  runner sidecar this supersedes for the LLM path. Its speech-to-text half stands.
- [ADR-008](ADR-008-llm-backends-model-management.md) describes backend and model
  management; the local backend it names is now served by this engine.
- [ADR-009](ADR-009-speech-to-text.md) chose `whisper-rs` for speech-to-text.
  That choice is unaffected: only the cohabitation with an LLM runner is.
- [ADR-024](ADR-024-sdk-runtime-contract-ctx.md) describes the `ctx.llm` contract,
  unchanged by this decision.
