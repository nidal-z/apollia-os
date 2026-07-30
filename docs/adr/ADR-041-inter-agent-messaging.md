# ADR-041 - Durable and auditable inter-agent messaging

- Status: Accepted
- Date: 2026-07-10

## Context

The runtime already has a functional actor mailbox (`crates/apollia-runtime/src/mailbox.rs`),
spawned permanently at startup (`supervisor.rs:1181`) and wired into `AppState`
(`api/server.rs:179`) and the embedded path (`embedded.rs:88`). It manages bounded per-recipient
queues, already emits `RuntimeEvent::AgentMessageSent` (`mailbox.rs:213`), and a read-only HTTP
route exposes it (`GET /api/v1/agents/{name}/messages`, `routes_messages.rs:60`).
But no agent can use it: the `ctx` surface was never laid down. The helpers
`send_inner`/`receive_inner` (`context.rs:1891-1909`) and the `RuntimeContext.mailbox` field
(`context.rs:1095`) are dead, referenced only by tests. This is a half-built capability: the
infra exists, the agent API is missing.

Decisive history: **ADR-024 explicitly removed** the old `ctx.send`/`ctx.receive`
(`ADR-024:82-89`), citing four unresolved objections:

1. persistence unspecified,
2. TTL unspecified,
3. delivery to a stopped recipient unspecified,
4. no clear boundary against `ctx.a2a.invoke`.

ADR-024 deferred "a real asynchronous bus until a concrete use case justifies a clean
specification". This workstream brings that use case (six professional multi-agent coordination
scenarios documented in D1) and that clean specification (D2). This ADR lifts the four objections
one by one and supersedes ADR-024's removal stance.

Why now: asynchronous messaging unblocks patterns that the synchronous RPC `ctx.a2a.invoke`
(ADR-025) cannot express without blocking the caller (streaming aggregated fan-out,
producer/consumer notification, long-task handoff, host supervision, out-of-band cancellation,
non-blocking progress). It is also a product differentiator aligned with the beachhead: auditable
inter-agent messaging that the host can drive, a direct argument for the EU AI Act (record-keeping,
oversight) and for "integration is the product" (ADR-037).

