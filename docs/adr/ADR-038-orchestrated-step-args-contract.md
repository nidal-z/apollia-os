# ADR-038 - Argument contract for orchestrated plan steps

- Status: Accepted
- Date: 2026-07-08

## Context

The orchestrated ORIA path was wired to the real governed `ToolProxy` (permissions + audit + resilience + budget), replacing the `NoopToolProxy`. Doing so exposed a gap: `apollia_core::plan::PlanStep` carries **no structured arguments**, only a description. An orchestrated step therefore cannot pass valid input to the native tools that require it (bash, file, http). A tool with trivial input (echo) works; the others do not. This is the last blocker for the "dispatch a task to an orchestrated agent" capability (cap 2.1).

Value constraint: the strength of orchestration is the **plan as a first-class artifact** (DAG for parallelism, HITL plan-gate, audit and replay). This is the accountability primitive the EU AI Act positioning relies on. The argument contract must preserve that property, not dilute it.

Technical constraints: modifying `PlanStep` touches a **public** model of `apollia-core` (defined by ADR-031), so it goes through the ASK FIRST procedure + this ADR. Apollia already has grammar-constrained generation (GBNF), used by the `do` command and by tool-calling.

## Decision

We adopt a **hybrid A+B** argument resolution for the tool steps of the orchestrated plan:

- **A (default, at plan time)**: `PlanStep` gains a structured-arguments field (`args: Option<serde_json::Value>`). The Reasoner fills it via **schema-guided generation (GBNF)** constrained to the targeted tool's schema, and it is **validated** before execution. The plan is thereby fully specified, deterministic, auditable and replayable with its real arguments.
- **B (fallback, at execution time)**: if a tool step's args are absent or fail validation, the `ActorLoop` triggers a **JIT extraction** (one LLM call mapping description + tool schema to args), validated in turn, before failing the step. A safety net for the cases where the Reasoner did not produce valid args at plan time.

The `ActorLoop` calls `tool_proxy.invoke(tool, args)` with the resolved args (A, then B as fallback). Execution stays entirely under the governed ToolProxy.

## Alternatives considered

### Option B alone - systematic JIT extraction (rejected)
**For:** does not touch the public `PlanStep` model.
**Against:** one LLM call per tool step (cost, latency), non-deterministic, and the plan stays under-specified, which degrades plan audit and replay. Loses the "fully specified plan" property.

### Option C - native tool-calling inside the plan (rejected)
**For:** reuses the chat-path mechanism, already proven.
**Against:** it makes orchestration converge toward generic ReAct and dilutes the plan-as-artifact. It loses part of the DAG / audit / replay moat, which is precisely the EU AI Act differentiator.

### Chosen: A + B hybrid
**For:** A preserves the auditable and replayable plan (the moat); B brings robustness without sacrificing that property. It builds on the GBNF already present.
**Trade-offs:** two arg-resolution paths (more complexity and tests); modification of a public `apollia-core` model.

## Consequences

**Positives:**
- Orchestration finally drives real native tools: unblocks cap 2.1.
- The plan stays a fully specified, auditable and replayable artifact: the EU AI Act primitive is reinforced, not diluted.
- Reuses the existing GBNF (no new building block).

**Negatives / Trade-offs:**
- Change to a public `apollia-core` model (`PlanStep`): touches plan-mode, `audit_journal` (plan snapshots), replay, the desktop plan-mode UI, and forces a migration / default value for existing plans. Cross-cutting work.
- Two arg-resolution paths (A + fallback B): increased complexity and test surface.
- Fallback B adds an LLM call when it triggers (cost/latency on those cases).

**Neutral / Watch:**
- The trigger rate of fallback B: if it is high, arg generation at plan time (A) is weak and must be improved.
- Replay compatibility with old plans without args (migration / default to `None`).

## Architectural principles

- **Principle #7 - Non-bypassable safeguards**: the resolved args go through the governed `ToolProxy` (permissions + audit + budget); execution stays under guard.
- **Audit / accountability moat**: a fully specified plan reinforces auditability and replay.
- Modifies a **public** model of `apollia-core`: the ASK FIRST procedure is respected via this ADR; it extends ADR-031.

## Related

- ADR-031 (unified plan model in apollia-core): this ADR extends `PlanStep`.
- ADR-037 (host driving contract): the preceding workstream.
- Origin: the budget-safeguards + orchestration workstream report, which exposed the need.
