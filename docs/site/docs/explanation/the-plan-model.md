---
sidebar_position: 5
title: The plan and orchestrated execution model
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

Every request is first classified into one of two execution modes. **Direct** is
single-step execution for simple work. **Orchestrated** is multi-step execution
with planning, for work that needs several tool calls driven by reasoning.

The classification is deterministic. It scores manifest features (the declared
step budget, tags, the number of tools, input length, planning-oriented keywords)
and compares the score against a threshold. An agent's manifest can also set the
mode explicitly, which overrides the heuristic. There is no LLM call in this
decision; classification is a pure function of what the agent and the request
declare, which keeps it predictable.

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
The graph is what would make parallelism possible. The engine walks the plan in
topological levels, and steps in the same level may run concurrently when they
are read-only tool calls needing no approval.

**In the shipped runtime they never do.** Deciding that a step is read-only is
delegated to the tool proxy, and the one production implementation keeps the
trait default, which answers no for every tool. Every step therefore runs
sequentially. The levels still matter, they order the plan and express what
depends on what, but they buy no speed today. Treat the plan as a dependency
graph, not as a scheduler.

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
them with a hybrid contract (ADR-038). At plan time, the reasoner fills structured
step arguments under a grammar constraint, so the plan already carries typed,
schema-valid arguments. If a step reaches execution without valid arguments, a
schema-constrained just-in-time extraction fills them from the step's description.
This is what lets the orchestrated path drive real native tools with real
structured arguments, rather than passing a blob of text and hoping.

## Verifying and correcting the result

A completed orchestrated run is not accepted on faith. The engine runs a
verification pass (ADR-039): an LLM critic reviews the result and produces a
verdict, and that verdict is recorded as a signed event in the audit journal. On
a failing verdict, the engine re-plans and re-runs, bounded by a small number of
attempts (the default is two) and drawing on the same shared budget, so
self-correction cannot escape the run's ceiling.

Two honest bounds. This verification pass is dark in the default Assisted tier and
becomes active from Supervised upward, so it is a property of the tier you choose
(see [Autonomy tiers](/explanation/autonomy-tiers)). And within that pass, the LLM
critic is the wired, working part; running an agent's own declared shell checks
under governance is not yet wired, that invoker is a no-op today. The self-check
is real; the deterministic shell checks are a follow-up. The audit side of this,
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
