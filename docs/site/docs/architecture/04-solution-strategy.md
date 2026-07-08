---
sidebar_position: 4
title: 4. Solution strategy
---

# 4. Solution strategy

Five structural decisions carry the quality goals into the code. Each one is
recorded as an architecture decision; this page states the choice and the
reasoning, and links onward.

## Tokio actors with message passing, no shared state

The runtime core is a set of Tokio actors. Each actor owns its state
exclusively and communicates only through bounded `mpsc` channels behind a
clonable handle. There is no `Arc<Mutex<T>>` shared between actors. This makes
it structurally impossible for one actor to mutate another's state, which is how
the "one actor, one responsibility" principle becomes an enforced property
rather than a convention. The trade-off is more message plumbing; the payoff is
the absence of a whole class of async deadlocks and data races.

See ADR-005 for the ORIA execution model that runs on top of this.

## Inference as a supervised sidecar

Local LLM inference runs in a separate runner process, not in the daemon.
`llama.cpp` is loaded through FFI in that runner, which the daemon spawns and
holds a single-flight load lock over. Isolating inference keeps a model crash or
an out-of-memory event from taking the runtime down, and lets the runtime speak
to the runner over a narrow interface. The runner keeps persistent slots with a
KV cache and fingerprint routing so repeated calls are cheap.

The current supervision is honest about its limits: the daemon spawns and
load-locks the runner, but automatic health monitoring and restart are not yet
wired. See [Risks and technical debt](/architecture/risks-and-technical-debt).
The decision itself is ADR-007.

## A PyO3 bridge with trait-decoupled services

Agents are Python; the runtime is Rust. The bridge is PyO3 with
`pyo3-async-runtimes` for async interop. The runtime does not hand Python raw
internals: it exposes a set of services (the `ctx` object) behind Rust traits,
so the agent contract is decoupled from the implementation and can be mocked for
testing. The agent side sees one typed context with fourteen services; the Rust
side can evolve behind it.

The bridge decision is ADR-002; the `ctx` contract is ADR-024 and is documented
in the [SDK reference](/reference/sdk).

## A machine contract for host integration

The runtime is meant to be driven by a host product, so its HTTP surface is a
first-class, stable contract, not an afterthought. The OpenAPI specification is
generated from the code, served by the daemon, and versioned (`/api/v1`, with
breaking changes reserved for a future `/api/v2`). Typed host SDKs in TypeScript
and Python are generated from that spec. An integrator drives a real daemon
without reverse-engineering anything.

This is ADR-037. The generated surface is the [HTTP API reference](/reference/api/apollia-os-runtime-api);
the how-to is [Integrate via the driving contract](/how-to/integrate-via-driving-contract).

## Governance lives in the runtime, not in the agent

Permissions, the audit trail, human-in-the-loop approvals, autonomy tiers, and
the step budget are enforced by the runtime around every agent, not implemented
by each agent. An agent author cannot forget them and an operator cannot be
surprised by their absence. This is what makes autonomy delegable, and it is the
subject of [Cross-cutting concepts](/architecture/crosscutting-concepts) and the
[accountability model](/explanation/accountability-model).

The governing decisions are ADR-015 (permission and tool governance), ADR-013
(human-in-the-loop), and ADR-016 (secrets and API auth).
