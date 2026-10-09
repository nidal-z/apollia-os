---
sidebar_position: 10
title: 10. Glossary
---

# 10. Glossary

Key terms used across this section and the rest of the documentation.

| Term | Meaning |
|---|---|
| **Agent** | A mission, examples and the tools it may reach, written as data: an `[[agents]]` entry of the `apollia.toml` of a package, or an agent created in the application. The engine built into the runtime runs it in a ReAct loop, under the runtime's governance. No Python stands for an agent. |
| **Package** | A folder with an `apollia.toml` at its root, distributed as a `.apo` archive: any combination of agents, tools and workflows. A package with no agent is valid. |
| **Tool** | Something an agent may be granted and call: a native tool of the runtime, an MCP tool (`mcp:<server>/<tool>`), or a Python tool of a package (`pkg:<package>/<tool>`), an `async` function under the `@tool` decorator. |
| **Workflow** | An `async` Python function of a package under the `@workflow` decorator, whose steps are declared ahead and which may run agents, run other workflows and ask the user a question. Addressed as `<package>/<workflow>`. |
| **Skill** | A typed, invocable capability addressed by a `skill_id` on the A2A path. No decorator of the SDK declares one: the Python of a package brings tools and workflows. |
| **A2A** | Agent-to-agent invocation: an agent calling another agent's skill by `skill_id`, with guards for depth, self-call, timeout, and chain deadline. |
| **ORIA** | The autonomous execution engine (`apollia-oria`): the ReAct loop, planner, budget, resilience, verification, and context management. |
| **Direct vs orchestrated** | Two ORIA execution modes. Direct runs a single agent loop; orchestrated plans and drives governed tool steps with verification and re-planning. |
| **ReAct** | The reason-then-act loop an agent runs: think, call a tool, observe, repeat. |
| **StepBudget** | The non-bypassable ceiling on reasoning steps, tool calls, and wall-clock time the runtime enforces on every run. |
| **ctx** | The runtime context passed to every tool and every workflow of a package, exposing seventeen typed services. The contract is `sdk/apollia/types.py`. See the [SDK reference](/reference/sdk). |
| **AgentKit** | The Python SDK (`apollia`): the two decorators, the schemas, the test doubles and the helpers the Python of a package is written against. |
| **MCP** | Model Context Protocol. Apollia is an MCP client that discovers and calls external tools, and it serves the agents and workflows the user exposed to an MCP client through a stdio bridge, off by default. |
| **Autonomy tier** | The per-run setting that governs the plan gate and the post-run verification pass. It governs no permission and no approval, so it changes how far a run goes unattended, not what an agent may touch. |
| **HITL** | Human-in-the-loop: the answer a person gives (once, always or refuse) when the governance decision asks about a tool call. The answer is recorded in the journal. |
| **Audit journal** | The append-only, hash-chained, signed record of governed actions, used for verification. |
| **Verify** | Checking a run's audit hash chain and signatures to confirm the record was not altered. |
| **Rollback** | Undoing filesystem changes made in a chat session. The journal format and the replay logic exist in the codebase, but nothing writes to the journal, so this is not a capability of this release. |
| **Replay** | Stepping offline through the trace the journal captured for a run and reporting it identical to its record, diverged or incomplete (`apollia-os audit replay`). It calls no model, runs no tool, and does not run the agent again. |
| **Runner** | The out-of-process sidecar (`apollia-runner`) that runs local speech-to-text (whisper). Local LLM inference is served by the embedded `llama-server` (upstream llama.cpp), which the daemon supervises. |
| **GGUF** | The single-file local model format the embedded `llama-server` loads. |
| **Driving contract** | The stable, versioned HTTP API plus generated host SDKs a host product uses to drive the runtime. See the [HTTP API reference](/reference/api/apollia-os-runtime-api). |
| **EventBus** | The runtime's structured event stream, which the audit journal and observability share. |
| **Sovereignty** | The property that no user data leaves the machine without an explicit action, with local inference and storage by default. |
