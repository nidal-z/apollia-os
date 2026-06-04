# ADR-012: Observability and plan feedback

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia traces task status and tool calls, but execution data is fragmented: tasks
record their final status but not their full input, output, or real duration; steps
persist `output` and `error` but not the rendered input or the millisecond duration;
LLM calls emit events but nothing persists them, so costs cannot be computed after the
fact and a prompt that produced a bad plan cannot be debugged; triggers and HITL
approvals log activation but not the full payload or the exact timing.

The runtime must make every agent action traceable after the fact while staying
local-first and avoiding any external migration tooling: persistence is `rusqlite`
inline. Two further constraints shape the design: LLM prompts can contain personal
data (a GDPR concern), and inputs and outputs can reach hundreds of kilobytes.

A second, related question is response quality. Plan quality depends on sampling
temperature and prompt style. Without a feedback mechanism, the runtime cannot capture
which of two candidate plans a user prefers.

## Decision

We adopt full SQLite-backed observability with configurable truncation and a unified
timeline endpoint, plus an optional plan-feedback mechanism that presents two
alternative plans and logs the user's choice.

### Schema extensions inline

Observability columns are added through plain `ALTER TABLE ... ADD COLUMN` statements in
the existing SQLite init functions of each crate (`audit.rs`, `task_repository.rs`,
`plan_repository.rs`, `persistence.rs`). SQLite has no `ADD COLUMN IF NOT EXISTS`, so
idempotency comes from wrapping each `ALTER TABLE` in an ignore-error path or guarding
it with a column-introspection check before issuing it. A new `llm_calls` table is
created with `CREATE TABLE IF NOT EXISTS`. No external migration tool is introduced.

### Persistence with configurable truncation

All inputs, outputs, stdout, and stderr are persisted to SQLite with configurable
truncation via `ObservabilityConfig`:

```rust
pub struct ObservabilityConfig {
    pub max_input_bytes: usize,       // default 32768
    pub max_output_bytes: usize,      // default 32768
    pub max_tool_output_bytes: usize, // default 10240
    pub debug_log_prompt: bool,       // default false
    pub capture_thoughts: bool,
    pub capture_llm_prompts: bool,
    pub capture_tool_args: bool,
    pub capture_tool_outputs: bool,
    pub capture_agent_logs: bool,
    pub retention_days: u32,
}
```

(The canonical `ObservabilityConfig` lives in `crates/apollia-core/src/observability.rs`.
An unrelated, identically named `ObservabilityConfig` also exists in
`apollia-llm/src/router.rs`; the two are not the same type.)

When text exceeds its limit, the runtime truncates on a valid UTF-8 boundary, appends a
marker, and sets a companion `*_truncated INTEGER NOT NULL DEFAULT 0` flag. Truncation
with a marker is preferred over rejecting the record: partial observability beats none.

### Unified timeline endpoint

`GET /api/v1/tasks/{id}/timeline` returns
`Json<TimelineResponse { task_id, events: Vec<TimelineEvent> }>`, the events sorted by
ascending timestamp. The handler reads five sources (tasks, plan_steps, llm_calls,
tool_invocations, task_approvals), merges them into timestamped tuples, sorts, and
serializes. Server-side aggregation guarantees temporal consistency and avoids forcing
each consumer to re-implement the merge.

### Prompt persistence gated by a debug flag

The `llm_calls` table has a nullable `prompt_text` column. By default
(`debug_log_prompt = false`) it stays `NULL`. The operator must explicitly set
`debug_log_prompt = true` under `[observability]` in `apollia.toml` to persist prompts.
The default is safe because prompts may contain personal data in clear text.

### Plan feedback logged to `plan_choices`

When enabled, the planner generates two candidate plans in parallel. A single
`plan_with_alternatives(ctx, config)` call runs both branches via `tokio::join!` at two
configurable temperatures (a conservative one and a creative one) and returns a
`PlanAlternatives` value:

```rust
let (plan_a, plan_b) = tokio::join!(
    self.plan_internal(ctx, Some(config.plan_alternatives_temp_a)),
    self.plan_internal(ctx, Some(config.plan_alternatives_temp_b)),
);
```

The agent presents both plans labeled A and B and waits for a choice. The choice is
logged to the `plan_choices` table, keyed by `session_id` (columns `session_id`,
`chosen`, `plan_a_json`, `plan_b_json`, `chosen_at`), for later analysis. `plan_choices`
is the SQLite table name; `PlanAlternatives` is the in-memory Rust type returned by the
planner, never a table. The feature is disabled by default. More than two alternatives is
rejected: a binary choice gives the cleanest signal, beyond two the decision time grows
and each extra plan adds a full LLM call.

## Alternatives considered

### Separate `.sql` files with a migration tool (rejected)
- Pros: explicit schema versioning, mature tooling.
- Cons: adds a dependency and a paradigm the project does not use; `ALTER TABLE` inline
  is idempotent and fits the existing init functions.

### File storage for large inputs/outputs (rejected)
- Pros: no size limit, no truncation.
- Cons: fragments the audit trail between SQLite and the filesystem; orphaned files,
  harder backup, a sixth timeline source.

### Always persist LLM prompts (rejected)
- Pros: simplest debugging.
- Cons: prompts hold personal data; persisting them by default in unencrypted SQLite is
  a GDPR problem. Opt-in is safer.

### Reject records over the limit (rejected)
- Pros: no data loss.
- Cons: the operator loses all visibility into large tasks; no observability is worse
  than partial observability with an explicit marker.

### More than two plan alternatives (rejected)
- Pros: more options for the user.
- Cons: decision time grows, each extra plan adds an LLM call, and the choice signal
  degrades past a binary pick.

### Chosen: SQLite observability plus the `plan_choices` log
- Pros: no black box, computable LLM costs, a single ordered timeline call, bounded
  database growth, prompts protected by default, a clean A/B feedback signal.
- Trade-offs: databases grow with persisted inputs/outputs; `apollia-llm` gains a
  `rusqlite` dependency; enabling plan feedback doubles the planning LLM cost.

## Consequences

- Positive: every agent action is traceable after the fact; LLM costs are computable per
  backend and model; the timeline is one ordered API call; prompts are GDPR-safe by
  default; the `plan_choices` log enables preference analysis with no external
  service.
- Negative / trade-off: SQLite size grows (mitigated by 32KB-per-field truncation);
  `prompt_text = NULL` by default makes LLM debugging less direct; plan feedback doubles
  planning cost when enabled and is logged rather than applied automatically.
- Watch: rotation and archival of old observability data; timeline latency across five
  sequential reads; growth of the `plan_choices` log on active runtimes.

## Architectural principles

- Principle #1 (local-first): all observability data and the `plan_choices` log stay
  in local SQLite, no external send.
- Principle #2 (zero external dependency): no external migration tool, `rusqlite` inline.
- Principle #4 (fail fast): a database that fails to open is a fatal startup error; a
  failed plan branch surfaces immediately through `tokio::join!`; truncation marks loss
  rather than silencing it.
- Principle #8 (human CLI, machine API): the timeline returns structured JSON, compatible
  with `--json`.

## Related

- [ADR-005](ADR-005-oria-execution-model.md) the ORIA execution model whose planner emits
  the two candidate plans and whose steps feed the timeline.
- [ADR-013](ADR-013-human-in-the-loop.md) the HITL approvals that contribute a timeline
  source.
