---
sidebar_position: 3
title: Sovereignty and local-first
---

# Sovereignty and local-first

Sovereignty is the claim Apollia is built to make good on: your data, your
models, and your runtime stay yours. "Local-first" is how that claim becomes
concrete rather than aspirational. This page explains what those words mean in
practice and why they are the core of the value proposition, not a feature bolted
onto it.

## The default never leaves the machine

The starting posture is the sovereign one. Storage is a local SQLite database.
The runtime's API binds to a Unix socket, and it only listens on TCP when you
explicitly enable it. There is no telemetry and no automatic upload of anything.
This is what "local-first" means operationally: the path that runs when you do
nothing special is the path that keeps everything on your hardware. Sovereignty
that you have to configure your way into is not sovereignty; here it is the
default, and leaving it is the deliberate act.

## Inference runs local

Apollia carries its own local inference. A model in GGUF format runs through a
supervised runner process, so the reasoning that drives an agent can happen
entirely on your machine with no external LLM call. This is the piece that makes
the rest meaningful: local storage is not much of a guarantee if every thought
the agent has is a round trip to someone else's server.

Two honest bounds on the local inference available today. The runner loads a
single-file GGUF model. And the delivered local capability is text generation:
Apollia does not present a local embeddings pipeline as a shipped feature. Stated
plainly so you can plan around what is there rather than around what a roadmap
implies.

## Zero external dependency

Local-first only holds if running locally does not quietly require a fleet of
services. It does not. The runtime needs no Docker, no Node, no external database,
and no separately installed Python: the interpreter is embedded. One binary is
the deployable unit. Every optional connection to an external service degrades
gracefully instead of turning into a hard requirement. This is the second
principle, and it is what keeps "local" from meaning "local plus five things you
also have to run."

## The cloud is a choice, never a default

Cloud inference exists, and it is opt-in on your own key. Even when enabled, the
local model stays the default and escalation to a cloud provider is something you
choose and control, not something that happens because a request looked hard. The
design treats the cloud as a capability you can reach for, not a dependency you
inherit.

## Your memory stays yours

An agent's memory, its episodic, semantic, and procedural layers, can be exported
and imported by you. That is not a convenience feature; it is the concrete form
of ownership. Data you can take out, inspect, and move is data that is yours in a
way a privacy policy cannot promise. Portability is what turns "we store it
locally" into "you hold it."

## Why this is the core, not a feature

Every other strength of Apollia rests on this one. Accountability means little if
the record lives on a server you do not control. Autonomy is hard to delegate if
delegating it means shipping your data offsite. Sovereignty and local-first are
the constraint that shapes every default, which is exactly why they read as the
first two of the [eight principles](/explanation/the-8-principles) rather than as
options in a settings page. For where this sits in the architecture, see the
sovereignty section of
[Cross-cutting concepts](/architecture/crosscutting-concepts) and the
[Constraints](/architecture/constraints) page. For the actual configuration knobs
(socket versus TCP, backend selection), see the
[Configuration reference](/reference/configuration).

## Related

- [The 8 principles](/explanation/the-8-principles)
- [Constraints](/architecture/constraints)
- [Cross-cutting concepts](/architecture/crosscutting-concepts)
- [Configuration reference](/reference/configuration)
