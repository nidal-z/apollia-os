# OBSERVABILITY

> Tracing fields, log levels, semantic conventions. Read this before adding
> a log statement or a span.

Apollia produces structured logs and event streams that flow into the same
ingestion pipeline (`apollia-runtime` audit journal + tracing-subscriber
console). Consistency across crates is what makes those pipelines queryable.
There is no telemetry export of any kind, and adding one would be a
sovereignty decision before it is a technical one.

---

## 1. Channels

| Channel | What | Producer | Consumer |
|---|---|---|---|
| `tracing` | structured logs and spans | every crate | `tracing-subscriber` console, log files |
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

`apollia-llm` is allowed to default-DEBUG its per-token output. That crate
carries no `AGENTS.md`, so the exception is recorded here and nowhere else.

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

A message that reads as prose is not a stylistic slip, it is a key that does
not group: `agent.started` can be counted, `Agent xyz started in 12 ms` answers
a different string every time. Where the sentence carried something the label
cannot, that something becomes a field, `reason` for the cause and `detail` for
the consequence. `scripts/check_tracing_messages.py` holds the rule, per crate,
on a ratchet that only descends.

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
| `skill_id` | `String` | skill identifier, a bare `String` in the code |
| `step_id` | `String` | step identifier inside a run |
| `session_id` | `String` | chat or CLI session |
| `run_id` | `String` | one execution of an agent (encompasses all steps) |
| `request_id` | `String` | HTTP request correlation |

### Counters and durations

| Field | Type | Meaning |
|---|---|---|
| `step` | `u64` | step counter within a run |
| `attempt` | `u64` | retry attempt counter |
| `duration_ms` | `u64` | elapsed milliseconds |
| `tokens_in` | `u64` | LLM input tokens, one site; §7 explains why the engine events use a triplet instead |
| `tokens_out` | `u64` | LLM output tokens, one site |

### Classification

| Field | Type | Meaning |
|---|---|---|
| `tool_name` | `&str` | tool invoked |
| `backend` | `&str` | LLM or MCP backend name |
| `provider` | `&str` | upstream provider (`anthropic`, `openai`, `ollama`, ...) |
| `scope` | `&str` | permission scope (`session`, `project`, `global`) |
| `decision` | `&str` | permission decision (`allow`, `ask`, `deny`) |
| `reason` | `&str` | why the event happened, when the label cannot say it |
| `detail` | `&str` | the consequence the label does not carry (`"agent starts degraded"`) |

### Network

| Field | Type | Meaning |
|---|---|---|
| `method` | `&str` | HTTP method |
| `path` | `&str` | HTTP path |
| `status` | `u16` | HTTP status code |
| `peer` | `&str` | remote endpoint |

### Local inference engine

Emitted by `llama.server.spawn.config`, once per embedded `llama-server`
launch. Each field is the resolved value of one launch parameter, after the
`APOLLIA_LLAMA_` environment overrides have been applied. An optional
parameter left unset reads `unset`, meaning the flag is not passed and the
engine's own default applies.

| Field | Type | Meaning |
|---|---|---|
| `binary` | `&str` | path of the `llama-server` executable |
| `model` | `&str` | path of the loaded `.gguf` file |
| `port` | `u16` | loopback port the server binds |
| `n_ctx` | `u32` | context window in tokens (`-c`) |
| `n_gpu_layers` | `i32` | layers offloaded to the GPU (`-ngl`) |
| `n_batch` | `&str` | logical batch size (`-b`), or `unset` |
| `n_ubatch` | `&str` | physical micro-batch size (`-ub`), or `unset` |
| `n_parallel` | `&str` | server slot count (`-np`), or `unset` |
| `cont_batching` | `&str` | continuous batching (`-cb` / `-nocb`), or `unset` |
| `cache_type_k` | `&str` | KV cache type for keys (`-ctk`), or `unset` |
| `cache_type_v` | `&str` | KV cache type for values (`-ctv`), or `unset` |
| `flash_attn` | `&str` | flash attention mode (`--flash-attn`), or `unset` |
| `cache_reuse` | `&str` | prefix cache reuse threshold (`--cache-reuse`), or `unset` |
| `args` | `&str` | the full launch argument vector, space-joined |
| `metrics` | `&str` | whether the Prometheus endpoint was opened (`--metrics`) |

`args` is the provenance record: a performance measurement is only comparable
to another when both quote the exact launch line that produced them.

### Completion timings

Emitted by `llm.completion.timings`, once per completion, from the engine's own
per-request `timings` object. The table below is the naming authority: a field
means here what it means in every log line and every analysis that reads them,
and the engine's own key names are deliberately not reused where they would
mislead. A rate that is undefined reads `unset`, never `0`.

