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

See [the execution model](/architecture/decisions#execution-model) for the engine that runs on top of this.

## Inference as a supervised sidecar

Local LLM inference runs in a separate process, not in the daemon: the embedded
`llama-server` (upstream llama.cpp), which the daemon spawns and speaks to over its
OpenAI-compatible HTTP API. Native tool calling is driven by the model's chat
template (`--jinja`), not a custom grammar path. Isolating inference keeps a model
crash or an out-of-memory event from taking the runtime down, and lets the runtime
speak to the engine over a narrow interface. Continuous batching decodes several
requests in the same pass, and tracking upstream llama.cpp widens the range of
supported model architectures.

The current supervision is honest about its limits: the daemon spawns the
inference process, but automatic health monitoring and restart are not yet
wired. See [Risks and technical debt](/architecture/risks-and-technical-debt).
See [local inference](/architecture/decisions#local-inference).

## A PyO3 bridge with trait-decoupled services

Agents are Python; the runtime is Rust. The bridge is PyO3 with
`pyo3-async-runtimes` for async interop. The runtime does not hand Python raw
internals: it exposes a set of services (the `ctx` object) behind Rust traits,
so the agent contract is decoupled from the implementation and can be mocked for
testing. The agent side sees one typed context with fifteen services; the Rust
side can evolve behind it.

See [stack and runtime](/architecture/decisions#stack-and-runtime) and [the agent contract](/architecture/decisions#agent-contract); `ctx` is documented
in the [SDK reference](/reference/sdk).

## A machine contract for host integration

The runtime is meant to be driven by a host product, so its HTTP surface is a
first-class, stable contract, not an afterthought. The OpenAPI specification is
generated from the code, served by the daemon, and versioned (`/api/v1`, with
breaking changes reserved for a future `/api/v2`). Typed host SDKs in TypeScript
and Python are generated from that spec. An integrator drives a real daemon
without reverse-engineering anything.

See [host integration](/architecture/decisions#host-integration). The generated surface is the [HTTP API reference](/reference/api/apollia-os-runtime-api);
the how-to is [Integrate via the driving contract](/how-to/integrate-via-driving-contract).

## Governance lives in the runtime, not in the agent

The step budget and the audit trail are enforced by the runtime around every
agent, not implemented by each agent: every tool call an agent makes is recorded
and bounded, whichever path it runs on. The autonomy tier reaches an
orchestrated run the same way, through the engine that plans and executes it.

<!-- claim:hitl-wired-in-chat-path-only -->
Permission rules and human-in-the-loop approvals are narrower, and stating the
boundary matters more than the slogan: they are wired on the chat path only. The
tool calls an installed Python agent makes through `ctx.tools` meet no
permission rule and no human checkpoint. That is a deliberate position rather
than an oversight, and the [agent trust
model](/explanation/agent-trust-model) explains why. The whole picture is in
[Cross-cutting concepts](/architecture/crosscutting-concepts) and the
[accountability model](/explanation/accountability-model).

The governing decisions are [the permission model](/architecture/decisions#permission-model),
[human in the loop](/architecture/decisions#human-in-the-loop), and [secrets and API
authentication](/architecture/decisions#secrets-and-api-auth).
