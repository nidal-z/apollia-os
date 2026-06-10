# ADR-033: Plan-construction audit and replay event

- Status: Proposed
- Date: 2026-06-10

## Context

The conversational plan-mode epic requires the construction of a plan to be
auditable and replayable: every block added, modified, or removed during a session
carries a reason and provenance (ADR-031), and the operator can replay how the plan
was built. ADR-032 makes the chat engine emit a `PlanMutation` for each change.

The audit journal (`crates/apollia-runtime/src/audit_journal/`) is an append-only,
per-run log, hash-chained with SHA256 so any deletion, reordering, or mutation breaks
the chain and is detectable. It captures tool calls, LLM calls, and agent lifecycle
events. A separate deterministic-replay layer is being built to capture the
non-deterministic inputs of a run (LLM responses, tool outputs, clock, randomness) and
re-execute it through a replay harness, comparing outputs to detect divergence.

Neither captures plan construction today. Plan revisions only survive as a counter:
`PlanRepository::begin_replan` deletes pending steps. Recording a new kind of audit
event is a security-boundary change (ARCHITECTURE Section E), so it is decided here.

## Decision

We make `PlanMutation` a first-class audit event and extend the replay layer to cover
plan construction.

- We add `JournalEntryKind::PlanMutation(PlanMutationSnapshot)` to the audit journal.
  The snapshot carries the mutation kind, the affected `step_id`, the reason, the
  `before` and `after` step, the plan revision, and a strictly increasing ordinal,
  hash-chained like every other entry. The audit subscriber maps the plan event emitted
  by the chat engine (ADR-032) to this entry.
- We extend the replay harness with a plan-mutation cursor: on replay, plan mutations
  must occur in the same order with the same reason and the same before and after. A
  mismatch (a different step added, removed, or modified) is reported as a replay
  divergence, alongside the existing input categories. The replay CLI surface reports
  plan divergences with the rest.
- The desktop "replay construction" scrubber and the audit viewer read the same
  canonical history: the `session_plan_mutations` table for the live session and the
  hash-chained journal for the durable, tamper-evident record.

The deterministic-replay scope (capture, harness, and CLI) is updated to include the
plan-mutation category.

## Alternatives considered

### Store plan history only in the chat SQLite table (rejected)
**For:** simplest, no journal change.
**Against:** not tamper-evident, not part of deterministic replay, and a second source
of truth diverging from the audit record.

### Reconstruct history from existing plan lifecycle events (rejected)
**For:** no new event type.
**Against:** the existing events lack per-step reason and provenance and lack the
before and after, so the reconstruction is lossy and cannot drive a faithful replay.

### Chosen: a first-class PlanMutation journal entry plus a replay cursor
**For:** a tamper-evident construction trace; deterministic replay that includes plan
building; one canonical history shared by the UI scrubber and the audit viewer.
**Compromis acceptés:** a new journal schema to version and maintain; the replay
harness gains one more cursor and divergence rule.

## Consequences

**Positives:**
- Plan construction becomes auditable and replayable, which strengthens the traceability
  and compliance story.
- The "replay construction" experience is backed by the hash-chained journal, not a
  mutable table alone.

**Negatives / Compromis:**
- A new journal entry schema that must be versioned for forward compatibility.
- The replay harness grows in complexity (an extra cursor and comparison path).

**Neutres / À surveiller:**
- Serialization stability of `PlanMutationSnapshot` across versions.
- Volume: rapid successive mutations may need coalescing to keep the journal readable.

## Architectural principles

- Audit lineage: append-only, hash-chained, no deletes, consistent with the permissions
  audit spirit (ADR-015) and observability (ADR-012).
- Principle #7 (non-negotiable safeguards): a replayable, divergence-checked construction
  trace reinforces runtime authority and traceability.
- Principle #1 (local-first): the journal stays on the machine.

## Related

- Part of the conversational plan-mode epic; consumes ADR-031 (the `PlanMutation` type)
  and ADR-032 (the engine that emits mutations).
- Extends the deterministic-replay layer (capture, harness, CLI).
- Related observability and audit decisions: ADR-012 and ADR-015.
