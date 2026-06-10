# ADR-032: Chat-native plan engine

- Status: Proposed
- Date: 2026-06-10

## Context

The conversational plan-mode epic makes plan mode a first-class, session-level mode
in the chat path, not only a gate on orchestrated task runs. ADR-031 provides the
shared plan type; this ADR decides how the chat path builds and runs a plan.

The chat path is a ReAct loop (`crates/apollia-runtime/src/chat/builtin_agent.rs`):
an LLM call, then tool dispatch, then repeat. Built-in tools such as `todo_write` are
advertised conditionally and executed inline in the loop. The `ask_user` tool already
exists and blocks the turn until the user answers (a ready-made discovery and
arbitration mechanism). A `ChatSession` carries a `mode` enum and per-session state.
There is no bridge from the chat path to the ORIA orchestrated engine, and no way to
pause an in-flight turn and inject an instruction.

Two mechanisms are possible. Reuse the ORIA orchestrated engine inside chat: it has a
real dependency-scheduled `ActorLoop` and a one-shot approval gate, but it is heavy,
runs one task at a time, and its blocking gate does not fit a conversation. Or build
plan management as a chat-native tool surface the agent drives directly, executed by
the existing ReAct loop. The user wants the agent to manage the whole plan
conversationally, to pause and adjust it mid-run, and the desktop to follow live.

## Decision

We build plan mode as a chat-native tool surface, and we keep the ORIA gate for
headless and orchestrated task runs.

- A per-session `PlanActor` (mirroring `todo_actor`) owns the plan DAG using the
  `apollia_core::plan` types, persists it in SQLite (`session_plans`,
  `session_plan_steps`, `session_plan_mutations` with removed-step tombstones),
  validates with `validate_plan`, and emits plan events. It is a bounded `mpsc`
  channel plus a clonable `Handle`, with no shared state across actors.
- A `plan_*` tool surface (`plan_propose`, `plan_add_step`, `plan_modify_step`,
  `plan_remove_step`, `plan_reorder`, `plan_set_step_status`, `plan_submit`) lets the
  agent fully manage the plan. Each mutation records a `PlanMutation` with a reason and
  provenance. The tools are advertised and executed inline in the ReAct loop only when
  the session is in plan mode, exactly like `todo_write`.
- `ChatSession` gains `plan_mode` and a `plan_phase`
  (`Discovery | Drafting | AwaitingApproval | Executing | Done`). Discovery reuses the
  blocking `ask_user` tool so the agent can explore context and ask arbitration
  questions before drafting.
- The gate is a soft, conversational gate, not a blocking one-shot. On `plan_submit`
  the phase becomes `AwaitingApproval`; a desktop approve command resumes the session
  in `Executing`; a user message while awaiting approval makes the agent revise the
  plan through the plan tools. The turn is not blocked inside the loop the way ORIA's
  one-shot gate blocks.
- Pause and inject use a `CancellationToken` threaded into the ReAct loop with
  cooperative checks, plus pause and resume commands. Partial progress is already
  persisted through step statuses; an instruction injected while paused makes the agent
  adjust the plan (a new or modified block carrying a reason) and resume.

Plan execution is the ordinary ReAct loop keeping step statuses current, not ORIA's
dependency-scheduled `ActorLoop`. The `StepBudget` already enforced in the chat loop
continues to govern execution.

## Alternatives considered

### Reuse the ORIA orchestrated engine inside chat (rejected)
**For:** real dependency scheduling, plan cache, resilience layer already exist.
**Against:** pulls a heavy crate into the chat hot path; ORIA is one-task-at-a-time
while chat sessions are concurrent; its blocking one-shot gate does not fit a
conversation; wrapping the ReAct loop in the `ActorLoop` is a large refactor.

### Hybrid: chat-native construction, ORIA execution of the approved plan (rejected for now)
**For:** conversational construction with heavy-duty execution.
**Against:** two engines to coordinate and more moving parts for no immediate gain;
revisit only if a session ever needs true parallel dependency scheduling.

### Chosen: chat-native tool surface
**For:** lightweight and conversational; the agent controls the plan directly through
tools; reuses the proven `todo_actor` and `ask_user` patterns; pause and inject are
natural; the desktop follows via plan events.
**Compromis acceptés:** a second plan-execution path beside ORIA (mitigated by the
single plan type from ADR-031 and shared validation); chat plan execution is reactive
rather than dependency-scheduled, which is the right model for a conversation.

## Consequences

**Positives:**
- Plan mode lives in the chat, with conversational editing, pause and inject, and live
  events for the desktop DAG view.
- Reuses existing patterns, so the surface area of new concepts is small.
- The `StepBudget` keeps governing plan execution (no new bypass).

**Negatives / Compromis:**
- Two plan systems share the type but not the executor (chat ReAct loop vs ORIA
  `ActorLoop`).
- Pause and inject are new machinery in the chat loop (cooperative cancellation and a
  resume path), the riskiest part of the epic.

**Neutres / À surveiller:**
- The LLM must use the plan tools reliably: mitigate with strict tool schemas, a
  discovery/draft/submit/execute state machine, and a plan-mode system-prompt block.
- Per-session actor state under concurrent sessions.

## Architectural principles

- Principle #5 (one actor, one responsibility): `PlanActor` is a bounded `mpsc` plus a
  `Handle`, no `Arc<Mutex>` across actors.
- Principle #6 (memory at agent initiative): the discovery phase must not auto-inject
  memory; the agent calls `ctx.memory.recall` when it chooses.
- Principle #7 (non-negotiable safeguards): plan execution stays inside the budgeted
  ReAct loop; the `StepBudget` remains runtime-enforced.
- Principle #3 (minimal contract): the plan tools are runtime-provided; agents need no
  change to benefit.
- Principle #8 (human CLI, machine API): plan-mode controls are exposed through Tauri
  commands and stay scriptable, with CLI parity to follow.

## Related

- Part of the conversational plan-mode epic; consumes ADR-031 (unified plan type).
- Builds on ADR-022 (chat subsystem) and the human-in-the-loop lineage in ADR-013.
- ORIA (ADR-005) keeps its own one-shot gate for headless and orchestrated task runs.
- Plan mutations feed the audit and replay event defined in the next ADR.
