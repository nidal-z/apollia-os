# ADR-013: Human-in-the-loop (HITL)

- Status: Accepted
- Date: 2026-06-04

> **Amended by [ADR-023](ADR-023-sdk-agentkit-design.md).** Passages below
> describing the agent contract as a `manifest()` method plus an async `run()`
> record the state at the time of writing. That contract was removed: the runtime
> calls no dynamic `manifest()`, and the bridge refuses an object without
> `__apollia_dispatch__`. The decision this ADR records is otherwise unchanged.

## Context

An agent must be able to suspend a task to ask for human approval before continuing.
Two interdependent design questions follow: how does the runtime hand the human
response (approved or rejected, plus a reason) back to the Python agent on resume, and
where does the agent declare which tools require approval before execution.

These choices touch the AIP contract and the public Python interface, and they are hard
to reverse once third-party agents adopt them. They are bound by the minimal-contract
principle (add as little as possible to `manifest()` + `run()`), by fail-fast (a
mis-implemented agent must fail explicitly, not silently), and by duck typing (no
mandatory base class).

A third, related surface is MCP sampling. The MCP `sampling` capability lets a server
invoke the client's LLM through `sampling/createMessage`, for example to make a
secondary call without its own API key. This is powerful for sub-agents but is also an
attack surface: a malicious or compromised server could spam the client with costly LLM
requests and leak data through prompts. The MCP specification recommends explicit
consent on both the prompt sent and the result returned. At present the protocol types
exist and the capability is advertised, but the request handler and the HITL gate are
not yet wired, so this ADR records the intended design rather than shipped behavior.

## Decision

We adopt resume-by-re-calling `agent.run()` with `is_resumed` plus an input response,
a `tools_requiring_approval` manifest declaration, and HITL pre-approval guarding MCP
sampling.

### Resume mechanism

`agent.run()` stays the single Python entry point. The contract lives on the Rust side
in `apollia-core`. Two fields are added to the `AIPTask` struct, and the response itself
is the `InputResponseData` struct:

```rust
pub struct InputResponseData {
    pub approved:     bool,
    pub reason:       Option<String>,
    pub context:      serde_json::Value,
    pub responded_at: String,
}

pub struct AIPTask {
    // existing fields
    pub is_resumed:     bool,                        // default false
    pub input_response: Option<InputResponseData>,
}
```

The agent suspends by returning `AIPResult.input_required(prompt, context)`. On resume,
`TaskRepository::rebuild_for_resume()` rebuilds the `AIPTask` with `is_resumed = true`
and a populated `input_response`, then ORIA re-calls `agent.run()`. The bridge reuses its
existing run path with no new execution branch. The Python agent receives the resumed
task as a plain JSON dict: it reads the `is_resumed` and `input_response` keys (the
latter carrying `approved`, `reason`, `context`, `responded_at`). The
`if task.is_resumed` pattern is idiomatic and explicit; an agent that ignores it produces
incorrect logic that is detectable at run time. The `context` field (the agent's
serialized state at suspension) persists in `task_approvals.context_json` and is
auditable.

### Declaring approval-gated tools

`tools_requiring_approval` is an optional manifest field (default empty list). It is a
field of the manifest JSON/TOML, parsed on the Rust side (`manifest.rs`), not a keyword
argument of the `@agent` decorator. The `@agent` decorator accepts `user_memory_write`
but not `tools_requiring_approval`:

```rust
pub struct AgentManifest {
    // existing fields
    #[serde(default)]
    pub tools_requiring_approval: Vec<String>,
}
```

In orchestrated execution, the actor loop checks this field before each step. If the
step's tool is listed, the loop suspends and waits for human approval before executing
the tool. In direct execution the agent instead calls `AIPResult.input_required()`
explicitly; the two mechanisms are orthogonal.

### MCP sampling guarded by HITL pre-approval (design, not yet wired)

This section records the intended design. Today the protocol types
(`SamplingCreateMessageParams`, `SamplingCreateMessageResult` in
`crates/apollia-mcp/src/protocol.rs`) exist, but there is no handler for
`sampling/createMessage`, no HITL approval flow for sampling, and no per-server
rate limit. The handler and the HITL gate are future work.

