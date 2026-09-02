---
sidebar_position: 2
title: The accountability model
description: "Who did what, and on whose authority: how Apollia records an agent's actions so a run can be replayed and answered for after the fact."
---

# The accountability model

Autonomous agents are only usable in serious settings if you can answer two
questions after the fact: what did the agent do, and can you trust that record.
Apollia's accountability model exists to answer both, and to keep a human in
control while the agent runs. This page explains how the
pieces fit together and what they are, and are not, meant to provide.

## The problem

An agent that reasons and acts on its own is powerful and, without controls,
opaque and irreversible. In regulated settings that is disqualifying. The answer
is not to make the agent less autonomous but to wrap its autonomy in governance:
bound what it can do, record everything it does in a way that cannot be quietly
altered, and keep a person in the loop on consequential actions. Those controls are built into the runtime rather than left to each
agent to implement, so an agent author cannot forget them and an operator cannot
be surprised by their absence.

## The building blocks

### A signed, tamper-evident trail

Every governed action an agent takes is written to an append-only journal. The
journal is a hash chain, and it is signed as it grows, so any later alteration
breaks the chain and is detectable. Each entry is linked twice: to the previous
entry of its own run, and to the previous entry of any run, so the journal is one
continuous sequence across every run, not a set of independent per-run logs. This
is what turns "the agent says it did X" into "here is the recorded, verifiable
sequence of what happened." The trail captures tool calls and, in the run
journal, the model's own completions, so the reasoning behind an action is
inspectable, not just its effect.

### Verification of that trail

A record is only as good as your ability to trust it. Verifying the journal
checks its hash chains and signatures and tells you whether the sequence has been
altered since it was written. Because entries are chained across all runs,
verification detects not only a mutated entry but also a run whose tail was
truncated or a whole run that was deleted: either leaves a gap the chain exposes.
Accountability rests on a record you can independently confirm, not on trusting
the process that produced it.

The honest scope matters. This is tamper-evidence, not tamper-proofing: it
detects alteration after the fact, it does not prevent it. The guarantee holds as
long as the signing key is uncompromised. A party who holds the key can recompute
and re-sign a shorter, consistent chain, so the runtime also exposes the head of
the chain as an anchor you can export and store off-machine. Comparing a run
against an externally held anchor is what defends against truncation of the most
recent activity even when the key itself is at risk.

For the commands behind these two, see
[Audit and verify a run](/how-to/audit-and-verify).

Undoing what an agent wrote is deliberately absent. A reversible journal exists
in the codebase, but nothing installs it on the tools that write files, so no
`v0.1.0-preview` install records anything to undo. Shipping the command anyway
would have been worse than shipping nothing: an empty result is indistinguishable
from a clean session, so an operator would read "nothing to revert" as a working
safety net and delegate accordingly. Treat every filesystem change an agent makes
as final, and give it a sandbox root you are willing to lose.

### Permissions and human oversight

<!-- claim:hitl-wired-in-chat-path-only -->
Before a tool call runs in a chat session, persisted permission rules classify
it, a guard refuses a shell command that chains or redirects, and anything left
raises an approval request that an operator resolves. That decision is itself
recorded. Permissions are scoped, so authority can be granted at the level of the
whole install, a project, or a single session.

One boundary worth stating plainly. The approval wrapper is placed on the
**chat** dispatcher only: the tool calls an
installed Python agent makes through `ctx.tools` meet no human checkpoint. That
is a deliberate position rather than an oversight, and the
[agent trust model](/explanation/agent-trust-model) explains why: an installed
agent already executes arbitrary Python under your account, so a gate on one call
path would not contain a hostile one.

### Autonomy tiers

An autonomy tier moves five things and no others: the suggested step budget,
memory injection, the post-run verification pass, the system-prompt profile, and
the plan gate. It moves no permission rule and no human checkpoint on a tool
call, so a higher tier does not widen what an agent is allowed to touch. It
widens how far a run goes before it stops to ask.

<!-- claim:plan-gate-yields-to-the-per-run-override -->
The plan gate is the conditional one. The tier decides it only when the run
carries no per-run override, and `apollia-os run` always sends one, so on that
path `--plan` decides and the tier does not. The tier itself is set per run,
through `--autonomy`: `apollia.toml` has no `[autonomy]` section the runtime
reads, and the desktop offers no tier control. Free chat does not vary it
either, every exchange running at the default tier.

### Non-negotiable safeguards

The runtime enforces a step budget on every autonomous run: a ceiling on
reasoning steps, on tool calls, and on wall-clock time. It is enforced by the
runtime itself and cannot be bypassed by an agent, so a run cannot loop or spend
without bound. This is the guarantee that autonomy has a hard edge.

### Shell command screening

<!-- claim:risk-classifier-has-no-patterns-by-default -->
Shell commands are screened before execution: a syntax check rejects what will
not parse, and a pattern filter can refuse a command outright. That filter ships
with empty pattern lists, and nothing fills them: the constructor that takes
patterns has no production caller, and no `apollia.toml` section reaches it, so
the filter blocks no command in the shipped runtime. When a standing prefix
rule is consulted for a code executor, a stricter guard refuses any command
that chains, pipes, redirects or substitutes, so an authorisation granted for
one command cannot smuggle a second; outside a matching rule, every code
executor invocation requires its own approval. The screening is recorded.

This screens **shell** injection. Apollia ships no defence against prompt
injection, and nothing here should be read as one.

### Self-checking on the orchestrated path

On the orchestrated execution path, a completed run can be verified by a critic
before its result is accepted, gated by the autonomy tier. The verdict is
emitted as a runtime event and lands in the signed journal, so the check is part
of the record. On a failing verdict the engine can re-plan and re-run within a
bounded number of attempts, under the same shared budget, which is how an
orchestrated agent corrects itself without escaping its ceiling.

One honest limit: this pass currently runs the LLM critic; running an agent's
own declared shell checks under governance is a later step, not yet wired. The
critic is active; the deterministic shell checks are still to come.

## How it maps to the EU AI Act

The controls above line up with obligations the EU AI Act places on high-risk AI
systems. Apollia **provides the technical primitives that support** these
requirements. It does not make you compliant and it certifies nothing:
compliance is a judgment made by your organisation and its auditors about your
whole system and process, not a property a runtime can grant.

With that framing, the mapping is direct:

| Requirement (theme) | Apollia primitive |
|---|---|
| Article 10, data provenance and quality | the signed, hash-chained audit trail plus verification, which records and lets you confirm what data and actions a run touched |
| Article 14, human oversight | persisted permission rules, the code-executor guard, human-in-the-loop approvals **on the chat path**, and autonomy tiers, which keep a person in control of consequential actions. An installed agent's own tool calls are outside this loop, see above |
| Article 16, documentation and traceability | the audit journal and run trace, which document what happened |

The value is that these are wired into the runtime and demonstrable today, not
promised. What remains a human responsibility is deciding whether your use of
them, in your context, meets the obligation. Apollia gives you the mechanism;
the compliance judgment stays with you.

## Why this is the point

Autonomy without accountability is a liability, and accountability bolted on
after the fact is not credible. Apollia's position is that the governance is part
of the runtime: bounded by budgets, recorded in a signed trail you can verify,
and supervised by permissions and tiers. That is what makes
delegating real work to an autonomous agent defensible.

## Related

- [Audit and verify a run](/how-to/audit-and-verify) for the
  hands-on workflow.
- [Embed Apollia via federation (MCP + REST)](/how-to/embed-via-federation) for
  how these controls travel into a host integration.
- The [CLI reference](/reference/cli) for the audit and permissions
  commands.
