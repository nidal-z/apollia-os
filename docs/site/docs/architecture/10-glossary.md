---
sidebar_position: 10
title: 10. Glossary
---

# 10. Glossary

Key terms used across this section and the rest of the documentation.

| Term | Meaning |
|---|---|
| **Agent** | A Python process that reasons and acts autonomously in a ReAct loop, under the runtime's governance. It duck-types a `manifest()` and an async `run()`. |
| **Worker** | An agent that exposes one or more typed skills for other agents to call. A domain expert in a team. |
| **Director** | An agent that orchestrates workers by calling their skills. |
| **Skill** | A typed, invocable capability an agent exposes, declared with the `@skill` decorator and addressed by a `skill_id`. |
| **A2A** | Agent-to-agent invocation: an agent calling another agent's skill by `skill_id`, with guards for depth, self-call, timeout, and chain deadline. |
| **ORIA** | The autonomous execution engine (`apollia-oria`): the ReAct loop, planner, budget, resilience, verification, and context management. |
| **Direct vs orchestrated** | Two ORIA execution modes. Direct runs a single agent loop; orchestrated plans and drives governed tool steps with verification and re-planning. |
| **ReAct** | The reason-then-act loop an agent runs: think, call a tool, observe, repeat. |
| **StepBudget** | The non-bypassable ceiling on reasoning steps, tool calls, and wall-clock time the runtime enforces on every run. |
| **ctx** | The runtime context passed to every agent handler, exposing fourteen typed services. The contract is `sdk/apollia/types.py`. See the [SDK reference](/reference/sdk). |
| **AgentKit** | The Python SDK (`apollia`): the decorators, schemas, harness, and helpers an author writes against. |
| **MCP** | Model Context Protocol. Apollia is an MCP client that discovers and calls external tools, and can expose a limited inbound MCP server. |
| **Autonomy tier** | The operator-set dial for how much an agent may do without asking. Lower tiers keep a human in the loop on more actions. |
| **HITL** | Human-in-the-loop: an approval a person resolves before a consequential action runs. The decision is recorded. |
| **Audit journal** | The append-only, hash-chained, signed record of governed actions, used for verification and rollback. |
| **Verify** | Checking a run's audit hash chain and signatures to confirm the record was not altered. |
| **Rollback** | Undoing filesystem changes made in a chat session by replaying the inverse of each mutation in reverse order. |
| **Replay** | Re-executing and comparing a run. Abandoned by decision; not a capability. |
| **Runner** | The inference sidecar process (`apollia-runner`) that loads a GGUF model through `llama.cpp` and serves completions. |
| **GGUF** | The single-file local model format the runner loads. |
| **Driving contract** | The stable, versioned HTTP API plus generated host SDKs a host product uses to drive the runtime. See the [HTTP API reference](/reference/api/apollia-os-runtime-api). |
| **EventBus** | The runtime's structured event stream, which the audit journal and observability share. |
| **Sovereignty** | The property that no user data leaves the machine without an explicit action, with local inference and storage by default. |
