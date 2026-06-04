# ADR-008: LLM backends, model management and transparency

- Status: Accepted
- Date: 2026-06-04

## Context

An agent calls `ctx.llm.chat()` without requiring any external service, yet some
users prefer to delegate inference to a cloud provider. Both cases must be
covered without forcing those who only use the cloud to compile the local engine.
A single statically configured backend is not enough: different agents benefit
from different models (a code agent on a coder model, a messaging agent on a
model tuned for French prose), and the backend configuration must be editable
from the desktop app like every other runtime entity rather than locked in a
text file. Large open-weight models ship sharded across several GGUF files, so
loading must handle multi-file models. Users also need a way to discover and
download models without hand-maintaining a static registry, cloud sessions repeat
large prompts and should not pay full price each time, and the frontend wants a
horizontal transparency layer that narrates the agent without a second model to
configure.

## Decision

We adopt cloud backends behind a `cloud` feature flag with the local engine in
the runner sidecar, a SQLite-backed multi-backend registry with per-agent binding
and DB-first TOML sync, single-file GGUF loading, the HuggingFace Model Hub,
prompt caching for cloud backends, and a shared transparency orchestrator that
reuses the user-configured router.

### Backends behind a feature flag

The `apollia-llm` crate has a single `cloud` Cargo feature (default), which
compiles the HTTP clients for three providers: Anthropic (`anthropic.rs`),
OpenAI-compatible providers (`openai.rs`), and Google Vertex (`vertex.rs`). The
Vertex client uses a hand-rolled Application Default Credentials flow, exchanging
the ADC token against `oauth2.googleapis.com` over `reqwest` rather than a
third-party auth crate. The local backend does not live in `apollia-llm`: it and
the `local-*` accelerator features live entirely in `apollia-runner`, which holds
the embedded `llama-cpp-2` engine. The local inference engine runs inside the
`apollia-runner` sidecar process, not in the main daemon (see ADR-007); the
daemon reaches it through the `RunnerProxy`. A `.gguf` model is never embedded in
the binary: it lives in `~/.apollia/models/` as an external data file. If a cloud
API key is missing, that backend is skipped with a warning; if no backend is
available, `ctx.llm` is `None` and the agent runs degraded.

### Multi-backend registry with per-agent binding

Backends live in a `llm_backends` table in `~/.apollia/system.db`, with exactly
one `is_default`. `AgentManifest.llm_backend` is an optional string; absent or
`None` means the default backend. The `LlmRouter` holds a map from backend name
to backend and routes per agent manifest. A REST surface
(`/api/v1/llm/backends`) exposes full CRUD plus set-default to the desktop app
and the CLI. Provider secrets use `${VAR}` interpolation or the keyring; an API
key never sits in clear text in the DB. If a manifest names an unknown backend,
the router falls back to the default with a warning rather than failing.

### DB-first with atomic TOML sync

The DB is the source of truth. After each mutating LLM REST handler,
`sync_to_toml()` rewrites the `[[llm.backends]]` blocks in `apollia.toml` from the
DB (line-based, preserving other sections and their comments) as a best-effort
step: a sync failure logs a warning but does not fail the request, since the DB
stays consistent and the TOML is re-synced on the next mutation. A reload command
performs an atomic router swap, dropping the old router so a replaced GGUF model
is freed from memory immediately, while in-flight requests holding a clone keep
the old model loaded until they finish.

### GGUF loading

Standard single-file GGUF loading is implemented: the runner loads a single
`.gguf` via `LlamaModel::load_from_file`. Multi-file shard support is recognized
in the CLI model listing, but shard-load validation is not wired: the
custom-split FFI path is not connected, and the `ModelShardMissing` and
`ModelShardNotFirst` error variants are defined but not yet used. Sharded
multi-file models are therefore a recognized future surface, not an implemented
two-mode load contract.

### HuggingFace Model Hub

