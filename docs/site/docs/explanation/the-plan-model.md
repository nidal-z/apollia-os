---
sidebar_position: 5
title: The plan and orchestrated execution model
description: How the runtime decides to run a task, plan-first or direct, what a plan contains, and where a human approves it before anything executes.
---

# The plan and orchestrated execution model

When you hand a task to an autonomous agent, the runtime has to decide how to run
it, turn intent into concrete steps, execute them safely, and check the result.
This page explains that model: how a request is classified, what a plan is, how
the plan gate keeps a human in control, and how the engine verifies and corrects
itself. It explains the model rather than the procedure; for the hands-on version,
see [Run an orchestrated agent](/how-to/run-an-orchestrated-agent), and for the
runtime sequence diagrams, see the [Runtime view](/architecture/runtime-view).

## Direct versus orchestrated

An agent runs in one of two execution modes. **Direct**: the model answers and
calls tools as it goes. **Orchestrated**: a plan is built first and its steps
are executed, for work that needs several tool calls driven by reasoning.

What chooses is the planning setting of the agent: `planning`, under
`[agents.defaults]` in the manifest of its package, which the person may
override. `never` runs direct, `always` plans first, and `auto`, the value when
neither says anything, lets the runtime judge the request. That judgement is
deterministic, and there is no LLM call in it. A conversation in plan mode is
apart: its plan is approved in the conversation, and the setting does not apply.

<!-- claim:execution-mode-classification-weights -->

Under `auto`, a run plans when the request itself reads as several steps, which
is the case when it holds a planning cue (`plan`, `etape`, `step`, `sequence`,
`workflow`, `pipeline`), or when a score over the agent and the request reaches
a threshold. The score adds independent weights:

| Weight | Added when |
| --- | --- |
| 0.30 | the step budget of the run exceeds 15 |
| 0.20 | the request has more than 3 input parts |
| 0.20 | the agent is granted more than 4 tools |
| 0.10 | the input text exceeds 500 characters |
| 0.10 | the mission of the agent contains a planning cue |

The threshold is 0.40, fixed in the code: the `[oria]` section of `apollia.toml`
is withdrawn and nothing reads it. Two consequences follow from the numbers
rather than from the prose. A planning cue in the request reaches the threshold
alone. And no single weight of the table does: without a cue, planning needs at
least two signals to agree. The scoring function also holds a weight for a
`multi-step` tag and one for the depth of episodic memory; the run of an agent
supplies neither, so they never apply.

## The plan cache

<!-- claim:plan-cache-is-consulted-by-the-engine -->

Planning is the expensive part of an orchestrated run, so a plan is cached and
reused. The key is a SHA-256 digest over the agent's name and version, its sorted
tool list, and the request text normalized to lowercase with collapsed
whitespace. A different agent version, a different tool set, or materially
different wording all miss the cache; a reworded-but-equivalent request may hit
it.

<!-- claim:plan-cache-has-no-automatic-expiry -->

Cached plans **never expire on their own**. There is no background eviction and
no time-to-live: an entry stays until an operator removes it. Clearing is a
manual command, and `apollia-os plan cache evict` takes an age in days that
defaults to 7. That default is the origin of the belief that the cache expires
after a week. It does not; nothing runs that command for you.

That matters when an agent's behaviour changes without its version changing. The
cache will keep serving the plan built before the change until it is cleared. See
[Deploy in production](/how-to/deploy-in-production) for the commands.

The cache is the only part of a plan that outlives the daemon. Execution state
does not: a plan in progress is held in memory, so restarting the daemon ends the
run rather than resuming it. Cached plans survive because they sit in
`plan_cache.db`; the run that was using one does not.

One honest detail worth knowing. The unified execution entry point implements the
orchestrated branch; its direct branch is a stub, and real direct execution runs
through a separate entry point. In practice the orchestrated path is the one this
page describes end to end. The stub is documented in
[Risks and technical debt](/architecture/risks-and-technical-debt).

## The plan as an artifact

Orchestration produces a plan, and the plan is a real artifact, not a hidden
control flow. It is a directed acyclic graph of steps: each step can depend on
others, and those dependencies are the graph's edges. Because the plan is
explicit, it can be shown, approved, audited, and revised.

<!-- claim:orchestrated-parallelism-not-active -->
The graph is what makes parallelism possible. The engine walks the plan in
topological levels, and the steps of one level run concurrently when there are
at least two of them and each one calls a tool that only reads.

**Every other level runs its steps one after the other.** That covers a step
the model answers itself, a tool that writes, and a level of one step. Whether
a tool only reads is asked of the tool proxy of the run, which answers from the
descriptor of each tool granted to the agent. The levels order the plan and
express what depends on what; they buy speed only where the work is reading.

## The plan gate

For a consequential plan, the runtime can pause before executing and emit a
plan-approval request. A human then approves it, rejects it, or pauses to inject
guidance. A rejection triggers a bounded re-plan that takes the feedback into
account. Whether this gate is active is governed by the autonomy tier: it is on
in the lower tiers and bypassed in the higher ones, as described in
[Autonomy tiers](/explanation/autonomy-tiers). The runtime-view page shows this as
a sequence in its chat plan-mode scenario.

## How steps get their arguments

A plan step names a tool, but the tool needs concrete arguments. Apollia resolves
them with a hybrid contract (see [the execution model](/architecture/decisions#execution-model)). At plan time, the reasoner fills structured
step arguments under a grammar constraint, so the plan already carries typed,
schema-valid arguments. If a step reaches execution without valid arguments, a
schema-constrained just-in-time extraction fills them from the step's description.
This is what lets the orchestrated path drive real native tools with real
structured arguments, rather than passing a blob of text and hoping.

## Verifying and correcting the result

A completed orchestrated run is not accepted on faith. The engine runs a
verification pass: an LLM critic reviews the result and produces a
verdict, and that verdict is recorded as a signed event in the audit journal. On
a failing verdict, the engine re-plans and re-runs, bounded by a small number of
attempts (the default is two) and drawing on the same shared budget, so
self-correction cannot escape the run's ceiling.

Two honest bounds. This verification pass is dark in the default Assisted tier and
becomes active from Supervised upward, so it is a property of the tier you choose
(see [Autonomy tiers](/explanation/autonomy-tiers)). And within that pass, the LLM
critic is the wired, working part; no shell check is run on behalf of an
agent. The audit side of this,
the signed journal and the verdict event, is covered by the
[accountability model](/explanation/accountability-model) and is not repeated here.

## Where to go next

- To build and run an orchestrated agent, read
  [Run an orchestrated agent](/how-to/run-an-orchestrated-agent).
- To see the execution as sequence diagrams, read the
  [Runtime view](/architecture/runtime-view).
- For the engine's place among the crates, read
  [Building blocks](/architecture/building-blocks).
- For the decisions behind this model, read
  [Architecture decisions](/architecture/decisions).

## Related

- [Autonomy tiers](/explanation/autonomy-tiers)
- [The accountability model](/explanation/accountability-model)
- [Run an orchestrated agent](/how-to/run-an-orchestrated-agent)
- [Runtime view](/architecture/runtime-view)
