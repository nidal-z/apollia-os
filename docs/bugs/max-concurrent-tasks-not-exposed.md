# W10 — `max_concurrent_tasks` defaults to 1 and is NOT settable from the SDK `@agent`

- **Severity**: High (feature gap — blocks agent parallelism; forces every integration to build a queue)
- **Components**: `apollia-core` (manifest), SDK `apollia` (`@agent`), runtime (task admission)
- **Version**: `apollia-os 0.1.0` (macOS/Metal)
- **Reported by**: Yumni × Apollia integration (real external usage)

## Symptom
An external app (Yumni) that fires **two classifications at once** gets, on the second request:
```
HTTP 503 — concurrency limit reached for agent 'c8ddc017-…'  (rse-classification-director)
```
The only workaround available to the integrator is a **client-side queue + retry** (which Yumni built).
There is no way to let the agent simply run the two requests in parallel.

## Investigation / root cause
1. Per-agent concurrency is `AgentManifest::max_concurrent_tasks`, **default `1`**:
   - `crates/apollia-core/src/manifest.rs` → `fn default_max_concurrent_tasks() -> u32 { 1 }`
     (field `max_concurrent_tasks: u32`, `#[serde(default = "default_max_concurrent_tasks")]`).
   - Hardcoded `max_concurrent_tasks: 1` at the runtime-only / bundled paths
     (`apollia-cli/src/commands/start.rs`, `apollia-aip/src/{a2a,context}.rs`,
     `apollia-desktop/src/bundled_agents.rs`).
2. The **local runner already supports parallel inference** — so the model layer is NOT the blocker:
   - `crates/apollia-runner/src/backends/llama_cpp.rs` → `const MAX_SLOTS: u32 = 8;` with a `SlotPool`
     of persistent per-model inference slots, each owning its own context + KV cache.
3. But the **Python SDK `@agent` decorator does not expose `max_concurrent_tasks`**. Its parameters are:
   `name, version, description, packages, tags, datasources, templates, secrets, tools_required,
   user_memory_write, memory_namespace, shared_memory_namespaces, step_budget, check_commands,
   agent_type, autonomy_level` — no concurrency field (`sdk/apollia/agent.py`).
   → SDK-authored agents are **pinned at 1** and cannot opt into concurrency from code.

Net: the runner can do up to 8 concurrent inferences, but every SDK agent is capped at 1 concurrent
task with no way to raise it — so agents can't use the parallel slots, and integrators are forced to
serialize/queue on their side.

## Reproduction
1. Author any `@agent` (e.g. an `assistant`) with the SDK and `agent start` it.
2. Submit two tasks to it within the same window (e.g. `POST /api/v1/tasks` ×2 concurrently, or two
   `apollia-os run <agent> …` in parallel).
3. The second → `503 concurrency limit reached`. There is no `@agent(...)` argument to prevent it.

## Expected / fix
Expose `max_concurrent_tasks` where agents are authored/configured:
- SDK: `@agent(..., max_concurrent_tasks: int = 1)` → written into the manifest.
- and/or `agent.toml` manifest field surfaced through `agent install`.
Default may stay 1, but it must be **raisable** so an agent can match the runner/GPU capacity
(`MAX_SLOTS = 8`; in practice tuned to what the hardware sustains — e.g. 2–4 for a 30B on one GPU).

## Acceptance
- `@agent(..., max_concurrent_tasks=3)` → the agent accepts **3** concurrent tasks; the **4th** (not the
  2nd) returns `503`.
- `apollia-os agent info <agent>` shows the effective `Max concurrency`.

## Note
A client-side queue / backpressure is still the correct pattern for bursts **beyond** capacity — this
ticket is only about not being pinned at 1. With the cap raised to real capacity, most concurrent load
runs in parallel and the queue handles only the overflow.
