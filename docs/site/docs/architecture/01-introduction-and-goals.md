---
sidebar_position: 1
title: 1. Introduction and goals
---

# 1. Introduction and goals

## What Apollia is

Apollia OS is a sovereign runtime for autonomous AI agents. It runs any Python
agent (LangGraph, CrewAI, or a custom one) in isolation, locally, with tools,
and without a cloud dependency. An agent here is not a scripted LLM pipeline: it
is a process that reasons and acts on its own, in a ReAct loop, under governance
the runtime enforces.

It is built to be **embedded**. The primary consumer is a host product that
drives an Apollia instance to do agent work over its own data, without moving
that data into a cloud sandbox. The runtime exposes a stable machine contract
(an HTTP API and generated host SDKs) for exactly this, and it can also run as a
local daemon behind a CLI and a desktop operator app.

## Quality goals

The architecture is optimized, in order, for three properties.

| Goal | What it means | Why it is first-class |
|---|---|---|
| **Sovereignty** | No user data leaves the machine without an explicit action. Inference can run fully local. | The runtime targets regulated and privacy-bound settings where a cloud sandbox is disqualifying. |
| **Accountability** | Every governed action is recorded in a signed, tamper-evident trail that can be verified and, for filesystem changes, reversed. | Autonomy is only delegable if you can answer, after the fact, what happened and undo it. |
| **Control** | A human sets how much an agent may do on its own, approves consequential actions inline, and the runtime enforces hard budgets that an agent cannot bypass. | Bounded autonomy is the difference between a tool and a liability. |

Performance, portability, and developer ergonomics matter, but they are shaped
by these three. Where a trade-off arises, sovereignty and accountability win.

## Stakeholders

| Stakeholder | What they need from Apollia | Where they read |
|---|---|---|
| **Host integrator** (beachhead) | Drive and embed the runtime from their product without reverse-engineering it | [Driving contract how-to](/how-to/integrate-via-driving-contract), [HTTP API reference](/reference/api/apollia-os-runtime-api) |
| **Agent author** (Python) | Write a typed agent or worker against a stable contract | [SDK / `ctx` reference](/reference/sdk) |
| **Operator** | Run and supervise agents day to day | [Operator help](/operator-help) |
| **Contributor** | Change the runtime without breaking its invariants | This section, plus the in-repo agent rulebook |

## Scope of this document

This section maps the runtime and its surfaces. It derives from the code, not
from aspiration. The precise command, endpoint, and service shapes live in the
[reference](/reference/api/apollia-os-runtime-api); this section explains how the parts relate and what
is, and is not, wired today.
