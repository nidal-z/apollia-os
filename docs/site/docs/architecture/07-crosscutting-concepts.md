---
sidebar_position: 7
title: 7. Cross-cutting concepts
---

# 7. Cross-cutting concepts

These concepts do not belong to one crate; they run through the whole runtime.

## Sovereignty and local-first

<!-- claim:daemon-binds-tcp-by-default -->
The default path never leaves the machine. Inference can run fully local, on
a model the embedded engine serves, storage is local SQLite, and the API binds to a Unix socket and to
loopback TCP, which the daemon opens on every start. Loopback is what keeps the
default local: reaching the API from another machine takes a non-loopback bind
address, configured on purpose, and the daemon refuses one that carries no
bearer token. A remote model is opt-in, reached at an address the user
configures, and even then the local model stays the default.
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
the hard edge that stops a run from looping or spending without bound. The
ceiling of an agent run is fixed in the code, and no key of `apollia.toml`
changes it; a workflow run has bounds of its own, `[workflows] max_steps` and
`wall_clock_secs`.

An agent holds no code, so these runtime safeguards and the governance decision
on each of its tool calls are what hold the line around it. The Python a
package brings is another matter: it runs in-process as trusted code, with no
OS sandbox around it. The [agent trust model](/explanation/agent-trust-model)
explains that posture and its limits in full.

## Permissions and autonomy tiers

<!-- claim:executor-guard-blocks-command-chaining -->
Before a tool call of an agent runs, one decision in `apollia-runtime` answers
it, in a fixed order: a refusal of the organisation policy, then a fine rule
`deny`, then the family of the tool (a tool that only reads runs), then, for
anything sensitive, a standing consent of the agent for that family, or a
prompt a person answers with once, always or refuse. The answer is recorded in
the signed journal. A fine rule names a tool and an optional prefix of its
first argument, and for a code executor the match goes through the guard
(`is_single_simple_command`) that refuses a shell
command that chains, pipes, redirects or substitutes, so an approval granted
for one command cannot smuggle a second. What this guard screens is **shell**
injection, not prompt injection: Apollia ships no prompt-injection defence.
The [Runtime view](/architecture/runtime-view) gives the order line by line.

A rule is scoped to one agent or to every agent, and a standing consent to one
agent and one family of action; nothing is scoped to a project or to a session.
Separately, an autonomy tier applies to a run: it governs the plan gate and
the post-run verification pass, and it governs no permission rule and no human
checkpoint on a tool call. A tier never widens what an agent is allowed to
touch, only how far a run goes before it stops to ask.

## Memory at agent initiative

<!-- claim:memory-injection-confined-to-builtin-assistant -->

The runtime never injects memory context into an agent's prompt automatically,
and writes nothing to memory on its behalf. An agent recalls when it decides
to, with the tool `memory_search`, and keeps a note with `memory_write`.
Checking Memory on the sheet of the agent is the one thing that grants both
tools, and neither asks for consent. The memory of an agent is one space named
after it, kept across its conversations and its tasks. An agent whose Profile
source is checked may propose a fact for the profile with `profile_propose`,
and that call is put to the person every time. The Python of a package receives
no `ctx.memory`. This keeps context assembly explicit and auditable rather than
a hidden side effect, and it is a deliberate principle, not an omission.

Three paths are outside that rule: a brief of the profile of the conversation,
at every autonomy tier while the profile switch of that conversation is on, past
session summaries on the first message of a free chat session, and the role, the
function and the tools of that profile when the operator clicks "Improve prompt"
in a chat composer. The first two live in the built-in conversational assistant, in
its own prompt builder and chat manager. The third lives outside the assistant,
in the desktop prompt-rewrite command, which builds its own one-shot prompt and
returns text to the composer rather than to a run. No agent execution path
reaches any of the three, which is why the principle holds where it is stated.

<!-- claim:rewrite-injects-work-context -->
The third one is worth naming precisely, because it is the newest and the least
obvious: the rewrite request carries those three profile fields only when the
operator triggers it, and its output lands in the composer, where it can still
be edited or discarded before anything is sent. See
[the eight principles](/explanation/the-8-principles).

## Observability

The runtime emits structured events on an EventBus with typed fields, not
free-form log strings. The audit journal subscribes to that bus, which is how
accountability and observability share one event stream. Tracing uses structured
fields throughout, so a run is inspectable after the fact.
