---
sidebar_position: 12
title: ctx.stt
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.stt`

Service type: `SttInterface` (from `apollia.context.stt`).

### `SttInterface`

_Bases: Protocol_

Audio transcription surface backed by ``apollia-stt``.

#### `transcribe`

```python
async def transcribe(self, path: str, *, language: str | None=None, backend: str | None=None) -> str
```

#### `status`

```python
async def status(self) -> dict[str, Any]
```
