---
sidebar_position: 1
title: The 8 principles
---

# The 8 principles

Apollia is a sovereign runtime for autonomous AI agents. It runs any Python agent
in isolation, locally, with tools, and without a cloud dependency. Eight
principles hold that promise together. They are not style preferences; each one
was forced by a real constraint, and each one shapes a default you can rely on as
an adopter. This page explains what each principle is and why it matters to you.
It does not restate them as rules; for the rule form and its enforcement, the
same eight appear as engineering [constraints](/architecture/constraints).

## 1. Local-first

No byte of user data leaves the machine without an explicit action. Inference can
run fully local on a GGUF model, storage is a local SQLite file, and there is no
telemetry and no phone-home. This exists because contractual guarantees were not
enough for the organisations Apollia targets: the answer was not to promise the
cloud would behave, it was to make the cloud technically unnecessary. For you, it
means the safe posture is the default, not a setting you have to remember to turn
on. The deeper treatment is in
[Sovereignty and local-first](/explanation/sovereignty-and-local-first).

## 2. Zero external dependency

The binary runs on a clean Linux machine with no prior install: no Docker, no
Node, no external database, no separate Python runtime. Every optional dependency
on an external service degrades gracefully rather than breaking the run. This
matters because operational complexity is a commercial veto for the teams
evaluating a sovereign runtime. One artifact to deploy is one attack surface to
reason about and one thing to keep running.

## 3. Minimal contract

<!-- claim:agent-contract-is-decorators-not-manifest-run -->
An agent is a class decorated with `@agent`, holding at least one async method
decorated with `@skill` or `@on_message`. There is no base class to inherit and
no framework to adopt. The point is to keep the surface an author must learn
small, so the runtime can carry the hard parts (governance, budgets, tools)
rather than pushing them into every agent. For the exact contract an agent sees,
read the [SDK reference](/reference/sdk).

The earlier contract, a `manifest()` method plus an async `run()`, is gone
(ADR-023). The bridge refuses any object without `__apollia_dispatch__`, the
attribute the decorators install, so an agent written the old way does not load
at all rather than half-working.

## 4. Fail fast

Any error detectable at startup is detected at startup, not three steps into a
run. A missing model, a malformed manifest, or an unreachable backend surfaces
before work begins. This keeps failures cheap and legible: you learn what is
wrong when you launch, not after the agent has spent part of its budget getting
into a broken state.

## 5. One actor, one responsibility

The runtime is built as Tokio actors that each own their state and talk over
message channels, with no shared locks between them. This is not an aesthetic
choice: shared mutable state across async tasks is where deadlocks and
unexplainable behaviour live. Keeping each actor responsible for one thing is
what makes the system's behaviour something you can reason about. The strategy
behind this is in [Solution strategy](/architecture/solution-strategy).

## 6. Memory at agent initiative

<!-- claim:memory-injection-confined-to-builtin-assistant -->
Apollia never injects memory into an agent's prompt. An agent recalls what it
decides to recall, when it decides to, through `ctx.memory`. Automatic memory
injection is convenient and quietly corrosive: it makes a run's inputs opaque and
its behaviour hard to attribute. Leaving recall to the agent's initiative keeps
the record of what fed a decision honest.

Two exceptions exist, both confined to the built-in conversational assistant, the
one behind the chat window. Neither is reachable from a Python agent you install:
they live in the assistant's own prompt builder and chat manager, and no agent
execution path goes through either.

The first is an operator's decision. At the highest autonomy tier only,
`long_autonomous`, the assistant appends a short user-persona brief to its system
prompt. The three lower tiers do not.

<!-- claim:cross-session-recall-injects-summaries -->
The second is not tier-gated, and is worth stating plainly. On the **first
message of a free chat session**, the runtime searches an index of past session
summaries with that message and appends up to three matches to the system prompt,
under a heading naming them as previous conversations. Messages shorter than 20
bytes skip it, so a greeting recalls nothing. Only the summaries are injected,
never past message content. Companion sessions are excluded outright: they must
not inherit personal history.

Read that second one for what it is. Inside the chat window, a new conversation
can start already carrying a trace of older ones, and the operator did not ask
for it per session. It buys continuity in a product surface where a user
reasonably expects to be remembered. It is also the one place in the runtime
where the "at agent initiative" rule genuinely does not hold, which is why it is
written here rather than left implicit.

The memory layer itself is exportable and importable, which is why it belongs to
sovereignty as much as to agency.

## 7. Non-negotiable safeguards

Every autonomous run is bounded by a step budget the runtime enforces: a ceiling
on reasoning steps, tool calls, and wall-clock time. An agent cannot raise or
remove it from its own code. Autonomy is only delegable if it has a hard edge, so
the edge lives in the runtime rather than in the agent's good intentions. This
page does not re-explain the mechanism; it is one of the pillars of the
[accountability model](/explanation/accountability-model). One honest detail: the
ceiling ships with a safe default, and reading a custom ceiling from `apollia.toml`
at runtime is a follow-up rather than a finished path. The full ledger of what is
partial is in [Risks and technical debt](/architecture/risks-and-technical-debt).

## 8. Human CLI, machine API

Every surface is dual: a human reads a terminal, a machine reads JSON. A global
`--json` flag and TTY detection mean the same command serves an operator at a
prompt and a host product driving the runtime through its API. This exists
because Apollia is meant to be embedded, not only used, and an embeddable runtime
has to speak both languages without one compromising the other. The command
surface is the [CLI reference](/reference/cli); the machine surface is the
[HTTP API reference](/reference/api/apollia-os-runtime-api).

## Why these eight, together

Taken singly, each principle is a reasonable engineering call. Taken together,
they are the value proposition: a runtime you can run without the cloud, deploy
without a stack, delegate to without losing control, and embed without
reverse-engineering. They are the reason autonomy here is something a regulated
team can actually adopt, not just admire.

## Related

- [Sovereignty and local-first](/explanation/sovereignty-and-local-first)
- [The accountability model](/explanation/accountability-model)
- [Constraints](/architecture/constraints)
- [Solution strategy](/architecture/solution-strategy)
