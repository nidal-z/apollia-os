# ADR-048: Code executors are never blanket-authorized

- Status: Accepted
- Date: 2026-07-20

## Context

A pre-launch security review found that a single "always allow" click granted an
arbitrary blank check over an entire shell interpreter. The desktop "always
allow" button persisted a permission rule with no argument prefix, and the
prefix engine treated a no-prefix rule as matching any argument, so one click
auto-approved every future `bash_executor` invocation. Worse, even a scoped
prefix rule was escapable by chaining: a rule keyed on `git` matched
`git status; rm -rf ...` because matching was a raw `starts_with` on the whole
command string.

Two consumption paths shared the weakness. The agent path evaluates each call
through `PermissionEngine::decide()` and the SQLite prefix engine. The chat path
never calls the engine; it seeds persisted allow rules into a name-only
pre-authorization set and inserts the tool into the session's authorized set,
both of which skip the human-in-the-loop prompt entirely with no argument
granularity.

`bash_executor` and `python_executor` are unlike ordinary tools: their single
argument is an unparsed, arbitrary-code payload (a shell line, Python source).
A grant scoped only by tool name is therefore a blank check over a whole
interpreter, not an approval of a reviewed action. The permission model had no
notion of this class of tool.

## Decision

We treat arbitrary-code executors as a distinct class,
`apollia_permissions::CODE_EXECUTOR_TOOLS` (`bash_executor`, `python_executor`),
and enforce one invariant across every layer: a code executor is never
blanket-authorized by name. A no-prefix allow rule never auto-approves one, and
a prefix rule on one matches only a single simple command (no chaining, pipe,
redirection, substitution, or backgrounding). "Always allow" is downgraded to a
per-invocation approval for these tools: the current call still runs once, the
next one asks again.

The invariant is enforced at the matching layer (`prefix_rule_engine`, so the
agent path and any engine consumer are covered), at rule persistence and
pre-authorization seeding (`chat/manager/libre.rs`), and at the in-session grant
(`chat/manager/exchange.rs`). A consumption-side filter in
`merge_live_authorized_tools` neutralizes any legacy rule already stored.

## Alternatives considered

### Full argv parsing of the executor payload (rejected for now)
- Pros: the ideal model. Scope an approval to a parsed command and its real
  arguments rather than a raw string prefix; word-boundary matching removes the
  `git` matches `gitleaks` quirk.
- Cons: a large, shell-grammar-dependent change across the permission surface,
  out of proportion for a frozen-release security fix. Left as future work.

### Enable default RiskClassifier patterns (rejected)
- Pros: an always-on substring net over risky commands.
- Cons: the classifier is a case-sensitive `contains` matcher, trivially bypassed
  (double spaces, quoting, variable expansion). Enabling it by default would give
  false confidence, broaden behavior during a quality freeze, and break the
  opt-in, local-first posture. It stays a defense-in-depth net, not the primary
  control.

### Chosen: a code-executor class with a no-blanket / simple-command invariant
- Pros: closes both consumption paths with a small, well-scoped change; keeps
  legitimate prefix rules and exact-match SafeList entries working; keeps
  autonomy tiers unchanged.
- Trade-offs: "always allow" no longer sticks for shell and Python tools, so an
  operator who ran them repeatedly now confirms each call or configures a scoped
  prefix rule.

## Consequences

- Positive: a single click can no longer hand an agent an arbitrary shell. The
  security boundary holds even when the injection detector does not fire.
- Negative / trade-off: repeated shell or Python calls need per-invocation
  approval unless a scoped simple-command prefix rule is configured.
- Watch: prefix matching is still a raw prefix, not tokenized, so `git` matches
  `gitleaks`; the SafeList exact-match is widened to a prefix once migrated into
  `governance.db`. Both are pre-existing and tracked for the argv-parsing work.

## Architectural principles

- Principle #7 (non-negotiable safeguards): the human-in-the-loop gate for an
  arbitrary-code executor cannot be turned into a standing blanket grant.
- Principle #1 (local-first): the RiskClassifier stays opt-in rather than
  shipping default block patterns.

## Related

- [ADR-015](ADR-015-permission-tool-governance.md) permission and tool governance model this hardens
