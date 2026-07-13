# OBSERVABILITY

> Tracing fields, log levels, semantic conventions. Read this before adding
> a log statement or a span.

Apollia produces structured logs and event streams that flow into the same
ingestion pipeline (`apollia-runtime` `AuditTrail` + tracing-subscriber
console + optional OpenTelemetry export). Consistency across crates is what
makes those pipelines queryable.

---

## 1. Channels

| Channel | What | Producer | Consumer |
|---|---|---|---|
| `tracing` | structured logs and spans | every crate | `tracing-subscriber`, OTLP exporter, log files |
| `EventBus` | runtime events | `apollia-runtime` actors | `AuditTrail`, CLI subscribers, desktop UI |
| `AuditTrail` | append-only event ledger | `apollia-runtime` | SQLite, exported via `apollia audit list` |
| CLI human stdout | user-facing prose or table | `apollia-cli` only | terminal |
| CLI machine stdout | `--json` payloads | `apollia-cli` only | scripts |

Rules :
- `tracing` is the only acceptable log producer outside `apollia-cli`.
- `println!`, `eprintln!`, `dbg!` are forbidden outside `apollia-cli`.
- EventBus events are past-tense (they record what happened).
- AuditTrail entries are immutable. No update, no delete.

---

## 2. Tracing levels

| Level | Meaning | Examples |
|---|---|---|
| `ERROR` | unrecoverable, user-impacting | actor crashed, OAuth refresh failed permanently, DB corrupt |
| `WARN` | degraded but operational | retry succeeded after N attempts, cache miss, fallback path |
| `INFO` | business event | agent started, task completed, tool invoked |
| `DEBUG` | dev visibility | message dispatched, branch taken, intermediate state |
| `TRACE` | very verbose | per-token output, raw bytes, internal loop iterations |

Per-crate defaults are not enforced in code (the operator controls
verbosity with `RUST_LOG`). The rule is consistency in *semantics* :
`INFO` always means "business event worth keeping at production
verbosity".

`apollia-llm` is allowed to default-DEBUG its per-token output. Document
the exception in the crate `AGENTS.md`.

---

## 3. Static message format

`domain.action[.qualifier]`. Lowercase, dot-separated. Verb in present or
past as semantically appropriate.

| Pattern | Use | Example |
|---|---|---|
| `domain.action` | nominal case | `agent.started`, `tool.invoked` |
| `domain.action.qualifier` | when a sub-state matters | `mcp.connect.timeout`, `memory.recall.failed` |
| `domain.entity.action` | when the entity owns the action | `task.budget.exceeded` |

The static message is never a sentence. It is a label. The fields carry
the data.

Bad : `tracing::info!("Agent {} started in {} ms", id, elapsed.as_millis());`
Good : `tracing::info!(agent_id = %id, duration_ms = elapsed.as_millis(), "agent.started");`

---

## 4. Field names, stable workspace-wide

Adding a new field to this table is a schema change. Document it in the PR
that introduces it and update this file.

### Identifiers

| Field | Type | Meaning |
|---|---|---|
| `agent_id` | `String` | `AgentId` value |
| `task_id` | `String` | `TaskId` value |
| `skill_id` | `String` | `SkillId` value |
| `step_id` | `String` | step identifier inside a run |
| `session_id` | `String` | chat or CLI session |
| `run_id` | `String` | one execution of an agent (encompasses all steps) |
| `request_id` | `String` | HTTP request correlation |
| `trace_id` | `String` | OTLP trace identifier |
| `span_id` | `String` | OTLP span identifier |

### Counters and durations

| Field | Type | Meaning |
|---|---|---|
| `step` | `u64` | step counter within a run |
| `attempt` | `u64` | retry attempt counter |
| `duration_ms` | `u64` | elapsed milliseconds |
| `bytes_read` | `u64` | bytes consumed |
| `bytes_written` | `u64` | bytes produced |
| `tokens_in` | `u64` | LLM input tokens |
| `tokens_out` | `u64` | LLM output tokens |

### Classification

