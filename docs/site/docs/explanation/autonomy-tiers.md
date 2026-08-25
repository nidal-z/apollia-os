---
sidebar_position: 4
title: Autonomy tiers
---

# Autonomy tiers

How far an agent runs on its own is not a fixed property of the agent. It is the
autonomy tier, chosen for a run. The same agent can pause on its plan and wait
for you, or run its plan through without interruption, depending on the tier you
put it in. This page explains the four tiers, what the tier actually changes, and
how to choose one. For the safeguards and the step budget that bound every run
regardless of tier, see the
[accountability model](/explanation/accountability-model); this page is the
companion that explains the tier, not the guardrails.

## The tier, not the agent

Separating "what the agent can do" from "how far it may go unattended" is the key
idea, and the tier only ever answers the second half. An agent declares the tools
it needs; the tier decides whether its plan waits for approval and whether its
result is verified afterwards. It decides nothing about permissions: no tier
grants a tool, refuses one, or adds a human checkpoint to a tool call. That
separation is what lets you deploy the same agent conservatively in a sensitive
context and loosely on an isolated test machine, without rewriting it.

## The four tiers

Apollia defines four tiers, from most supervised to most autonomous:

- **Assisted.** The default. The gate is active: a proposed plan and
  consequential actions wait for a human decision before they proceed. This is
  the tier for unfamiliar agents, sensitive data, or any run where you want to
  see the plan before it executes.
- **Supervised.** The gate is still active, so a human stays in the loop on
  consequential steps, but this is the tier from which the runtime's own
  verification pass turns on (see below). Use it when you want oversight plus the
  engine's self-checking.
- **Bounded autonomous.** The gate is bypassed: the agent acts without pausing
  for plan approval, within the runtime's non-bypassable budget. Use it for
  trusted, well-scoped work where interruption would cost more than it protects.
- **Long autonomous.** Also gate-bypassed, intended for longer unattended runs.
  This is the widest tier, appropriate only when the task is well understood and
  the budget and permissions already constrain the blast radius.

The gate is active in Assisted and Supervised, and bypassed in Bounded autonomous
and Long autonomous, but only for a run that carries no decision of its own.
`apollia-os run` always carries one: `--plan` arms the gate for that run and its
absence disarms it, whichever tier the run is in. Moving up the tiers trades
interruption for momentum.

## What the tier actually changes

<!-- claim:plan-gate-yields-to-the-per-run-override -->
Five things move with the tier, and no others. The plan gate: in the two lower
tiers a consequential plan pauses for human approval, in the two higher tiers it
does not, and in both cases a run that carries its own decision overrides the
tier. Self-checking: the runtime's verification and critic pass is dark in
Assisted and comes into play from Supervised upward, so a completed run can be
checked before its result is accepted. Memory injection: the highest tier alone
appends a user-persona brief, and only inside the built-in assistant. The
system-prompt profile: Assisted takes one built-in prompt, the three other tiers
take a more persistent one. And the suggested step budget, which is the one worth
reading twice.

<!-- claim:tier-sets-budget-runtime-ceiling-caps-it -->
The four tiers declare 100, 200, 300 and 500 reasoning steps, and that table is
real. What reads it is the chat path, and free chat runs at the default tier and
never varies.

<!-- claim:tier-budget-capped-at-thirty-on-agent-paths -->
An agent run does not read that table at all. Both execution paths take the
budget the agent's manifest declares and cap it against a fixed runtime ceiling
of 30 reasoning steps, 60 tool calls and 600 seconds of wall clock. So raising
the tier widens what an agent may attempt well before it changes what the agent
actually gets, and it never removes the hard edge. The mechanics of the
verification loop belong to the
[plan and orchestrated execution model](/explanation/the-plan-model), and the
safeguards it runs under belong to the
[accountability model](/explanation/accountability-model).

## Choosing a tier

Choose by the cost of a wrong action and the trust the task has earned. Start new
or sensitive work in Assisted, where you see the plan first. Move to Supervised
when you want the engine's verification without giving up the gate. Reach for the
autonomous tiers only when the work is well scoped, the permissions are tight, and
the interruptions of a lower tier would cost more than they protect. The tier is
easy to change, so treat it as a per-run judgment, not a permanent setting.

The tier is set for a run, with `--autonomy` on `apollia-os run`. There is no
`[autonomy]` section in `apollia.toml` that the runtime reads, and the desktop
offers no tier control.

## Related

- [The accountability model](/explanation/accountability-model)
- [The plan and orchestrated execution model](/explanation/the-plan-model)
- [Choose an autonomy level](/operator-help/agents/choose-an-autonomy-level)
