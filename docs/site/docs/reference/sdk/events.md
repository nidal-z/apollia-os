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

#### `emit_action_parse_error`

```python
def emit_action_parse_error(self, *, step: int, raw: str, fatal: bool=False) -> None
```
