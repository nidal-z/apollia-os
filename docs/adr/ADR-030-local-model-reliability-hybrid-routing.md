# ADR-030: Local-model reliability and hybrid routing

- Status: Accepted
- Date: 2026-06-05

## Context

Apollia targets small local models (7-14B) as the default backend (Principle #1,
local-first). Two reliability gaps surface at that size.

First, tool calling. The grammar layer can emit a GBNF grammar from the active tool
set and the runner can apply it at decode time, but the ReAct loop never attaches that
grammar to the request. Without a decode-time constraint, a small model regularly
produces malformed tool calls: wrong tool names, missing required arguments, broken
JSON. The invocation then fails and the loop wastes a step recovering. Frontier cloud
models do not need the constraint and their backends ignore the field.

Second, hard steps. Some steps exceed what a small local model can do well (deep
reasoning, repeated failures). An operator may want to spend a bounded amount of money
to escalate those specific steps to a frontier model, while keeping everything else
local and never exceeding a budget they set. There is today no configuration surface for
a frontier backend, no per-session cost ceiling, and no routing entry point that decides
when to escalate.

Both decisions must be taken now: the grammar plumbing exists but is inert, and the
hybrid routing config plus policy are the foundation the agent loop will build on next.

## Decision

We adopt two linked mechanisms.

1. We wire GBNF into the ReAct loop. `ToolCallHelper::run_tools` generates a grammar
   from the tool set once per invocation and attaches it to every `CompletionRequest`,
   but only when the backend is local and tools are present. Local detection is a new
   defaulted trait method `CompletionModel::is_local() -> bool` (default `false`,
   overridden to `true` by the runner-backed backend). Cloud backends keep the field
   `None` and are unchanged.

2. We add optional hybrid frontier routing. A new optional config section declares a
   `frontier` backend and a per-session `cost_ceiling_usd`
   (`HybridRoutingConfig`, validated at startup). The router gains
   `route_with_escalation(signal, level)`: it returns the frontier backend only when an
   escalation signal is present and the session cost is strictly under the ceiling, and
   otherwise degrades to the local backend without ever erroring at runtime. The
   escalation signal is a typed enum (`EscalationSignal`) so later waves can enrich the
   reasons without changing the signature.

## Alternatives considered

### Detect local backend by name string (rejected)
- Pros: no trait change.
- Cons: couples the loop to a naming convention in `apollia.toml` ("local", "llama");
  fragile, breaks silently when an operator renames a backend.

### Boolean escalation flag instead of a typed signal (rejected)
- Pros: simpler call site today.
- Cons: a boolean cannot carry the reason (repeated failure, autonomy tier, future
  confidence score); enriching it later is a breaking signature change.

### Hard-fail when the frontier backend is misconfigured at runtime (rejected)
- Pros: surfaces the misconfiguration loudly.
- Cons: violates local-first. A missing or budget-exceeded frontier must degrade to
  local, not crash a running session. Misconfiguration is instead caught at startup.

### Chosen: defaulted `is_local()` trait method + typed `EscalationSignal` + budget-gated router method
- Pros: backward compatible (the defaulted method leaves every existing impl untouched);
  no string coupling; the signal is extensible; the cost ceiling is consulted on every
  escalation and cannot be bypassed; degradation is always local.
- Trade-offs: one extra method on the core trait; the router carries a third routing
  entry point alongside `route_precise` and `route_fast`.

## Consequences

- Positive: a local backend with tools can no longer emit an invalid tool call; cloud
  behavior is byte-for-byte unchanged.
- Positive: operators get opt-in frontier intelligence on hard steps under a hard budget.
- Negative / trade-off: the grammar is generated once per `run_tools` call even when the
  tool set is tiny; negligible cost, but it is work the cloud path skips.
- Watch: the escalation signal itself is derived by the agent loop in a later wave;
  until then `route_with_escalation` is exercised with an injected signal. The frontier
  backend name is validated at startup against the backend map, the same way `precise`
  and `fast` are.

## Architectural principles

- Principle #1 (local-first): the grammar constraint and the escalation both keep local
  as the default; frontier is opt-in by explicit config, and degradation is always local.
- Principle #4 (fail fast): an incomplete hybrid config (empty frontier, non-positive
  ceiling) is rejected at startup, and a frontier name absent from the backend map is a
  typed construction error, not a runtime crash.
- Principle #7 (non-negotiable safeguards): the per-session cost ceiling is read before
  every escalation decision and is not bypassable.

## Related

- [ADR-025](ADR-025-worker-agents-a2a-routing.md) domain expertise in agents for small
  local models, the same reliability concern this ADR extends to decode-time constraints.
