# ADR-009: Speech-to-text engine

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia transcribes speech to text locally so a user can dictate into any
application through a global hotkey. The speech-to-text engine must honor the
same constraints as local LLM inference: no audio sample leaves the machine
(principle #1), no third-party speech service is installed or launched
(principle #2), a missing model is detected at engine startup rather than at the
first transcription (principle #4), and the audio pipeline (spectrogram, then
sequential decoding) is distinct enough from text generation that it must be a
separate actor (principle #5). The target model is a French-tuned Whisper in GGML
format, kept as an external file, with a sub-two-second latency target on Apple
Silicon. The engine must run wherever local inference runs, alongside the LLM
engine, without symbol conflicts.

## Decision

We use a dedicated `apollia-stt` crate built on an `SttBackend` trait, with a
whisper-rs backend, the engine running inside the `apollia-runner` sidecar
process.

### `SttBackend` trait

An object-safe `Send + Sync` trait is the universal contract for any
speech-to-text engine. Its API is synchronous (the caller wraps it in
`spawn_blocking`); input is `&[f32]` PCM 16 kHz mono and output is a
`TranscriptResult` with segments, timestamps, confidence, and detected language.
Isolating the caller from the engine behind this trait means a future backend
swap is implementing the trait and flipping a feature flag, with no change to the
audio pipeline, the engine actor, or the desktop integration.

### whisper-rs backend in the sidecar

The `SttBackend` trait and its IPC adapter, `RunnerSttBackend`, live in
`apollia-runtime`: `RunnerSttBackend` implements `SttBackend` and speaks HTTP IPC
to the runner. The raw whisper engine lives in `apollia-runner` as
`WhisperBackend`, built on `whisper-rs` (safe bindings over whisper.cpp) and
compiled statically; it is feature-gated, and `apollia-runner` ships no default
features, with the accelerator feature selected at packaging time. The engine
runs inside the `apollia-runner` child process that also hosts the LLM engine
(see ADR-007); the daemon reaches it through the `RunnerProxy`, which exposes the
`/stt/transcribe` endpoint. The LLM and STT engines share the same compiled
backend inside the runner and load together under the same accelerator feature.
The GGML model is an external data file in `~/.apollia/models/`, the same pattern
used for SQLite databases and GGUF models: compiled code in the binary, data as
files on disk.

## Alternatives considered

### candle-whisper, pure Rust (rejected for now)
- Pros: pure Rust, no CMake or C++ at build time.
- Cons: weaker Metal benchmarks and lower maturity than whisper.cpp, and GGML
  quantization not natively supported.

### Cloud speech-to-text service (rejected)
- Pros: zero build complexity, state-of-the-art quality.
- Cons: audio leaves the machine (violates principle #1), requires an API key and
  a connection (violates principle #2), and network latency breaks the latency
  target.

### Chosen: whisper-rs behind the `SttBackend` trait
- Pros: production maturity, static compilation with no runtime dependency,
  native Metal support, and a trait boundary that makes future engine migration a
  feature-flag change.
- Trade-offs: CMake required at build time, a few extra megabytes of compiled
  C++ in the binary, and the GGML model as a separate download.

## Consequences

- Positive: a fully local pipeline from hotkey to capture to transcription to
  clipboard under the latency target, a clean migration path behind the trait,
  and an STT engine that coexists with the LLM engine in the runner without
  symbol conflicts.
- Negative / trade-off: a build-time CMake dependency and an initial model
  download of meaningful size.
- Watch: microphone permission handling on macOS, and the comparative Metal
  benchmarks that would justify a future backend swap.

## Architectural principles

- Principle #1 (Local-first): inference is fully local, no audio sample leaves
  the machine.
- Principle #2 (Zero external dependency): whisper.cpp is compiled statically into
  the runner; the GGML model is an external data file; CMake is build-time only.
- Principle #4 (Fail fast): a missing or corrupt GGML model raises a load error at
  engine startup, before the first transcription.
- Principle #5 (One actor, one responsibility): `apollia-stt` is its own crate and
  the STT engine is a distinct actor, separate from the LLM engine.
- Principle #7 (Non-negotiable safeguards): a maximum recording duration prevents
  an unbounded capture.
- Principle #8 (Human CLI, machine API): the STT CLI commands support `--json`.

## Related

- [ADR-007](ADR-007-inference-multi-runner-sidecar.md) the sidecar that hosts the
  whisper-rs engine.
- [ADR-008](ADR-008-llm-backends-model-management.md) the LLM backends that share
  the same runner and the same external-model pattern.
