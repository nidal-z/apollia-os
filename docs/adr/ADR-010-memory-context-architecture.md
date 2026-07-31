# ADR-010: Memory and context architecture

- Status: Accepted
- Date: 2026-06-04

<!-- claim:memory-injection-confined-to-builtin-assistant -->

> **Scope note, 2026-07-31.** The decision below stands for agents, which is what
> it was written about. Read "the runtime never injects memory into an LLM call"
> as "into an agent's LLM call". The built-in conversational assistant, which did
> not exist in this form when this was decided, injects in two places: a
> user-persona brief at the `long_autonomous` tier, and up to three past session
> summaries on the first message of a free chat session. Neither is reachable
> from an agent execution path, so the decision is not reversed, only bounded.

## Context

Apollia agents need three distinct capabilities that are easy to conflate: durable
memory the agent accumulates over time, full-text recall over that memory, and a
description of the situation the agent runs in (the current project, branch, rules).

Modern AI frameworks blur these by injecting memory automatically into every LLM
call. That produces hidden LLM costs, context noise, and opaque behavior. Apollia
targets predictable behavior and bounded cost, so the memory subsystem must be
explicit and the context subsystem must be cheap, bounded, and never confused with
memory.

The memory store lives in `apollia-memory` (SQLite with FTS5, WAL mode) at
`~/.apollia/memory.db`. The ContextManager that assembles situational context lives
in `apollia-oria`. Both must respect Principle #6: nothing is ever injected into a
prompt without the agent asking for it.

## Decision

We adopt an explicit, agent-initiative memory subsystem backed by SQLite + FTS5,
paired with a separate, bounded context assembly layer. Memory and context never mix.

### Memory is always at agent initiative

The runtime never injects memory into an LLM call. The agent calls
`ctx.memory.search()`, `ctx.memory.recall()`, `ctx.memory.record()` explicitly. Three
memory types coexist: episodic (append-only event log with optional TTL), semantic
(key/value), and procedural (named step sequences). The agent decides when and what
to retrieve.

### Full-text search with the `unicode61` tokenizer

Every FTS5 table in `apollia-memory` is declared with `tokenize='unicode61'`. This
tokenizer strips diacritics before indexing and querying, so "reunion" matches
"réunion" and "societe" matches "société", which is required for the French target
documents agents handle. It ships natively inside the bundled SQLite (no external
dependency) and BM25 scoring stays reliable. The trade-off is no stemming: searching
"générer" does not match "génération".

### Conversation memory: sliding window plus summary

Chat conversations grow unbounded and eventually overflow the LLM context window. We
keep a sliding window of the last N messages (default 20) and an LLM-generated summary
of the messages that fell out of the window. The summary is stored in
`chat_sessions.summary` and is recomputed only when the window slides, not on every
message. The full context for each LLM call is: system prompt, user profile block,
summary, last N messages, current message. This bounds context size at the cost of one
extra LLM call per window shift (amortized roughly every 20 messages).

### Episodic memory: append-only with TTL, consolidation deferred

Episodic memory stays an append-only log with explicit per-entry TTL. Expired entries
are purged at startup (`auto_purge = true`, default), and the CLI exposes
`memory purge <namespace>` and `memory clear <namespace>` for manual control. To bound
growth without heuristics, the memorized output of a step is truncated to
`DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS = 200` characters (configurable via
`ORIAConfig::step_memory_max_chars`), suffixed with an `…` ellipsis.

Automatic consolidation (LLM summarization or merging of old episodes) is deferred. It
would introduce uncontrolled LLM cost, unpredictable behavior, and destructive data
loss, and it would violate Principle #6. The agent can already synthesize its own
history through explicit `ctx.memory.record()` calls.

### Export and import: a single JSON document

Memory export and import use a single pretty-printed JSON object (the `MemoryExport`
struct), written to a `<name>.apollia-memory` file. The object carries a
`format_version` header field alongside the entries; the CLI serializes it with
`serde_json::to_string_pretty` and writes it with `fs::write`. No line-delimited
framing, no gzip, no `flate2`. A `format_version` newer than the running install fails
fast with an explicit error rather than corrupting the database. The CLI exposes
`memory export` (whole store or a single namespace) and `memory import` (merge by
default via `INSERT OR IGNORE`, deduplicating by entry `id`, or `--replace` with
confirmation). Implemented in `crates/apollia-memory/src/export.rs`.

