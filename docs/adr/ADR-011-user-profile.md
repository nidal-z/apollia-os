# ADR-011: Canonical user profile

- Status: Accepted
- Date: 2026-06-04

## Context

Chat sessions are isolated by default: the runtime knows nothing about the user across
sessions, so every conversation restarts from zero. Assistants of comparable ambition
keep a durable user context (name, role, preferences, constraints) and let the model
use it as it sees fit.

Apollia needs the same continuity, but with two firm constraints. First, the profile
must never be deterministic: the model receives it as information and decides what to
use (Principle #6). Second, the surface must stay small. A profile of a few dozen
fields does not justify a large API, multiple competing editing screens, or an
unconventional storage convention. The design must give one schema, one storage
location, one editing page, and one agent-facing API.

## Decision

We adopt a single canonical user profile, declared by a central Rust schema, exposed to
the Python SDK as `ctx.profile`, stored in the `__user__` memory namespace, edited from
one Settings page, and injected non-deterministically.

### Declarative central schema

A constant `PROFILE_SCHEMA: &[ProfileField]` in
`crates/apollia-memory/src/profile_schema.rs` lists the canonical fields, grouped into
four display sections (`Identity`, `Work`, `Preferences`, `Constraints`). Each field
carries a `sensitive: bool` flag that triggers a "re-run onboarding" warning in the UI.
The schema is the single source of truth for which keys make up the profile.

### Storage in the `__user__` namespace

The profile is stored in the `semantic_memories` table under the `__user__` namespace.
Keys are dotted and category-prefixed (`domain.sector`, `tech.stack`,
`preferences.language`, `agents.hitl`, `constraints.sovereignty`, ...). A leading
`user.` prefix is stripped on access. Internal onboarding state uses a `__` key prefix
that hides it automatically from the profile listing (`get_internal` / `set_internal`).

### SDK API: `ctx.profile`

The Python SDK exposes `ctx.profile` with the methods `.get(key)`, `.has(key)`,
`.all()`, `.schema_keys()`, `.set(key, value)`, `.update(dict)`, plus a `.writable`
flag. Reads use keyed access (`ctx.profile.get("user.name")`); writes are gated by the
manifest flag `user_memory_write: true`. Provenance is recorded as
`WrittenBy { Onboarding, User, Agent(name) }`.

### One editing surface

`Settings -> Profile` (`routes/settings/Profile.svelte`) is the only editing surface, a
form-based view driven by the schema. The Memory page keeps a namespace explorer for
debugging only.

### Non-deterministic injection

When the profile is consulted, its block is informative ("for reference, use as you see
fit"), never a runtime rule. The agent reads it explicitly when relevant, exactly like
any other memory recall. The model is free to ignore it.

## Alternatives considered

### Typed JSON profile blob (rejected)
- Pros: strict Rust typing, IDE auto-completion for agents.
- Cons: every new field requires a recompile with no agent-side extensibility; a heavier
  wrapper is needed for plain key access.

### Typed profile plus a separate free-notes area (rejected)
- Pros: separates canonical fields from free text, scalable.
- Cons: reintroduces two concepts (`ctx.profile.*` plus a notes namespace), more API and
  UI surface to maintain.

### Chosen: canonical profile with a central declarative schema
- Pros: one source of truth for profile keys (the Rust schema), one editing surface, a
  drastically smaller API, and a focused agent API (`ctx.profile.get("user.name")` reads
  better than a raw recall against the underlying memory namespace). Agents can still
  write free keys, visible under "other entries", but promotion to a canonical field is
  an explicit act.
- Trade-offs: adding a canonical field requires updating the Rust schema plus i18n
  labels and recompiling.

## Consequences

- Positive: a single source of truth for profile keys; a single editing page; a smaller,
  more expressive agent API; cross-session continuity that stays local.
- Negative / trade-off: a new canonical field needs a Rust recompile; there is no
  confidence score or validation badge in the UI (the SQL column remains, so a scoring
  workflow can be reintroduced later if a real need emerges).
- Watch: adoption of `ctx.profile.*` by new agents; growth of the schema (beyond roughly
  thirty fields, consider a declarative registration mechanism loaded at boot).

## Architectural principles

- Principle #6 (memory at agent initiative): preserved. No automatic injection.
  `ctx.profile` is consulted explicitly by the agent when needed.
- Principle #3 (minimal contract): reinforced. A typed, explicit API replaces a loose
  key convention.
- Principle #8 (human CLI, machine API): the UX is simplified to one form-based page,
  while the agent-facing API is more expressive.

## Related

- [ADR-010](ADR-010-memory-context-architecture.md) the memory subsystem that hosts the
  `__user__` namespace and its non-deterministic injection contract.