| Field | Type | Meaning |
|---|---|---|
| `error_kind` | `&str` | error variant name as a string (`Io`, `Parse`, ...) |
| `tool_name` | `&str` | tool invoked |
| `backend` | `&str` | LLM or MCP backend name |
| `provider` | `&str` | upstream provider (`anthropic`, `openai`, `ollama`, ...) |
| `scope` | `&str` | permission scope (`session`, `project`, `global`) |
| `decision` | `&str` | permission decision (`allow`, `ask`, `deny`) |

### Network

| Field | Type | Meaning |
|---|---|---|
| `method` | `&str` | HTTP method |
| `path` | `&str` | HTTP path |
| `status` | `u16` | HTTP status code |
| `peer` | `&str` | remote endpoint |

---

## 5. Field prefixes

| Prefix | Use |
|---|---|
| `%val` | `Display` impl |
| `?val` | `Debug` impl |
| bare `val` | typed value (already concrete) |
| `field = expr` | named typed value |

Use `%` for IDs, names, and other Display-stable values. Use `?` for
debug-only inspection (development DEBUG / TRACE lines). Never log a full
struct at `INFO` or above unless its `Display` is intentionally concise.

---

## 6. Spans

```rust
#[tracing::instrument(
    skip(self, payload),
    fields(
        agent_id = %self.id,
        skill_id = %skill_id,
    ),
)]
async fn dispatch(&self, skill_id: SkillId, payload: Bytes) -> Result<Reply, Error> { ... }
```

Rules :
- Use `#[instrument]` on async fns worth tracing as a unit of work.
- Skip large or sensitive arguments (`skip(payload)`, `skip(secret)`).
- Add typed fields to the span (`fields(agent_id = %...)`).
- Spans hierarchy : root span = API request or CLI invocation. Child
  spans = actor calls. The whole request fits in one tree.

---

## 7. Error logging discipline

Log an error once, at the boundary where it stops propagating.

```rust
match ctx.tool.invoke(&name, args).await {
    Ok(out) => out,
    Err(err) => {
        // log here because we are converting the error to a user-facing
        // response, not propagating further
        tracing::error!(
            tool_name = %name,
            error_kind = err.kind_name(),
            error = %err,
            "tool.invocation.failed",
        );
        return Err(err.into());
    }
}
```

Never log-and-rethrow. Either log here or let the caller log. Double logs
double the noise without doubling the information.

---

## 8. EventBus events

| Convention | Rule |
|---|---|
| Naming | Past-tense, `UpperCamelCase` (`TaskCompleted`, `AgentCrashed`) |
| Variants | Carry typed fields, no `String` blobs |
| Sensitive data | Never. PII or secrets must not appear in EventBus payloads |
| Capacity | Bounded broadcast channel, capacity [64, 65536] |
| Lag handling | Subscriber receives `Lagged(n)` instead of an event; do not panic, log a `WARN` and resubscribe |

EventBus is a wire format observed by the desktop UI and audit subscribers.
Renaming a variant is a breaking change.

---

## 9. AuditTrail

The audit journal persists events to SQLite as a hash-chained,
append-only ledger. Module : `crates/apollia-runtime/src/audit_journal/`.
Retention controlled by `[audit].retention_days` in the config.

CLI surface (`apollia audit ...`) : `list`, `stats`, `export`, `verify`,
`anchor`, `replay`, `show`.

Add a new audit-worthy event by extending the `JournalEntryKind` enum,
not by emitting a free-form string. Inter-agent mailbox events are
journaled as `MessageSent`, `MessageDelivered`, and `MessageDropped`.

---

## 10. OpenTelemetry compatibility

Apollia logs and spans are designed to round-trip into OTLP without
remapping :

- Field names match OTLP semantic conventions where applicable
  (`http.method`, `http.status_code` mapped from our `method` / `status`).
- Span hierarchy is preserved via `tracing-opentelemetry`.
- Activation : feature `otlp` on `apollia-runtime` (off by default).

Reason : operators run Apollia behind Grafana / Jaeger / Honeycomb without
modifying the codebase.

---

## 11. When the rules block you

- Need a new field : add a row to §4 in the same PR.
- Need to emit a sensitive value : extract a non-sensitive identifier
  (hash, prefix, or category) and log that. Never the raw value.
- Field name conflicts with OTLP convention : OTLP wins; rename ours.
