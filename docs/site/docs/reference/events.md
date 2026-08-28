---
sidebar_position: 9
title: Runtime events
---

# Runtime events

Everything the runtime does that another part of the system can react to travels
as one `RuntimeEvent` on a single in-process broadcast bus. The desktop
application, the HTTP streams, the notification engine, the audit journal and
the observability store are all readers of that one bus.

This page is the catalogue. It is generated from the Rust source, so it says
what the binary carries and not what an earlier design intended.

## What a variant name is

A variant name is a wire contract. It reaches the desktop application inside the
`runtime-event` envelope as `event_type`, and it reaches an HTTP client through
the task and chat event streams. Renaming one breaks every reader that matched
on it, so a rename is a wire-format change, not a refactor.

## Categories, and why they matter more than names

The desktop bridge does not forward variant names for the interface to switch
on. It attaches a **category**, and the webview dispatches on that: one category
maps to one refresh, one store, one panel. A variant added to an existing
category is read by whatever already reads that category; a variant given a new
category is read by nobody until a listener exists for it.

That asymmetry is why the category is in the table below. Three variants sat in
categories no listener read, and the interface received them and did nothing,
which no test and no compiler could see.

## Lag

The bus is a bounded ring. A subscriber that falls behind receives a lag report
rather than the events it missed. The rule is one line and it lives in one
place, `apollia_core::events::ResilientReceiver`: log a `WARN` naming the
subscriber and the number of events dropped, resubscribe to the tail, and carry
on. Never panic on lag, and never drop events without saying so.

The server-sent-event routes are the one exception, named as such: they hand a
stream to the HTTP layer and a stream owns its receiver, so there is nothing to
resubscribe. They keep the half of the rule that is reachable, the `WARN`.

## The catalogue

Before the table, one caveat it does not carry. `HookDecisionRecorded` reports
the decision of a `PreToolUse` hook, and `PreToolUse` is outside the supported
surface of `v0.1.0-preview`. Its decision is applied best effort: a handler that
times out, fails to deliver, or answers with something unparseable falls back to
`allow`, and the tool call proceeds.

<!-- BEGIN GENERATED: eventbus-catalogue -->

### `a2a`

| Variant | Payload | What it reports |
|---|---|---|
| `A2ACompatibilityWarning` | named fields | Semver compatibility mismatch between the required and advertised version. |
| `A2AGuardTriggered` | named fields | An A2A safeguard blocked an inter-agent invocation. |
| `A2AInvocationCompleted` | named fields | An A2A invocation finished, emitted after the result or a failure is received. |
| `A2AInvocationStarted` | named fields | An A2A invocation started, emitted by `A2AInvoker` before task submission. |
| `A2ASkillCompleted` | named fields | An A2A skill just finished, emitted after the result is received. |
| `A2ASkillInvoked` | named fields | An A2A skill was just invoked, emitted before the effective submission. |

### `agent-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `AgentDegraded` | named fields | An agent moved to a degraded state. |
| `AgentDisabled` | named fields | An installed agent was disabled (no longer loaded at boot). |
| `AgentEnabled` | named fields | An installed agent was enabled for auto-start at boot. |
| `AgentInstalled` | named fields | An agent was installed permanently. |
| `AgentLoadFailed` | named fields | Loading an installed agent failed at boot. |
| `AgentMessageAcked` | named fields | A delivered message was acknowledged and removed from the store. |
| `AgentMessageDelivered` | named fields | A pending message was leased to its recipient (delivered on receive). |
| `AgentMessageDropped` | named fields | A message was dropped without being processed. |
| `AgentMessageSent` | named fields | A message was sent between two agents via the AgentMailbox. |
| `AgentReady` | tuple | An agent finished initializing and is operational (state: Active). |
| `AgentRegistered` | tuple | An agent was registered in the Registry (state: Initializing). |
| `AgentStopped` | tuple | An agent stopped cleanly. |
| `AgentStopping` | tuple | An agent is shutting down (state: Stopping, draining tasks). |
| `AgentUninstalled` | named fields | An installed agent was removed. |
| `MailboxGuardTriggered` | named fields | A mailbox safeguard blocked a send. |

### `approval-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `HitlFilesystemRequired` | named fields | A filesystem operation by an agent requires a human approval. |
| `PermissionRequired` | named fields | A tool invocation requires a human approval. |
| `TaskApprovalTimeout` | named fields | An `input_required` task expired, canceled automatically by the `TimeoutWatcher`. |
| `TaskInputRequired` | named fields | A task is suspended awaiting human input. |
| `TaskResumed` | named fields | A task resumed after a HITL suspension. |