Amended 2026-07-31: the client no longer advertises `sampling` or `elicitation`
during `initialize`. It did, which meant a compliant server was told to send
requests that nothing would answer. The capability is announced in the same
change that adds the handler, not before.

When wired, `sampling/createMessage` will route through the existing LLM router and be
guarded by mandatory HITL pre-approval:

1. The MCP handler receives `sampling/createMessage`.
2. An approval event is emitted to the desktop inbox with a full prompt preview and the
   source server identity.
3. The user sees the reused HITL approval card with Approve and Refuse actions.
4. On Approve, the LLM router executes and the result is returned to the server via the
   matching JSON-RPC response.
5. On Refuse or timeout, a `cancelled` error is returned to the server.

The design also calls for a per-server budget (a configurable cap on sampling calls per
window) so that, beyond it, a `rate_limited` error is returned without prompting the
user, preventing a malicious burst from saturating the human. Pre-approval (rather than
post-call approval) saves the LLM call on refusal and matches the specification's
explicit-consent recommendation. The protocol plumbing is meant to route through the
existing HITL pipeline once the handler lands.

## Alternatives considered

### New `on_resume(response, ctx)` hook (rejected)
- Pros: clean separation between first-call and resume logic.
- Cons: a fourth contract method, an ambiguous default when absent, and a duplicated
  execution path in the bridge.

### Response stored in memory, read via `ctx.memory` (rejected)
- Pros: no `AIPTask` change.
- Cons: automatic write into memory violates the spirit of Principle #6; couples HITL to
  the memory subsystem; the agent cannot tell a resume from a first call without reading
  memory.

### MCP sampling without HITL (rejected)
- Pros: zero UX friction.
- Cons: violates the MCP consent recommendation, lets a server drain the LLM, and gives
  the user no visibility.

### MCP sampling with post-call approval (rejected)
- Pros: the user sees the real exchanged content.
- Cons: too late to prevent the LLM cost, and confusing UX about what is being approved.

### Chosen: re-call `run()` with `is_resumed`, manifest-declared tools, pre-approved sampling
- Pros: a single Python entry point, a declarative manifest field, no memory coupling,
  reuse of the existing run path and approval card, and MCP-aligned explicit consent.
- Trade-offs: HITL agents must implement the `if task.is_resumed` pattern; an agent with
  no resume logic is silently incorrect if HITL fires; sampling adds one approval per
  call.

## Consequences

- Positive: the AIP contract grows only by optional `AIPTask` and manifest fields;
  approval-gated steps are inspectable before plan generation; the planned sampling flow
  reuses the existing approval card and is designed to be rate-limited against bursts.
- Negative / trade-off: `AIPTask` must serialize fully for faithful resume; the response
  `context` round-trips through JSON, so non-JSON-native Python types lose precision;
  once wired, heavy sampling can saturate the user.
- Watch: multi-approval tasks keep only the latest response in `input_response` (each
  suspension is still recorded individually); if sampling saturation is observed, add an
  opt-in session approval (auto-approve N calls for a server over T minutes).

## Architectural principles

- Principle #3 (minimal contract): `manifest()` + `run()` stay sufficient;
  `tools_requiring_approval`, `is_resumed`, and `input_response` are additive.
- Principle #4 (fail fast): a resume on a task not in `input_required` is rejected with
  `409 CONFLICT`; once sampling is wired, a refused or timed-out sampling fails explicitly.
- Principle #6 (memory at agent initiative): `input_response` travels in the AIP contract,
  not injected into memory.
- Principle #7 (non-negotiable safeguards): a timeout watcher cancels stale
  `input_required` tasks; the planned per-server sampling rate limit is designed to be
  unbypassable, enforced by the runtime once the handler lands.

## Related

- [ADR-006](ADR-006-tool-subsystem.md) the tool subsystem whose orchestrated execution
  intercepts approval-gated tools.
- [ADR-024](ADR-024-sdk-runtime-contract-ctx.md) the SDK runtime contract that exposes
  `is_resumed`, `input_response`, and the approval surface to the agent.
