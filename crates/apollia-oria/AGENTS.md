# crates/apollia-oria/AGENTS.md

> Local rules for the ORIA engine (Observer-Reasoner-Actor). Read after
> `docs/agents/INDEX.md` and before editing this crate. Pair with
> `docs/agents/ARCHITECTURE.md` §C (StepBudget) and ADR-025.

ORIA is the reasoning loop. It owns the StepBudget guard, the
ResilienceLayer (retry policies, circuit breakers), and the plan cache.
The crate's job is to run an agent step-by-step under hard runtime
authority.

---

## 1. The ORIA loop

```
Observer  -> reads runtime state (memory, context, tools)
Reasoner  -> calls the LLM, produces a plan or a tool invocation
Actor     -> executes the tool, captures the result
Repeat    -> until done, until budget exceeded, until cancelled
```

Source : `src/engine.rs`. The loop is a Tokio actor following the
canonical pattern (see `docs/agents/RUST-PATTERNS.md` §2).

---

## 2. StepBudget

`StepBudgetConfig` from `apollia-core` :

```rust
pub struct StepBudgetConfig {
    pub max_steps: u32,
    pub max_duration: Duration,
    pub max_tokens_in: u64,
    pub max_tokens_out: u64,
    pub max_tool_invocations: u32,
}
```

Enforced by ORIA at every step. **Non-bypassable** from Python or from a
tool. The agent never sees the budget; it just gets cut off when the
budget is reached.

Rules :
- Every step increments the budget counters via `Budget::tick(...)`.
- Crossing the budget terminates the task with
  `EventBus::publish(RuntimeEvent::TaskBudgetExceeded { ... })` and a
  final `AIPResult` carrying the partial output.
- The budget is set at task spawn time. It cannot be mutated mid-task.
- A budget of zero in any dimension is rejected at task spawn
  (`OriaError::InvalidBudget`).

If you find yourself wanting to "just bypass the budget for this one
case", the answer is no. Open an ADR.

---

## 3. ResilienceLayer

Retry policies and circuit breakers wrap LLM calls and tool invocations.

```rust
pub enum RetryPolicy {
    None,
    FixedBackoff { delay: Duration, max_attempts: u32 },
    Exponential { initial: Duration, multiplier: f64, max_attempts: u32 },
}

pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: u32,
}
```

Rules :
- Default policy : `Exponential { 100ms, 2.0, 3 }` for LLM calls,
  `FixedBackoff { 500ms, 2 }` for tool invocations.
- Per-backend overrides via `[llm.routing]` or `[tool.<name>]` in the
  agent TOML.
- Circuit breakers are per (backend, agent). Tripping the breaker emits
  `RuntimeEvent::BackendCircuitOpen { backend, reason }`.
- Retry is **not** transparent to observability : each attempt logs at
  `WARN` with `attempt = N`.

---

## 4. Plan cache

ORIA caches the LLM-generated plan when the agent declares
`@orchestrated(cache_plan = true)`. Cache key : (agent_id, skill_id,
hash(payload)).

Rules :
- Cache TTL : default 5 minutes, configurable in agent TOML.
- Cache is in-memory per process. Not persisted across runtime restarts.
- Cache invalidation : explicit via `ctx.cache.invalidate(...)`. Never
  silent.
- Cache misses log at `DEBUG`. Cache hits log at `TRACE`.

---

## 5. HITL (human in the loop)

`@orchestrated` skills can pause for human input via the
`NeedHumanInput` exception from the SDK. ORIA captures the exception,
emits `RuntimeEvent::HumanInputRequested`, and parks the task.

Rules :
- Parked tasks have a TTL (default 24h). On expiry, they emit
  `TaskAbandoned` and the task transitions to `failed`.
- Resumption : the operator answers via the desktop UI or via
  `apollia task resume <id> --input @answer.json`.
- The agent's `Ctx.input.next()` returns the answer, the loop continues.
- Budget continues to count during parked state for `max_duration`,
  pauses for `max_steps` / `max_tokens`.

---

## 5b. Plan gate (plan-then-approve)

The plan gate is a pause point in `execute_orchestrated_plan`, between
`PlanGenerated` and `StepBudget` creation. When active it registers a
oneshot in `PendingPlanGates`, emits `PlanApprovalRequired`, and awaits an
approve/reject decision before starting the `ActorLoop`. No budget is
created while waiting (the gate cannot consume budget).

Scope and boundaries :
- The gate lives on the orchestrated engine path only. The chat ReAct loop
  (`apollia-runtime` `BuiltInChatAgent`) does not produce a discrete plan, so
  it has no plan gate; its `run_id` is a correlation id, not a gate key. The
  two are deliberately not unified.
- Activation : `plan_gate_active()` returns the per-run override
  (`with_plan_gate_override`) when set, else the autonomy tier's `gate_policy`
  (Assisted/Supervised gate, Bounded/Long bypass; default tier is Assisted).
- The per-task engine only receives a `PendingPlanGates` registry when the
  gate is explicitly requested (CLI `--plan` -> `run_options.plan_gate ==
  Some(true)`), so headless submissions (A2A, triggers) never block. Without a
  registry the gate resolves to `Approved` immediately.
- On reject the engine replans with feedback (`Reasoner::plan_with_feedback`),
  bounded by `plan_gate_max_replans`.

Known follow-up : the SDK `@agent(autonomy_level=...)` value is carried in the
manifest JSON but is not yet read by the Rust `AgentManifest` (no field).
Consuming the manifest-declared tier requires adding
`autonomy_level: Option<AutonomyLevel>` to `AgentManifest` (a wide change:
every full struct literal must be updated). Until then, the tier reaches the
engine only via the per-run `run_options.autonomy_level` (CLI `--autonomy`).

---

## 6. Pipelines (ADR-025)

Pipelines are declarative orchestration over multiple agents, expressed
in TOML. ORIA executes them via the `pipeline::Runner` :

```toml
[pipeline.email-triage]
director = "triage-agent"
workers = ["classifier", "summarizer"]
mode = "fan-out"
budget = { max_steps = 10, max_duration_secs = 60 }
```

Rules :
- The director is always an ORIA agent. Workers may be ORIA agents or
  pure A2A workers (see `agents/`).
- Mode : `fan-out` (parallel), `sequential`, `conditional`. New modes
  require an ADR.
- HITL gates can be inserted between stages via
  `[pipeline.X.stages.Y].requires_approval = true`.

---

## 7. Forbidden in this crate

- Bypassing the budget guard (no `#[allow(...)]`, no env-var override).
- Holding the GIL across a `tokio::select!` (the GIL belongs in
  `apollia-aip`).
- Side effects in Observer (it reads, period).
- Mutable global state. The engine is one actor per task.
- Direct calls to `apollia-llm` backends. Route through `ResilienceLayer`.

---

## 8. Testing

- Property-based tests on the budget guard (`proptest`) :
  arbitrary step counts, arbitrary durations, the guard never lets the
  task exceed the configured limit.
- Integration tests in `tests/` exercise the full loop with a fixture
  LLM backend that returns deterministic plans.
- Plan cache : tests verify TTL expiry, invalidation, hit/miss counters.

---

## 9. When the rules block you

- Need a new resilience pattern (exponential jitter, hedge requests) :
  add it as a `RetryPolicy` variant, default `None`, opt-in per backend.
- Need a new pipeline mode : ADR first.
- Need to inspect the budget mid-task for diagnostics : add a
  `BudgetSnapshot` event, never expose mutable access.
