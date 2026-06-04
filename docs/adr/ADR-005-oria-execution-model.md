# ADR-005: ORIA execution model

- Status: Accepted
- Date: 2026-06-04

## Context

ORIA is the engine that drives an autonomous agent through a task. Two facts
shape its design. First, roughly 80% of real tasks are atomic: a single action
with a direct result ("list the files in this directory"). Paying for an extra
LLM planning call on those tasks is unacceptable latency and cost. Second, a
genuine multi-step task ("produce a complete quote in five stages") needs an
explicit plan, a per-step execution loop, and runtime guardrails that the agent
cannot bypass.

The Reasoner-Planner approach is optimal for complex tasks but adds overhead on
atomic ones. A single execution path cannot serve both well. Separately, in
multi-step execution the engine must decide who runs the tools, how a step sees
the outputs of prior steps, how to avoid regenerating identical plans, and how
to keep a long session under the model context window. All of these must hold
the non-negotiable safeguard from principle #7: the `StepBudget` enforced by the
runtime and never bypassable.

## Decision

We adopt a bimodal execution model with automatic classification, an
Option B orchestrated mode where the runtime executes tools, per-step
observation, a SQLite plan cache, and context-window compaction.

### Automatic mode classification

Each incoming task is classified by the runtime, not by the agent. The agent
exposes the same `run()` interface in both modes.

- Direct mode: atomic task. The runtime calls `agent.run()` directly under
  `StepBudget` supervision. Criteria: few required tools, low estimated step
  count.
- Orchestrated mode: complex multi-step task. The `Reasoner` generates an
  `ExecutionPlan`, the `ActorLoop` executes it step by step, with a
  configurable number of replans on failure (default two).

### Orchestrated mode: the runtime executes tools (Option B)

In orchestrated mode the `ActorLoop` runs the tools directly. `agent.run()` is
never called during step execution. The agent becomes declarative: its
`manifest()` and `system_prompt` describe what it wants, ORIA decides how to
run it. This keeps `ResilienceLayer`, `StepBudget`, per-step SQLite persistence,
and replanning entirely under runtime control, and lets the `ActorLoop` be unit
tested in pure Rust with a mock completion model and a mock tool proxy.

An optional `on_plan_complete(step_results, ctx)` hook, detected by Python duck
typing, is the only re-entry point into the agent after the plan finishes. If
present, ORIA calls it with all step outputs (`dict[str, str]`). If absent, ORIA
concatenates the outputs and returns a completed result. The minimal contract
stays `manifest()` plus `run()`; `on_plan_complete()` is a third, always
optional, hook. In orchestrated mode `system_prompt` is required and its absence
fails fast before any Reasoner call.

### Per-step observation

After each step the runtime injects the prior step outputs into the next step
context (`StepContext`) and auto-records a lightweight episodic memory entry.
This adds no extra LLM call: it is a map lookup plus a fire-and-forget memory
write. Without it each step would run blind to earlier results, degrading
multi-step plan quality. Memory at agent initiative (principle #6) is relaxed
only here, because in orchestrated mode the runtime drives execution; in Direct
mode the agent keeps full control of its memory.

### Plan cache

Orchestrated plans are cached in SQLite, keyed by a SHA-256 of
`{agent_name}:{agent_version}:{sorted_tool_names}:{normalized_task_text}`, with a
7-day default TTL. Expired entries are evicted by TTL (by `created_at`). The
cache is checked before each `Reasoner::plan()`. A hit emits
`RuntimeEvent::PlanCacheHit` and reuses the plan under a fresh `plan_id`. The
`agent_version` component plus the TTL handle invalidation when an agent changes
its tools or version.

### Context-window compaction

The `ContextManager`, which lives in `apollia-oria`, exposes `maybe_compact()`,
called inside the `ActorLoop` before each per-step LLM completion and in the
chat builtin agent. When the estimated token count of the current
history crosses 80% of the active model context window (configurable in
`apollia.toml`, conservative by design to leave room for the response), the
manager summarizes the history into a single message and replaces the running
messages with `[original_system_message, summary_message]`. The original system
prompt is always preserved as `messages[0]`. A `RuntimeEvent::ContextCompacted`
event is emitted. If the summary call fails, a placeholder is used and the
session continues rather than failing fatally.

## Alternatives considered

### Single orchestrated mode (rejected)
- Pros: one code path, simplest to maintain.
- Cons: an LLM planning call on every task, including the simplest, prohibitive
  cost and latency for the common atomic case.

### Single direct mode (rejected)
- Pros: maximum simplicity, no planning LLM.
- Cons: impossible to handle genuinely multi-step tasks well; the agent would
  have to manage all orchestration itself.

### Agent chooses its own mode (rejected)
- Pros: the agent knows its own task complexity.
- Cons: too much configuration for the target user, non-deterministic behavior,
  and a violation of the minimal contract (principle #3).

### Orchestrated step delegated to `agent.run()` per step (rejected)
- Pros: the agent keeps fine control of each step.
- Cons: the agent must carry inter-step state itself, `run()` is called N times
  with unpredictable side effects, and the runtime guardrails (StepBudget,
  ResilienceLayer, per-step audit) are pushed out of runtime control.

### Keep the last N messages on overflow (rejected)
- Pros: trivial, no extra LLM call.
- Cons: loses the original task constraints, creates an inconsistent history
  referencing dropped messages, and is hard to debug. An LLM summary preserves
  semantic continuity.

### In-memory plan cache (rejected)
- Pros: fastest possible lookup.
- Cons: lost on restart, unbounded growth on long-running daemons. SQLite
  survives restart and is bounded.

### Chosen: bimodal classification with runtime-executed orchestrated mode
- Pros: minimal latency on atomic tasks, native support for multi-step tasks,
  declarative agents, all guardrails enforced by the runtime, fully testable in
  Rust, cached plans, bounded context.
- Trade-offs: two code paths to test, classification can mis-judge ambiguous
  tasks, plan quality depends on the Reasoner, and compaction summaries may drop
  subtle detail.

## Consequences

- Positive: the common atomic case is fast and cheap, complex tasks are
  supported natively, the orchestrated loop is testable without Python, repeated
  tasks skip planning, and long sessions no longer fail on context overflow.
- Negative / trade-off: two execution paths, a classification heuristic that can
  err, and an approximate token estimate that may compact slightly early on very
  dense content.
- Watch: classification accuracy on real tasks, the replan rate on plans over
  eight steps, the plan-cache hit rate, and the growth of episodic memory on
  step-heavy plans.

## Architectural principles

- Principle #3 (Minimal contract): the agent never selects its mode and the AIP
  contract stays `manifest()` plus `run()`; `on_plan_complete()` is optional.
- Principle #4 (Fail fast): a missing `system_prompt` in orchestrated mode fails
  before any Reasoner call, and compaction triggers before the model overflows.
- Principle #5 (One actor, one responsibility): the `Reasoner` builds the plan,
  the `ActorLoop` executes steps, the `ContextManager` only compacts.
- Principle #6 (Memory at agent initiative): relaxed only in orchestrated mode,
  where the runtime drives execution.
- Principle #7 (Non-negotiable safeguards): the `StepBudget` is enforced by the
  runtime in both modes and is never bypassable.

## Related

- [ADR-001](ADR-001-foundations-stack.md) vision and stack foundations the
  engine builds on.
- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) the PyO3 bridge that exposes
  `run()` and `on_plan_complete()` to the engine.
- [ADR-006](ADR-006-tool-subsystem.md) the tool subsystem the orchestrated loop
  invokes.