Model metadata (file list, sizes, generation parameters, tags, license) is read
live from the public HuggingFace API; there is no static registry to maintain. An
in-memory session cache (an `RwLock<HashMap>` of model types) holds resolved
entries with a 24-hour TTL, rather than a `system.db` table. Most popular models
need no token; an optional HuggingFace token is supplied per request (passed to
`HfRegistryClient::new`) and sent as a bearer only when present, with a short
wizard for gated models. GGUF files download directly from HuggingFace as a CDN:
Apollia redistributes no model and HuggingFace handles licensing. Hardware
detection sizes a memory budget and tags each file as fits, might fit, or too
large.

### Prompt caching for cloud backends

The Anthropic client applies up to three ephemeral cache breakpoints (the system
prompt, the tool definitions, and a sliding breakpoint on the third message from
the end) and always sends the prompt-caching beta header. `TokenUsage` carries
`cache_read_input_tokens` and `cache_write_input_tokens`, both zero for backends
without an equivalent mechanism, so the change is backward compatible. On long
sessions repeating the same context this cuts input-token cost substantially.

### Transparency orchestrator

The `MetaLlmOrchestrator` service in `apollia-llm` generates transparency
artifacts (tool-call rationales, thinking summaries, session titles, error
explanations, risk assessments, alternative branches, and more) by reusing the
user-configured `LlmRouter` rather than a dedicated second model. It is a Tokio
actor with an LRU cache (keyed on routine plus canonical-JSON SHA-256), a
per-session token budget that emits a budget-exceeded event, a 10-second timeout
with a static-text fallback, and a strict opt-in toggle. Reusing the main router
avoids a second backend, a second key, and a second quota, and keeps the
narration consistent in tone.

## Alternatives considered

### External inference daemon managed by the supervisor (rejected)
- Pros: no in-tree engine to maintain.
- Cons: assumes a third-party daemon is installed, which violates principle #2,
  and complicates lifecycle management.

### Per-agent environment variables for backend selection (rejected)
- Pros: no runtime change.
- Cons: makes the agent responsible for infrastructure config, cannot be managed
  from the desktop, and offers no central routing.

### Proprietary shard manifest file (rejected)
- Pros: one file unifying both load modes.
- Cons: diverges from the llama.cpp ecosystem and duplicates the shard list,
  risking silent desync.

### Static embedded model registry (rejected)
- Pros: works offline at first.
- Cons: becomes stale the moment a new popular model ships, a permanent
  maintenance burden.

### Dedicated second model for transparency (rejected)
- Pros: isolates narration cost.
- Cons: a second backend and quota to configure, stylistic inconsistency, and
  double billing for the same interaction.

### Chosen: feature-flagged cloud backends, SQLite registry, single-file GGUF, Model Hub, prompt caching, shared orchestrator
- Pros: cloud and local both first class, per-agent routing editable live,
  single-file GGUF models loadable, zero-maintenance model discovery, lower cloud
  cost, and a transparency layer with no extra configuration.
- Trade-offs: a multi-backend router to keep correct, network dependence for the
  Model Hub, and a shared main-model budget between useful work and narration.

## Consequences

- Positive: offline inference with a valid GGUF, several agents on different
  models simultaneously, live editing from the desktop, immediate RAM release on
  model swap, single-file GGUF support, and substantial cloud-token savings on
  long sessions.
- Negative / trade-off: a refactored router carries some regression risk, the
  Model Hub needs the network, and transparency narration draws on the main model
  budget.
- Watch: router behavior when a manifest names a missing backend, the Model Hub
  cache hit rate, and the transparency budget per session.

## Architectural principles

- Principle #1 (Local-first): the local backend is fully offline, API keys stay
  local, and model downloads are explicit.
- Principle #2 (Zero external dependency): static linking inside the runner, no
  third-party daemon, no proprietary shard format.
- Principle #3 (Minimal contract): `llm_backend` is optional in the manifest.
- Principle #4 (Fail fast): a missing or unreadable model file raises a typed
  error at load time.
- Principle #7 (Non-negotiable safeguards): the ReAct loop consults the
  `StepBudget` on every iteration.

## Related

- [ADR-007](ADR-007-inference-multi-runner-sidecar.md) the sidecar that hosts the
  local engine these backends drive.
- [ADR-005](ADR-005-oria-execution-model.md) the engine that issues the LLM calls
  routed here.