### `chat-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `ChatApprovalRequired` | named fields | A human approval is required for a tool call in the chat. |
| `ChatApprovalResolved` | named fields | A tool-call approval was resolved by the user. |
| `ChatApprovalTimeout` | named fields | A tool-call approval expired (timeout). |
| `ChatError` | named fields | An error occurred in a chat session. |
| `ChatMessageSent` | named fields | A user message was sent in a session. |
| `ChatResponseCompleted` | named fields | The full response was generated. |
| `ChatResponseStarted` | named fields | The runtime began generating a response. |
| `ChatSessionClosed` | named fields | A chat session was closed. |
| `ChatSessionCreated` | named fields | A chat session was created. |
| `ChatToolCallCompleted` | named fields | A tool call finished in a chat session. |
| `ChatToolCallStarted` | named fields | A tool call started in a chat session. |
| `ChatUserInputRequired` | named fields | The agent requests information from the user via the `ask_user` tool. |
| `ChatUserInputResolved` | named fields | The user answered the `ask_user` tool's questions. |
| `DecisionPointRecorded` | named fields | A significant decision point was captured with its alternatives. |
| `ThinkingEnded` | named fields | Emitted at the end of the Reasoner phase, reasoning is complete. |
| `ThinkingStarted` | named fields | Emitted at the start of the Reasoner phase, the agent begins "thinking". |
| `ToolCallRetrying` | named fields | A tool call is being retried after a transient failure. |

### `chat-token`

| Variant | Payload | What it reports |
|---|---|---|
| `ChatToken` | named fields | A streaming token was produced by the LLM. |

### `hook-decision`

| Variant | Payload | What it reports |
|---|---|---|
| `HookDecisionRecorded` | named fields | A `PreToolUse` hook resolved a decision for a tool call. |

### `llm-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `CostCeilingReached` | named fields | The hybrid routing cost ceiling was reached while `HardStop` is active. |
| `LlmCallCompleted` | named fields | An LLM call finished, emitted by `complete_with_observability()`. |
| `LlmCallFailed` | named fields | An LLM call failed, emitted by `complete_with_observability()` when the |
| `LlmFallbackTriggered` | named fields | The LLM router switched to a secondary backend. |
| `LlmModelFailed` | named fields | Loading an LLM backend failed: backend skipped, runtime continues. |
| `LlmModelLoading` | named fields | An LLM backend is loading (before `load()` or HTTP initialization). |
| `LlmModelReady` | named fields | An LLM backend is ready: model loaded in memory or cloud connection verified. |
| `LlmResponseCaptured` | named fields | A full LLM response captured for deterministic replay. |
| `MetaLlmBudgetExceeded` | named fields | Emitted by `MetaLlmOrchestrator` when the tokens/session budget is exceeded. |
| `TokenBudgetUpdated` | named fields | Session budget update, emitted after each LLM call. |

### `memory-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `SharedNamespaceAdded` | named fields | An agent gained access to a shared memory namespace. |

### `onboarding-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `OnboardingCompleted` | named fields | Emitted when the onboarding state machine reaches the `Done` phase. |
| `OnboardingRequired` | none | Emitted on first launch when the UserMemory is empty. |
| `OnboardingStarted` | named fields | Emitted when an onboarding session is triggered (full or partial). |

### `plan-approval`

| Variant | Payload | What it reports |
|---|---|---|
| `PlanAbandoned` | named fields | A run was abandoned after hitting the replan limit or a fatal replan error. |
| `PlanApprovalRequired` | named fields | A plan was generated and is awaiting an operator decision before execution. |
| `PlanApproved` | named fields | A plan gate decision resolved to approval; the ActorLoop is starting. |
| `PlanRejected` | named fields | An operator rejected a plan; the engine will replan with the feedback. |

### `plan-mode`

| Variant | Payload | What it reports |
|---|---|---|
| `ChatPlanApproved` | named fields | A submitted session plan was approved by the operator. |
| `ChatPlanPhaseChanged` | named fields | The plan-mode lifecycle phase of a session changed. |
| `ChatPlanRejected` | named fields | A submitted session plan was rejected by the operator. |
| `PlanSubmitted` | named fields | A session plan was submitted for approval. |
| `PlanUpdated` | named fields | A session plan was mutated (proposed, edited, reordered, or a step status changed). |

### `session-metrics`

| Variant | Payload | What it reports |
|---|---|---|
| `SessionMetricsUpdated` | named fields | Aggregated session metrics updated. |

