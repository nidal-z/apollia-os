---
sidebar_position: 7
title: 7. Cross-cutting concepts
---

# 7. Cross-cutting concepts

These concepts do not belong to one crate; they run through the whole runtime.

## Sovereignty and local-first

The default path never leaves the machine. Inference can run fully local on a
GGUF model, storage is local SQLite, and the API binds to a Unix socket unless
TCP is explicitly enabled. Cloud inference is opt-in, on the user's own key, and
even then the local model stays the default with escalation under control.
Memory can be exported and imported by the user, so the data stays theirs. This
is not a feature bolted on; it is the constraint that shapes every default. See
[Constraints](/architecture/constraints).

## The accountability model

Autonomy is only delegable if it is accountable. Every governed action is
recorded in a signed, hash-chained journal; the record can be verified for
integrity; and filesystem changes in a chat session can be reversed. This is the
runtime's answer to "what did the agent do, can I trust the record, and can I
undo it."

This concept has its own page, which also maps the controls to the EU AI Act.
This section does not duplicate it: read
[the accountability model](/explanation/accountability-model).

## Non-negotiable safeguards

The runtime enforces a step budget on every autonomous run: a ceiling on
reasoning steps, on tool calls, and on wall-clock time. It is enforced by the
runtime, not by the agent, and cannot be bypassed. Direct and orchestrated runs
are both bounded, with a safe default rather than an unlimited budget. This is
the hard edge that stops a run from looping or spending without bound. One item
still to wire: reading the ceiling from `apollia.toml` at runtime, which has a
safe default in place today.

Agent code itself runs in-process as trusted code, so these runtime safeguards
and the human-approval gate, not an OS sandbox, are what hold the line around an
agent. The [agent trust model](/explanation/agent-trust-model) explains that
posture and its limits in full.

## Permissions and autonomy tiers

<!-- claim:permission-engine-not-wired -->
<!-- claim:executor-guard-blocks-command-chaining -->
Before any action runs, the chat dispatch path in `apollia-runtime` classifies
it, and it is worth being precise about what ships enabled. The live gate is a
**tool-name authorization set**: persisted name-only allow rules seed it, code
executors are excluded from it on every route, and anything not in it raises a
**human-in-the-loop approval** the operator resolves and that decision is
recorded. Two further mechanisms exist in `apollia-permissions` but are not
evaluated per invocation on that path today: the **prefix matcher**, which
would match a call's argument against the operator's standing prefix rules,
and the **code-executor guard** (`is_single_simple_command`), which would
refuse a shell command that chains, pipes, redirects or substitutes. Both are
reachable only through `PermissionEngine::decide`, which no shipped binary
wires, so a rule carrying an argument prefix is stored and displayed but
auto-approves nothing.

<!-- claim:injection-detector-is-shell-not-prompt -->
`apollia-permissions` also contains a `PermissionEngine` aggregating a safe-list
and a shell-injection detector. **It is not active in the shipped application.**
`ToolDispatcher` holds an `Option<PermissionEngine>` that no production caller
populates, so those two components never run. They are kept for an embedder that
opts in, and the crate says so in its own module documentation. Note also that
the detector screens **shell** injection, not prompt injection: Apollia ships no
prompt-injection defence.

Permissions are scoped to the whole install, a project, or a single session. On
top of that, an autonomy tier is a dial the operator sets for how much an agent
may do without asking. The same agent can run cautiously or freely depending on
the trust the operator extends.

## Memory at agent initiative

<!-- claim:memory-injection-confined-to-builtin-assistant -->

The runtime never injects memory context into an agent's prompt automatically. An
agent recalls when it decides to, through `ctx.memory`. This keeps context
assembly explicit and auditable rather than a hidden side effect, and it is a
deliberate principle, not an omission.

The built-in conversational assistant is outside that rule, in two ways: a
user-persona brief at the `long_autonomous` tier, and past session summaries on
the first message of a free chat session. Both live in the assistant's own prompt
builder and chat manager, so no agent execution path reaches either, which is why
the principle holds where it is stated. See
[the eight principles](/explanation/the-8-principles).

## Observability

The runtime emits structured events on an EventBus with typed fields, not
free-form log strings. The audit journal subscribes to that bus, which is how
accountability and observability share one event stream. Tracing uses structured
fields throughout, so a run is inspectable after the fact.