### Workspace context assembly and project-scoped namespaces

The `apollia-workspace` crate assembles situational context, distinct from memory. The
`ProjectRuntime` orchestrator aggregates a set of `WorkspaceProvider` implementations,
each wrapped in a per-provider 2s timeout, behind a 30s cache TTL:

- `GitProvider`: branch, HEAD, modified files via a `git` subprocess. On a repo
  without git it returns `None` (fail-silent). The `git2` crate was rejected because it
  pulls in a dynamic `libgit2` C dependency, incompatible with Principle #2.
- `RulesProvider`: locates `APOLLIA.md` walking from CWD upward, bounded by
  `search_depth` (default 5 parent levels). First match wins, `None` otherwise.
- `TreeProvider`: a directory tree bounded by `max_lines` (default 100), excluding
  `.git`, `node_modules`, `target`, `__pycache__`, `.DS_Store`, `.next`, `dist`.
- `StyleProvider` and `ScriptProvider`: the remaining situational providers.

The assembled context is exposed to the agent as `ctx.workspace`. It is session-scoped
and ephemeral; it is never memory.

Memory is project-scoped by namespace prefixing. The `project_id` (already present on
`ChatSession`) is propagated to the memory layer and prefixed onto the manifest
namespace:

```
effective_namespace = "{project_id}:{manifest.memory_namespace}"  when project_id is set
effective_namespace = manifest.memory_namespace                   otherwise
```

The agent declares a plain namespace in its manifest; the runtime applies the prefix
transparently. This isolates the same agent running across two projects so it cannot
recall rules from the wrong one. Implemented in `crates/apollia-aip/src/context.rs`.

## Alternatives considered

### Automatic memory injection (rejected)
- Pros: agents look "smarter" with no effort.
- Cons: unpredictable cost, context noise, loss of agent control, hard to debug.

### ICU tokenizer for search (rejected)
- Pros: full Unicode handling, per-language stemming.
- Cons: external `libicu` dependency, complex build, over-engineered for the target.

### Automatic episodic consolidation (rejected)
- Pros: bounded database, "intelligent" memory.
- Cons: uncontrolled LLM cost, unpredictable and irreproducible behavior, destructive
  loss of original episodes, violates Principle #6.

### `git2` crate for workspace context (rejected)
- Pros: native git access.
- Cons: dynamic `libgit2` C dependency, longer build, larger binary, all avoidable with
  a `git` subprocess.

### Chosen: explicit memory plus bounded context assembly
- Pros: deterministic and debuggable behavior, zero hidden LLM calls, native FTS5
  diacritic folding, bounded conversation and episodic growth, portable JSON backups,
  per-project memory isolation, cheap fail-silent context.
- Trade-offs: agents must call memory APIs explicitly; no automatic consolidation or
  stemming.

## Consequences

- Positive: agent behavior is fully deterministic and auditable; memory stays local and
  sovereign; conversation context is bounded; backups are portable and tool-readable
  (a plain JSON document, readable with `jq`); projects are isolated.
- Negative / trade-off: episodic memory grows linearly for long-running agents; agents
  must manage their own history explicitly; exports are unencrypted (the user owns file
  security).
- Watch: measure episodic database size after sustained use and reconsider the priority
  of consolidation; FTS5 index reconstruction on import can take a few seconds on large
  stores; orphaned namespaces remain if a project is deleted without an explicit purge.

## Architectural principles

- Principle #6 (memory at agent initiative): the foundation of this ADR. No automatic
  injection, no automatic consolidation. Context is distinct from memory and is the only
  thing the runtime assembles, always bounded and explicit.
- Principle #2 (zero external dependency): `unicode61` is native SQLite; workspace git
  context uses a subprocess, not `libgit2`.
- Principle #1 (local-first): all memory, search, and exports stay on the local machine.
- Principle #5 (one actor, one responsibility): each workspace provider has a single
  responsibility.

## Related

- [ADR-005](ADR-005-oria-execution-model.md) the ORIA execution model that hosts the
  ContextManager and the conversation summarization step.
- [ADR-011](ADR-011-user-profile.md) the canonical user profile, stored in the
  `__user__` memory namespace and injected non-deterministically.
