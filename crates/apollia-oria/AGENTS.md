# crates/apollia-oria/AGENTS.md

> Local rules for the ORIA engine (Observer-Reasoner-Actor). Read after
> the root `AGENTS.md` and before editing this crate. Pair with
> `docs/agents/ARCHITECTURE.md` §C (StepBudget).

ORIA is the reasoning loop. It owns the StepBudget guard, the
ResilienceLayer (retry policy, circuit breakers), the plan cache and the
plan gate. The crate's job is to run an agent step-by-step under hard
runtime authority.

Every symbol named below is a symbol the crate carries today. The
sections this file used to hold described a budget with five dimensions,
a `Budget::tick`, an `OriaError::InvalidBudget`, a `CircuitBreakerConfig`,
a `RuntimeEvent::TaskBudgetExceeded`, a `RuntimeEvent::BackendCircuitOpen`,
a `RuntimeEvent::HumanInputRequested`, a `TaskAbandoned`, a
`ctx.cache.invalidate`, an `@orchestrated(cache_plan = true)` and a TOML
`[pipeline.X]` runner. `git grep -c` answered 0 for each of them.

---

## 1. The ORIA loop

```
Observer  -> reads runtime state (memory, context, tools)
Reasoner  -> calls the LLM, produces a plan or a tool invocation
Actor     -> executes the tool, captures the result
Repeat    -> until done, until budget exhausted, until cancelled
```

Sources: `src/observer.rs`, `src/reasoner.rs`, `src/actor.rs`, driven by
`src/engine.rs` (with `src/engine/` for the per-mode paths: `direct.rs`,
`orchestrated.rs`, `plan_cache_ops.rs`, `builder.rs`).

---

## 2. StepBudget

`StepBudgetConfig` from `apollia-core` (`crates/apollia-core/src/budget.rs`)
carries three dimensions, not five:

```rust
pub struct StepBudgetConfig {
    pub max_steps: u32,        // default 30
    pub max_tool_calls: u32,   // default 60
    pub wall_clock_secs: u64,  // default 600
}
```

`StepBudget` (`src/budget.rs`) is the runtime side. It is built with
`StepBudget::new(&config)`, or with `StepBudget::from_capped(agent, runtime)`
which takes the minimum of the agent's declared budget and the runtime's, so
an agent cannot raise its own ceiling.

Rules :
- Counters advance through `increment_steps()` and `increment_tool_calls()`.
  There is no `tick`.
- Exhaustion is read with `is_exhausted()` and named with
  `exhaustion_reason()`. `wait_for_exhaustion()` is the awaitable form, over a
  `watch` channel so it is re-armable after a HITL resume.
- Crossing the budget ends the run through `ORIAError::BudgetExceeded { reason }`
  (`src/engine.rs`), or, on the orchestrated path, through
  `fail_plan_budget_exhausted` which returns a final `AIPResult` carrying the
  partial output (`src/actor/persist.rs`).
- The budget is set at task spawn time and is not mutated mid-task.
- The agent never sees the budget as a control surface; it reads the remaining
  allowance through `ctx.budget` (`StepBudget::to_live_budget_view`).

If you find yourself wanting to "just bypass the budget for this one
case", the answer is no. The rule in force is stated in
`docs/site/docs/architecture/08-decisions.md` under `#budget-and-safeguards`,
and any change to it starts by rewriting that section.

---

## 3. ResilienceLayer

