---
sidebar_position: 9
title: ctx.events
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.events`

Service type: `EventsInterface` (from `apollia.context.events`).

### `EventsInterface`

_Bases: Protocol_

Public typed events for streaming, ReAct observability, error reporting.

Only the four canonical event kinds are exposed.
Internal lifecycle events (``task_started``, ``step_completed``, ...)
are emitted by the runtime, not by the agent.

#### `emit_token`

```python
def emit_token(self, delta: str) -> None
```

Streaming token chunk.

#### `emit_thought`

```python
def emit_thought(self, text: str, *, step: int) -> None
```

Agent reasoning trace (ReAct observability).

#### `emit_retry`

```python
def emit_retry(self, *, step: int, reason: str, count: int) -> None
```

Signal that the current step is being retried.

Args:
    step: Zero-based index of the step being retried.
    reason: Why the retry happens, in one short phrase.
    count: How many attempts this step has now consumed.

#### `emit_action_parse_error`

```python
def emit_action_parse_error(self, *, step: int, raw: str, fatal: bool=False) -> None
```

Signal that a model action could not be parsed.

Args:
    step: Zero-based index of the step that produced the action.
    raw: The unparsable model output, kept verbatim for diagnosis.
    fatal: Whether the run stops here rather than retrying.