| Field | Type | Meaning |
|---|---|---|
| `prompt_tok_total` | `u32` | every prompt token submitted, cached or recomputed |
| `prompt_tok_computed` | `u32` | prompt tokens the engine actually evaluated |
| `prompt_tok_cached` | `u32` | prompt tokens served from the KV cache |
| `prompt_cache_hit_ratio` | `&str` | cached share of the submitted prompt, or `unset` |
| `prefill_ms` | `f64` | prefill duration, covering the computed tokens only |
| `decode_tok` | `u32` | tokens generated, reasoning and content together |
| `decode_ms` | `f64` | generation duration |
| `prefill_tps` | `&str` | derived prefill rate, or `unset` when nothing was computed |
| `decode_tps` | `&str` | derived decode rate, or `unset` |
| `engine_prefill_tps` | `&str` | the engine's own prefill rate, kept as a cross-check |
| `engine_decode_tps` | `&str` | the engine's own decode rate, kept as a cross-check |

`prompt_tok_*` is a triplet rather than a single field because the engine counts
only what it evaluated: a name like "prompt tokens" that silently excludes
cached tokens is the ambiguity most likely to corrupt a measurement. For the
same reason `tokens_in` and `tokens_out` above are not admissible on this event.

### Turn decomposition

Emitted by `chat.react.turn.timings`, once per user-visible turn. A turn holds
one or more completions and any number of tool invocations; these fields say how
its wall-clock divided between them.

| Field | Type | Meaning |
|---|---|---|
| `turn_wall_ms` | `f64` | accepting the user message to emitting the final reply |
| `iterations` | `usize` | completions issued to the engine in this turn |
| `tool_calls` | `usize` | tool invocations executed in this turn |
| `approvals` | `usize` | human approvals waited on in this turn, whatever the answer |
| `engine_ms_total` | `f64` | sum over iterations of `prefill_ms + decode_ms` |
| `tool_ms_total` | `f64` | sum of every tool invocation's wall-clock |
| `approval_ms_total` | `f64` | sum of every approval wait |
| `orchestration_residual_ms` | `f64` | `turn_wall_ms` minus the three sums above |
| `orchestration_residual_ratio` | `f64` | the residual as a share of the turn |

`orchestration_residual_ms` is the part that belongs to Apollia rather than to
the engine, a tool, or a person, so it is computed explicitly rather than left to
be inferred. It can come out negative, which means work overlapped and the
additive model does not hold for that turn; it is reported as measured, never
clamped.

`approvals` is not `tool_calls`. An approval that is refused, or that times out,
waits and then runs nothing, so the counts diverge on exactly the turns where the
wait dominates. Human wait is subtracted from the residual for the same reason:
a turn that waited on a person is not a slow turn, and one unanswered approval
was enough to report a 98.7 percent residual with nothing running.

Spans: `chat.react.turn` wraps one user turn, `chat.react.iteration` wraps each
completion inside it.

| Field | Type | Meaning |
|---|---|---|
| `env_var` | `&str` | environment variable name, on a configuration warning |
| `value` | `&str` | the rejected raw value |

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
| Naming | Past-tense, `UpperCamelCase` (`TaskCompleted`, `AgentLoadFailed`) |
| Variants | Carry typed fields, no `String` blobs |
| Sensitive data | Never. PII or secrets must not appear in EventBus payloads |
| Capacity | Bounded broadcast channel, capacity [64, 65536] |
| Lag handling | Subscribe through `apollia_core::events::subscribe_resilient`, which logs a `WARN`, resubscribes and continues. Do not restate the rule at a call site |

EventBus is a wire format observed by the desktop UI and audit subscribers.
Renaming a variant is a breaking change, and so is removing one. The catalogue
is published at `docs/site/docs/reference/events.md`, generated from the enum
and from the desktop bridge's categories by `docs/site/regen.sh`.

The category the bridge attaches decides who reads the variant: the webview
dispatches on the category, never on the variant name. A variant given a
category no listener reads reaches the interface and is dropped in silence.

---

## 9. AuditTrail

The audit journal persists events to SQLite as a hash-chained,
append-only ledger. Module : `crates/apollia-runtime/src/audit_journal/`.
Retention is `retention_days` under `[observability]` in the config
(`crates/apollia-core/src/observability.rs`), default 90 days. There is no
`[audit]` table.

CLI surface (`apollia audit ...`) : `list`, `journal`, `stats`, `export`,
`verify`, `anchor`, `replay`, `show`.

Add a new audit-worthy event by extending the `JournalEntryKind` enum,
not by emitting a free-form string. Inter-agent mailbox events are
journaled as `MessageSent`, `MessageDelivered`, and `MessageDropped`.

---

## 10. Telemetry export

There is none. No manifest declares an exporter, no crate carries a feature
for one, and no field is emitted for a distributed trace.

The section that used to sit here announced an OTLP exporter behind a
feature flag, with correlation fields to match, and an operator who read it
went looking for a switch that had never been written. What ships is the
console subscriber and the audit journal, both local, which is what principle
1 asks for.

If an export ever lands it is an ASK FIRST: sending spans off the machine is
a sovereignty change before it is a dependency.

---

## 11. When the rules block you

- Need a new field : add a row to §4 in the same PR.
- Need to emit a sensitive value : extract a non-sensitive identifier
  (hash, prefix, or category) and log that. Never the raw value.
- A field name is ambiguous : rename it before it spreads. `prompt_tok_*`
  in §7 is the worked example, split into a triplet rather than left as one
  count that silently excluded cached tokens.
