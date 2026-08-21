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

<!-- claim:rollback-journal-has-no-writer -->
Autonomy is only delegable if it is accountable. Every governed action is
recorded in a signed, hash-chained journal, and the record can be verified for
integrity. Undoing what an agent wrote is not part of it: the journal format and
the replay logic exist in the codebase, but nothing installs the journal on the
tools that write files, so nothing is recorded and no change can be undone. This
is the runtime's answer to "what did the agent do, and can I trust the record."

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

<!-- claim:executor-guard-blocks-command-chaining -->
Before any action runs, the chat dispatch path in `apollia-runtime` classifies
it in three steps. The first gate is a **tool-name authorization set**:
persisted name-only allow rules seed it, and code executors are excluded from
it on every route. On a miss, the loop consults the **prefix rules** per
invocation: the call's argument is matched against the operator's standing
rules, longest prefix first, and for a code executor the match additionally
goes through the guard (`is_single_simple_command`) that refuses a shell
command that chains, pipes, redirects or substitutes, so an approval granted
for one command cannot smuggle a second. Anything left raises a
**human-in-the-loop approval** the operator resolves, and that decision is
recorded. What this guard screens is **shell** injection, not prompt injection:
Apollia ships no prompt-injection defence.

Permissions are scoped to the whole install, a project, or a single session.
Separately, an autonomy tier is chosen for a run: it governs the plan gate and
the post-run verification pass, and it governs no permission rule and no human
checkpoint on a tool call. A tier never widens what an agent is allowed to
touch, only how far a run goes before it stops to ask.

## Memory at agent initiative

<!-- claim:memory-injection-confined-to-builtin-assistant -->

The runtime never injects memory context into an agent's prompt automatically. An
agent recalls when it decides to, through `ctx.memory`. This keeps context
assembly explicit and auditable rather than a hidden side effect, and it is a
deliberate principle, not an omission.

Three paths are outside that rule: a user-persona brief at the `long_autonomous`
tier, past session summaries on the first message of a free chat session, and the
Work section of the user profile when the operator clicks "Improve prompt" in a
chat composer. The first two live in the built-in conversational assistant, in
its own prompt builder and chat manager. The third lives outside the assistant,
in the desktop prompt-rewrite command, which builds its own one-shot prompt and
returns text to the composer rather than to a run. No agent execution path
reaches any of the three, which is why the principle holds where it is stated.

<!-- claim:rewrite-injects-work-context -->
The third one is worth naming precisely, because it is the newest and the least
obvious: the rewrite request carries the operator's Work section only when the
operator triggers it, and its output lands in the composer, where it can still
be edited or discarded before anything is sent. See
[the eight principles](/explanation/the-8-principles).

## Observability

The runtime emits structured events on an EventBus with typed fields, not
free-form log strings. The audit journal subscribes to that bus, which is how
accountability and observability share one event stream. Tracing uses structured
fields throughout, so a run is inspectable after the fact.
