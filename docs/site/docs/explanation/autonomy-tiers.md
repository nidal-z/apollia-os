---
sidebar_position: 4
title: Autonomy tiers
---

# Autonomy tiers

How much an agent may do on its own is not a fixed property of the agent. It is a
dial the operator sets, called the autonomy tier. The same agent can run
cautiously, asking before anything consequential, or freely, acting without
interruption, depending only on the tier you put it in. This page explains the
four tiers, what the tier actually changes, and how to choose one. For the
safeguards and the step budget that bound every run regardless of tier, see the
[accountability model](/explanation/accountability-model); this page is the
companion that explains the dial, not the guardrails.

## The dial, not the agent

Separating "what the agent can do" from "how far it may go unattended" is the key
idea. An agent declares the tools it needs; the tier decides how much human
oversight sits between the agent's intent and its actions. That separation is
what lets you deploy the same agent conservatively in a sensitive context and
loosely in a sandbox, without rewriting it. The tier reflects the trust you
extend for a given task, and trust is a decision about context, not code.

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
and Long autonomous. Moving up the tiers trades interruption for momentum.

## What the tier actually changes

Two things move with the tier. First, the plan gate: in the two lower tiers a
consequential plan pauses for human approval; in the two higher tiers it does
not. Second, self-checking: the runtime's verification and critic pass is dark in
Assisted and comes into play from Supervised upward, so a completed run can be
checked before its result is accepted. The mechanics of that verification loop
belong to the [plan and orchestrated execution model](/explanation/the-plan-model),
and the safeguards it runs under belong to the
[accountability model](/explanation/accountability-model). What matters here is
that the tier is the single dial that governs both.

Note what the tier does not change: the step budget. The ceiling on reasoning
steps, tool calls, and wall-clock time is enforced by the runtime on every tier,
including the most autonomous one. Raising autonomy widens what an agent may
attempt; it never removes the hard edge.

## Choosing a tier

Choose by the cost of a wrong action and the trust the task has earned. Start new
or sensitive work in Assisted, where you see the plan first. Move to Supervised
when you want the engine's verification without giving up the gate. Reach for the
autonomous tiers only when the work is well scoped, the permissions are tight, and
the interruptions of a lower tier would cost more than they protect. The tier is
easy to change, so treat it as a per-run judgment, not a permanent setting. For
where the tier is configured, see the
[Configuration reference](/reference/configuration).

## Related

- [The accountability model](/explanation/accountability-model)
- [The plan and orchestrated execution model](/explanation/the-plan-model)
- [Configuration reference](/reference/configuration)