Circuit breakers wrap tool invocations; a retry policy computes the backoff.
Source: `src/resilience.rs`.

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,   // default 3
    pub base_delay_ms: u64,  // default 500
    pub max_delay_ms: u64,   // default 10_000
    pub jitter: bool,        // default true
}
```

`ResilienceLayer::new(default_failure_threshold, default_cooldown)` is the
constructor; the engine builder passes `(3, Duration::from_secs(30))`
(`src/engine/builder.rs`). There is no `CircuitBreakerConfig` struct and no
`RetryPolicy` enum with `None` / `FixedBackoff` / `Exponential` variants: the
backoff is always exponential, `min(base_delay_ms * 2^(attempt-1), max_delay_ms)`,
with an optional jitter of plus or minus 25 percent.

Rules :
- Breakers are per tool name, registered with `register_tool` and consulted by
  `pre_check` before every invocation; `record_success` and `record_failure`
  move the `CircuitState`.
- The operator reads and resets them through `apollia-os resilience list`,
  `resilience show <tool>` and `resilience reset <tool> --confirm`, served by
  `snapshot()` and `reset_breaker()`.
- Retry is not transparent to observability: attempts are logged, and
  `crates/apollia-runtime/src/observability/resilience_subscriber.rs` persists
  the breaker state changes.

---

## 4. Plan cache

`PlanCacheRepository` (`src/plan_cache.rs`) is a SQLite store, not an in-memory
map: it opens `~/.apollia/plan_cache.db` through
`apollia_core::schema::open_versioned` and survives runtime restarts. The engine
holds it as `Option<Arc<Mutex<PlanCacheRepository>>>`, injected with
`ORIAEngine::with_shared_plan_cache`; without the injection every lookup and
store is a no-op.

Cache key : `compute_cache_key(agent_name, agent_version, tools, task_text)`,
a SHA-256 over the agent name, the agent version, the alphabetically sorted
tool names and the task text lowercased, whitespace-collapsed and truncated to
500 characters. There is no per-skill opt-in decorator.

Rules :
- Lookups and stores are best-effort. A poisoned lock or a SQLite error is
  logged (`plan.cache.lock.poisoned`, `plan.cache.lookup.failed`,
  `plan.cache.store.failed`) and treated as a miss, never as a failure of the
  run.
- A cache hit reuses the cached steps under a fresh `plan_id`.
- Eviction is an operator act, not a TTL: `apollia-os plan cache evict
  --max-age-days N` (default 7) calls `evict_expired`, `plan cache clear
  --confirm` calls `clear_all`, and `plan cache stats` reads `stats()`.

---

## 5. HITL (human in the loop)

Two distinct pauses live in this crate, and they are not the same mechanism.

**Step approval.** Before a step whose tool needs it, `suspend_for_approval`
(`src/actor/steps.rs`) registers a oneshot in `PendingApprovals` under
`<task_id>::<step_id>`, emits `RuntimeEvent::TaskInputRequired { task_id,
prompt, step_id: Some(..) }` and awaits the decision. Approval lets the step
run; refusal returns `StepError::RejectedByUser { reason }`; a closed channel
returns `StepError::ApprovalChannelClosed`. The wait is a pure `await`, so no
counter advances during it.

**Agent-raised input.** A skill that raises the SDK's `NeedHumanInput` returns
an `AIPResult` with `TaskStatus::InputRequired`, carrying `prompt` and
`context`; the same `TaskInputRequired` event is emitted with `step_id: None`.

Rules :
- Resumption is `apollia-os task resume <task_id> --approve` or `--reject
  [--reason ...]`, or the equivalent surface in the desktop UI. There is no
  `--input @answer.json` flag and no `ctx.input.next()`.
- Suspended tasks do not expire by default. `TimeoutWatcher`
  (`crates/apollia-runtime/src/timeout_watcher.rs`) cancels them only when
  `[hitl] timeout_hours` is set in `apollia.toml`; its scan interval is 60 s.

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
- Timeout and channel loss are typed: `ORIAError::PlanGateTimeout { run_id,
  plan_id, ttl_secs }` and `ORIAError::PlanGateChannelClosed { run_id }`.

Known follow-up : the SDK `@agent(autonomy_level=...)` value is carried in the
manifest JSON but is not yet read by the Rust `AgentManifest` (no field).
Consuming the manifest-declared tier requires adding
`autonomy_level: Option<AutonomyLevel>` to `AgentManifest` (a wide change:
every full struct literal must be updated). Until then, the tier reaches the
engine only via the per-run `run_options.autonomy_level` (CLI `--autonomy`).

---

## 6. Errors

`ORIAError` (`src/engine.rs`) is the crate's outward error. It is
`#[non_exhaustive]` and carries, among others, `BudgetExceeded`,
`ExecutionFailed`, `ObserverError`, `BridgeError`, `NoLlmConfigured`,
`PlanFailed`, `ApprovalChannelClosed`, `PlanGateTimeout` and
`PlanGateChannelClosed`. The per-component errors are `StepError`
(`src/actor.rs`), `ObserverError`, `ReasonerError` and `PlanValidationError`
(`src/reasoner.rs`), `ResilienceError`, `PlanCacheError`,
`PlanRepositoryError`, `ArgResolveError` and `OffloadError`.

There is no `OriaError` under that spelling. Match on the variant, never on
the formatted message.

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

- The budget guard is covered by unit tests in `src/budget.rs`, including
  `exhaustion_reason` per dimension and the re-armable
  `wait_for_exhaustion`, plus a `proptest` block over arbitrary step and
  tool-call counts and a Kani proof over the whole `u32` domain.
- Integration tests in `tests/` exercise the loop with a fixture backend.
- Plan cache : `src/plan_cache.rs` covers store, lookup, hit counters,
  eviction by age and `clear_all`; `src/engine.rs` covers the shared
  repository being reachable from the engine.
- The plan gate and the mailbox lease have abstract Loom models in
  `crates/apollia-loom-models`, not in this crate.
- GIVEN / WHEN / THEN, as everywhere.

---

## 9. When the rules block you

- Need a different backoff shape (hedged requests, per-tool overrides) : it
  goes on `RetryPolicy`, defaulted so existing call sites do not change.
- Need multi-agent orchestration : this crate has no pipeline runner. State
  the execution model in `docs/site/docs/architecture/08-decisions.md` under
  `#execution-model` before writing one.
- Need to inspect the budget mid-task for diagnostics : read
  `to_live_budget_view()`, never expose mutable access.
