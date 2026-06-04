# ADR-027: Onboarding agent

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia needs an onboarding flow to collect the user's initial context: name,
preferences, tools, and domain. Most applications use a deterministic wizard with
numbered steps and fixed forms, but that conflicts with the agentic philosophy of
the product. Agents are conversational, adaptive, and non-deterministic.
Onboarding is the user's first contact with the system, so it should showcase
what the system is rather than break from it with a rigid form. The collected
context must also be useful immediately, which means each piece of information has
to be persisted as it is learned rather than committed only at the end of a
completed flow.

## Decision

We implement onboarding as a standard conversational agent, authored with the
decorator-first AgentKit ([ADR-023](ADR-023-sdk-agentkit-design.md)) as an
`@agent` class with an `@on_message` method, driven by a guided system prompt,
persisting each insight as it is learned and proposing permission rules behind
human approval.

### Standard conversational agent

The onboarding agent uses the same contract and the same `ctx` services as any
other agent. It is an `@agent` class whose `@on_message` method drives the
conversation; there is no dedicated onboarding base class. It runs inside a normal
chat session, so it is a first-class demonstration of the agentic experience
rather than a separate code path.

### Guided system prompt, agent-driven flow

The shipped flow calibrates the user in three questions and collects four Tier-1
facts: identity (`user.name`, `user.role`), supervision preference
(`user.agents.hitl`, the human-in-the-loop posture), and sovereignty stance
(`user.constraints.sovereignty`). The agent decides the wording and depth of its
questions. There is no rigid schema beyond those facts, no numbered form, and the
agent reads the conversation history to pick the next question. The user can
leave at any time, and the flow adapts to the user's expertise instead of marching
through a fixed sequence. If coverage is incomplete because the user leaves early,
onboarding can be re-triggered on a specific topic later.

### Immediate persistence of each insight

Each insight is persisted the moment it is learned, through `ctx.memory` and
`ctx.profile` (the canonical user profile defined in
[ADR-011](ADR-011-user-profile.md)). Because persistence is immediate, the
collected context is usable from the very first real conversation after
onboarding, and an early exit still leaves a useful partial profile.

### Human-gated permission rules

When the conversation surfaces a permission decision (for example, allowing a
class of tools the user clearly wants to use), the agent proposes a permission
rule rather than applying it. The proposal goes through human-in-the-loop
approval and is recorded in the permission and tool governance layer
([ADR-015](ADR-015-permission-tool-governance.md)). The agent never silently
grants itself or others a capability.

## Alternatives considered

### Deterministic wizard with numbered steps (rejected)
- Pros: predictable, guaranteed complete coverage.
- Cons: feels mechanical, does not showcase the agentic capabilities, and imposes
  a rigid schema that contradicts the product philosophy.

### Passive learning only, no explicit onboarding (rejected)
- Pros: zero friction.
- Cons: needs many sessions to build a useful profile, giving a poor first
  experience.

### Chosen: conversational agent with a guided system prompt
- Pros: a natural and adaptive experience, a first-class showcase of agentic
  capability, immediate persistence of each answer, and immediate value in the
  first real conversation.
- Trade-offs: coverage can be incomplete on an early exit (mitigated by immediate
  persistence and topic re-triggering), and the experience depends on careful
  system-prompt engineering.

## Consequences

- Positive: onboarding is a first-class demonstration of the agentic model, the
  interaction is natural and adaptive, and the learned profile is useful from the
  next conversation onward.
- Negative / trade-off: incomplete coverage on an early exit, and a dependence on
  good system-prompt engineering.
- Watch: completion rates and area coverage, and whether system-prompt variants
  improve the flow.

## Architectural principles

- Principle #3 (Minimal contract): the onboarding agent uses the same AgentKit
  contract as any other agent.
- Principle #6 (Memory at agent initiative): the agent decides what to remember,
  insight by insight, rather than following a fixed schema.

## Related

- [ADR-011](ADR-011-user-profile.md) the canonical user profile the agent writes
  to as it learns.
- [ADR-015](ADR-015-permission-tool-governance.md) the governance layer that
  records the human-approved permission rules the agent proposes.