Constraint: the eight principles (notably #5 one actor one responsibility, #7 non-bypassable
safeguards, #6 agent-initiated, #1 local-first, #8 machine API), and the `Ctx` contract format
verified at load (ADR-024).

## Decision

We adopt an inter-agent messaging that is **durable, auditable and host-drivable**, exposed to
agents under a new dedicated service `ctx.mail`, distinct from `ctx.a2a`. It answers ADR-024's
four objections point by point.

### A dedicated service `ctx.mail` (lifts objection 4)

Messaging becomes the 15th service of the `Ctx` contract (`sdk/apollia/types.py`), not a facet
of `ctx.a2a`. The mental boundary is sharp and documented: `ctx.a2a.invoke` calls a skill and
awaits a typed result (synchronous RPC, ADR-025); `ctx.mail.send` posts a message into an agent's
mailbox and continues (asynchronous, non-blocking). The agent API is
`send`/`receive`/`poll`/`pending`/`list`/`ack`/`nack`, backed by a Rust pyclass
`MailInterface` mirroring `A2AInterface` (`a2a.rs:38-50`), wired into `RuntimeContext` by the
same pattern (`Option<Py<...>>` field, construction under `with_gil`, `#[getter]`). Adding a
service = a minor SemVer bump of the SDK contract.

### A durable SQLite store (lifts objections 1 and 3)

Messages are persisted in a SQLite table owned exclusively by the mailbox actor
(one connection, one actor, principle #5, on the audit-journal actor pattern). They survive
restart as long as they are not acknowledged; a stopped recipient finds its messages again
on return. Delivery is **at-least-once**: `receive` leases the message (in-flight state, visibility
timeout, default 60 s) instead of deleting it; the ack deletes it; a crash before ack lets the
lease expire, which redelivers the message. The ack is automatic when the consumer context
completes successfully; explicit `ack`/`nack` remain available. This at-least-once choice is
settled at specification time (not deferred), because it shapes the SQLite schema and the API:
adding it afterwards would be a refactor.

Ordering is **best-effort FIFO per recipient**, not strict: a message whose lease expires
(or is refused by `nack`) is redelivered after more recent messages already delivered. This is
inherent to at-least-once queues with a visibility timeout; strict ordering is not guaranteed
under redelivery, and this limit is assumed explicitly rather than falsely promised.

A message payload size is bounded by a configurable limit
(`mailbox_max_payload_bytes`) rejected at send time, to prevent an agent from inflating the
durable store.

### A TTL and bounded eviction (lifts objection 2)

Each message carries `sent_at`; a sweep evicts messages never picked up beyond a configurable
TTL (`mailbox_message_ttl`, default 24 h) and redelivers leased messages whose lease has expired
(`mailbox_visibility_timeout`, default 60 s). Eviction emits `AgentMessageDropped`
{ reason: expired } and an audit entry. The store is thereby bounded, the TTL objection lifted.

### Addressing, scoping and safeguards

- Unicast addressing by registered agent name, self-addressing allowed; fan-out is done
  by N unicast sends. Broadcast/topics/groups deferred to future extensions (sobriety).
- Scoping: a `mailbox` capability declared in the manifest, mandatory opt-in (like
  secrets/datasources), with an optional recipient allowlist. Without declaration, no access.
- Unknown recipient: `send` validates against the `AgentRegistry` and returns
  `MailboxError::UnknownRecipient` (fail-fast, principle #4), correcting the current behavior
  of silent queue creation (`mailbox.rs:200`).
- Anti-spam: a per-run send quota applied in the actor (a prudent default, on the order of 50
  sends per run, configurable), emitting `MailboxGuardTriggered` on the pattern of
  `A2AGuardTriggered` (`events.rs:1050`). The `StepBudget` is not overloaded (a message is not
  a reasoning step). Non-bypassable from Python (principle #7).
- HITL: not gated by default (local messaging, principle #1); opt-in gate via
  `PermissionEngine` (synthetic tool name `mailbox:send`) or `tools_requiring_approval`,
  enforceable by the host.

### Provable auditability

Each send, delivery, ack and drop emits a `RuntimeEvent` and, via the subscriber, an
HMAC-SHA256-signed and chained audit-journal entry. Hard prerequisite: mailbox events
must carry a `run_id`, failing which the subscriber ignores them
(`subscriber.rs:483`). For a message sent by an agent, it is the sender's `run_id`.
For a message **injected by the host** (which has no agent run), the injection allocates a
host-scoped synthetic `run_id`, so that injected messages are journaled on their own audit chain
and the invariant "everything journaled carries a `run_id`" holds without a special case in the
subscriber. Without this synthetic `run_id`, host injection would pierce the "the host injects and
everything is auditable" promise. Entries carry `from`, `to`,
`message_id`, `payload_hash`, `sent_at`; the full payload is journaled only if a runtime flag
enables it (`mailbox_audit_full_payload`, off by default). Non-repudiation proof without
storing content at rest. Everything stays fire-and-forget (no impact on the send path).

### Host control (driving contract)

The `/api/v1` API (ADR-037) exposes, additively and non-breaking: observation
(`GET .../messages` existing + an SSE stream `GET /api/v1/mailbox/stream`), proof
(`GET /api/v1/mailbox/audit`), injection (`POST /api/v1/agents/{name}/messages`, sender
`host:<id>`, with allocation of a host-scoped synthetic `run_id` for auditability), and
the gate (routing via `PermissionEngine`, hold-for-approval policy). All annotated with utoipa, so
propagated automatically to the TS and Python host SDKs (`clients/regen.sh`). The host stays master
of the choreography.

## Alternatives considered

### Remove the mailbox for good (rejected)

**For:** consistent with ADR-024's stance, zero new surface.
**Against:** wastes an infra already built and wired, and abandons a product differentiator
(auditable asynchronous coordination) now justified by concrete use cases.
ADR-024 had explicitly conditioned removal on the absence of a use case and a spec; both
now exist.

### Volatile in-memory queue (status quo) or hybrid (rejected)

**For:** the simplest; the existing actor is already in-memory.
**Against:** loses messages on restart, incompatible with durable handoff and the
"reliable + auditable" promise. The hybrid (volatile queue + persisted audit) proves but does not
guarantee delivery. Set aside by product arbitration in favor of the full guarantee.

### At-most-once delivery (delete on pickup) (rejected)

**For:** simpler schema and API (no lease, no ack).
**Against:** inconsistent with a durable store chosen for reliability; an agent crash
between `receive` and end of processing would lose the message. The processing guarantee is
precisely what the product sells. Adding it in v2 would be a schema and API refactor.

### Extend `ctx.a2a` with messaging methods (rejected)

**For:** no new service.
**Against:** reintroduces exactly the boundary blur ADR-024 wanted to remove. A
separate service keeps the mental model clean.

### Count sends against the `StepBudget` (rejected)

**For:** reuses an existing non-bypassable safeguard.
**Against:** pollutes reasoning-budget accounting (a send is not a step). A dedicated
guard on the A2AGuard pattern is more semantically correct.

### Chosen: durable, auditable, drivable messaging

**For:** lifts ADR-024's four objections, unblocks the asynchronous use cases, serves the
beachhead (auditability + host control), reuses existing infra and patterns (actor,
A2AInterface, audit journal, PermissionEngine, driving contract).
**Trade-offs:** migration of the actor from `VecDeque` to a SQLite store (contention and
GC to manage), broadened contract surface (15th service, additive endpoints and events),
lease/ack complexity assumed from v1.

## Consequences

**Positives:**
- The half-built capability becomes a complete and coherent product.
- Concrete EU AI Act differentiator: prove what the agents said to each other, and give the host
  a grip on their coordination.
- Sharp `mail` vs `a2a` boundary, ADR-024's design debt resolved cleanly.
- Additive and non-breaking: existing agents and `ctx.a2a` are not affected.

**Negatives / Trade-offs:**
- The mailbox actor becomes stateful on SQLite (schema, migration, GC, contention) instead of a
  simple `VecDeque`.
- The `Ctx` contract goes from 14 to 15 services, forcing a documentation regeneration (rulebook,
  reference docs, host SDK).
- The at-least-once lease and ack add complexity to the consumption path.

**Neutral / Watch:**
- Growth of the durable store under heavy traffic (TTL, quota and payload bound as limits).
- Contention of the mailbox actor if many agents consume simultaneously.
- Chosen values, to tune with usage: visibility timeout 60 s, TTL 24 h, per-run send quota
  on the order of 50, automatic ack on success plus optional explicit `ack`/`nack`.
- Best-effort FIFO ordering (not strict under redelivery): to watch if a use case
  requires strict ordering, which would then be an extension.

## Architectural principles

- **Principle #1 - Local-first:** all messaging stays in-process and local; no message
  crosses the machine boundary.
- **Principle #5 - One actor, one responsibility:** the durable store is owned exclusively by
  the mailbox actor (bounded mpsc + cloneable handle), never a shared `Arc<Mutex<T>>`.
- **Principle #6 - Agent-initiated memory:** the pull model; the recipient picks up
  its messages when it decides, never an automatic injection.
- **Principle #7 - Non-bypassable safeguards:** anti-spam quota, per-recipient cap, capability
  and permission gating are applied by the runtime, non-bypassable from Python.
- **Principle #4 - Fail fast:** unknown recipient, undeclared capability, and invalid config
  fail early.
- **Principle #8 - Machine API:** the host exposure extends the versioned driving contract.

## Related

- Related ADRs: ADR-024 (`ctx` runtime contract, which had removed the mailbox; superseded on this
  point), ADR-025 (workers and synchronous A2A routing, complement to the mailbox), ADR-037 (host
  driving contract, extended here), ADR-023 (AgentKit decorators, capability manifest),
  ADR-015 (permission governance), ADR-033 (signed audit journal and `JournalEntryKind`,
  extended here with the Message* kinds), ADR-012 (observability and EventBus)
