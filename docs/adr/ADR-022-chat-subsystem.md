# ADR-022: Chat subsystem

- Status: Accepted
- Date: 2026-06-04

## Context

Agents run programmatically: triggers fire them and the `TaskRouter` dispatches
fire-and-forget tasks in the background. What is missing is an interactive mode
where the user opens a free conversation, asks questions, and an agent runs on
demand with real-time feedback. Two distinct needs emerge. A user may want to use
the LLM directly with native tools (shell, file IO) without any Python agent, for
exploration and quick ad-hoc tasks. A user may also want to converse with an
installed Python agent that has its own memory and specialized tools, for a deep
interaction with a domain agent.

The `TaskRouter` is designed for fire-and-forget tasks that are stateless between
each other, with a per-agent concurrency semaphore. Chat has fundamentally
different semantics: long sessions, mutable state (history, authorized tools),
token-by-token streaming, and inline human-in-the-loop with progressive
escalation. Forcing chat into the `TaskRouter` would create leaky abstractions: a
chat session is stateful, must not be drained at shutdown, and its
token-by-token stream has no equivalent in the task model where the stream closes
on completion. The constraints are local-first persistence (principle #1), one
actor with one responsibility so the `TaskRouter` is not overloaded (principle
#5), a fresh `StepBudget` enforced per exchange (principle #7), and reuse of the
existing EventBus and Tauri events bridge.

## Decision

We adopt a dedicated `ChatSessionManager` actor with a separate execution path
from the `TaskRouter`, two conversation modes, POST plus SSE streaming, a
per-session human-in-the-loop whitelist, and a separate `chat.db`.

### A dedicated actor

`ChatSessionManager` is a supervised actor, independent from the `TaskRouter`, so
chat exchanges never contend on the task concurrency semaphores. It owns the chat
execution path end to end: session lifecycle, streaming, tool approval, and
persistence. The entry point is `POST /api/v1/sessions/:id/messages`, distinct
from the task entry point, and the manager allows one active exchange per
session.

### Two modes

Chat Libre uses a Rust built-in agent. The runtime embeds a native
`BuiltInChatAgent` that runs a ReAct loop with no Python: a user message becomes
a completion request (system prompt, history, tool specs), the manager streams
the response token by token through the LLM router, and on a tool call it checks
the session authorization, executes or requests approval, feeds the result back,
and loops, all under a `StepBudget` guard that caps iterations per exchange.
Because there is no Python agent, Chat Libre starts instantly and has no memory
namespace.

Chat Agent runs an installed Python agent through the bridge. `ChatSessionManager`
calls the bridge `run` path directly, not through the `TaskRouter`, converting
the session into a task carrying its history. The Python agent accesses its own
memory and tools, and memory population stays at the agent initiative (principle
#6).

### Streaming and human-in-the-loop

Communication is POST plus SSE, not WebSocket: the client sends a message by
POST and receives the response over a persistent SSE stream. WebSocket was
rejected because no WebSocket infrastructure exists in the project and axum plus
SSE already covers tasks, dashboard, and triggers. Chat Libre streams token by
token, emitting a high-frequency `ChatToken` runtime event the Tauri bridge
forwards on a dedicated channel rather than triggering a full IPC refresh.

In chat mode every tool requires human approval by default, which is stricter
than background mode. The user is offered Accept, Refuse, and Always Accept.
Always Accept adds the tool to a per-session whitelist, so the escalation never
contaminates the agent manifest and is scoped to the live session.

### Separate persistence

Chat lives in its own SQLite database, `chat.db`, holding sessions, messages, and
per-session tool authorizations, kept separate from `tasks.db`. The history is
persistent and never leaves the machine.

## Alternatives considered

### WebSocket for chat (rejected)
- Pros: native bidirectional channel, a common standard for real-time chat.
- Cons: no WebSocket infrastructure exists, and adding it introduces a second
  communication stack (HTTP upgrade, frame parsing, reconnection) when POST plus
  SSE already covers the need.

### Session as one long-running task in the TaskRouter (rejected)
- Pros: reuses the existing infrastructure fully, no new module.
- Cons: incompatible with the stateless task model. A chat session is stateful,
  must not be drained at shutdown, the per-agent semaphore would block background
  tasks during a chat, and token-by-token streaming has no equivalent.

### Chat through the TaskRouter with extensions (rejected)
- Pros: a single execution path to maintain.
- Cons: it would force per-session state, continuous streaming, inline approval
  with Always Accept, and a semaphore bypass into the `TaskRouter`, distorting it
  and violating principle #5.

### Chosen: a separate path with a dedicated ChatSessionManager
- Pros: a clean separation of responsibilities, an unchanged and still simple
  `TaskRouter`, chat with its own semantics, database, and events, and a built-in
  Rust agent that gives a Python-free Chat Libre.
- Trade-offs: partial code duplication (EventBus subscription, SSE setup), a
  second actor to maintain, and new chat runtime event variants on the core enum.

## Consequences

- Positive: chat is a first-class citizen rather than a hack on the `TaskRouter`,
  token-by-token streaming works natively, inline Always Accept stays clean as a
  per-session whitelist, the `TaskRouter` is untouched so background tasks carry
  zero regression risk, and Chat Libre starts instantly with no Python.
- Negative / trade-off: new chat event variants enlarge the core enum, `chat.db`
  is an additional database to back up and migrate, and the manager duplicates
  some `TaskRouter` patterns.
- Watch: the `ChatToken` event is emitted at very high frequency, so the EventBus
  behavior under load needs monitoring, and concurrent access from a chat and a
  background task to the same non-thread-safe Python agent may need a lock or a
  distinct bridge instance.

## Architectural principles

- Principle #1 (Local-first): `chat.db` is local and the history never leaves the
  machine.
- Principle #5 (One actor, one responsibility): `ChatSessionManager` is a
  dedicated actor and the `TaskRouter` is not modified.
- Principle #6 (Memory at agent initiative): in Chat Agent the agent accesses its
  memory freely; Chat Libre has no memory namespace.
- Principle #7 (Non-negotiable safeguards): each exchange consumes a fresh
  `StepBudget`, and every tool passes through human approval in chat mode.
- Principle #8 (Human CLI, machine API): chat exposes REST endpoints, an SSE
  stream, and Tauri commands on the same runtime.

## Related

- [ADR-013](ADR-013-human-in-the-loop.md) the human-in-the-loop model the
  per-session approval whitelist builds on.