### `stt-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `SttModelLoaded` | named fields | The STT model was loaded successfully, engine operational. |
| `SttRecordingStarted` | none | STT audio recording started (hotkey activated). |
| `SttRecordingStopped` | named fields | STT audio recording stopped (hotkey released or silence detected). |
| `SttTranscribed` | named fields | An STT transcription completed successfully. |
| `SttTranscriptionFailed` | named fields | An error occurred during an STT transcription. |

### `system`

| Variant | Payload | What it reports |
|---|---|---|
| `AllReady` | none | All components are ready, runtime operational. |
| `ContextCompacted` | named fields | Emitted by `ContextManager` when the conversation history was compacted. |
| `FileModifiedSinceRead` | named fields | A previously read file was modified between two accesses. |
| `McpServerHealthChanged` | named fields | An MCP server's operational health changed. |
| `McpServerReloaded` | named fields | An MCP server was hot-reloaded successfully. |
| `ShutdownRequested` | none | Shutdown requested (SIGTERM or CLI command). |
| `ToolCircuitBroken` | named fields | A tool circuit breaker opened. |
| `ToolCircuitRestored` | named fields | A tool circuit breaker closed again after recovery. |
| `ToolOutputCaptured` | named fields | A tool output captured for deterministic replay. |

### `task-changed`

| Variant | Payload | What it reports |
|---|---|---|
| `BashFilePathsExtracted` | named fields | File paths extracted from a bash command's output. |
| `PlanAlternativesGenerated` | named fields | Two alternative plans were generated in parallel by the Reasoner. |
| `PlanCacheHit` | named fields | A plan was retrieved from the cache instead of being generated by the Reasoner. |
| `PlanCompleted` | named fields | All steps completed successfully, plan finished. |
| `PlanFailed` | named fields | The plan failed irrecoverably. |
| `PlanGenerated` | named fields | An `ExecutionPlan` was generated by the Reasoner and persisted to SQLite. |
| `PlanReplanning` | named fields | A replan was triggered after a retryable step failed. |
| `StepCompleted` | named fields | A step completed successfully, emitted by `ActorLoop` after each successful call. |
| `StepFailed` | named fields | A step failed, emitted by `ActorLoop` after each failure. |
| `StepStarted` | named fields | A step started executing, emitted by `ActorLoop` before each tool or LLM call. |
| `TaskCanceled` | named fields | A task was canceled. |
| `TaskCompleted` | named fields | A task finished (success or failure). |
| `TaskStarted` | named fields | A task started on an agent. |
| `TodoUpdated` | named fields | The session todo list changed after a successful `todo_write`. |
| `VerificationCompleted` | named fields | The post-run verification pass produced a verdict on an orchestrated run. |

### `trace-event`

| Variant | Payload | What it reports |
|---|---|---|
| `A2AInvokeCompleted` | named fields | An A2A invocation finished. |
| `A2AInvokeStarted` | named fields | An A2A (agent-to-agent) invocation starts. |
| `ActionParseError` | named fields | The LLM returned an invalid action JSON that could not be repaired. |
| `AgentLog` | named fields | An agent emitted a message via `ctx.log(level, msg, **fields)`. |
| `LlmCallStarted` | named fields | An LLM call is about to be sent (before `LlmProxy::complete`). |
| `Retry` | named fields | The ReAct loop retries a step (parse error, tool failure, etc.). |
| `Thought` | named fields | The LLM emitted a ReAct `thought` (reasoning chain). |
| `ToolCallCompleted` | named fields | The tool finished its execution. |
| `ToolCallDenied` | named fields | The tool was denied (manifest, permission rule, HITL). |
| `ToolCallStarted` | named fields | A tool is about to be invoked (before the dispatcher). |

### `trigger-fired`

| Variant | Payload | What it reports |
|---|---|---|
| `TriggerDisabled` | named fields | A trigger was disabled via the CLI or the API. |
| `TriggerEnabled` | named fields | A trigger was enabled via the CLI or the API. |
| `TriggerError` | named fields | An error occurred while processing a trigger. |
| `TriggerFired` | named fields | A trigger fired, task submitted to the runtime. |
| `TriggerQueueFull` | named fields | A trigger's bounded queue is full, the trigger is dropped. |
| `TriggerSkipped` | named fields | A trigger was skipped (OnBusyPolicy::Skip or agent busy). |
| `TriggersReloaded` | named fields | The TriggerEngine reloaded its configuration (hot reload or initial start). |
<!-- END GENERATED: eventbus-catalogue -->
