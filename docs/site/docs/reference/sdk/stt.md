---
sidebar_position: 13
title: ctx.stt
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.stt`

Service type: `SttInterface` (from `apollia.context.stt`).

The bridge never attaches this service. `ctx.stt` is `None` on every binary this project ships, so any attribute access on it raises `AttributeError`; no builder that could fill it (`with_stt`) has a caller outside tests. `scripts/check_optional_builders.py` holds that measurement.

### `SttInterface`

_Bases: Protocol_

Audio transcription surface backed by ``apollia-stt``.

#### `transcribe`

```python
async def transcribe(self, path: str, *, language: str | None=None, backend: str | None=None) -> str
```

Transcribe an audio file to text.

Args:
    path: Path to the audio file, resolved inside the workspace.
    language: BCP 47 hint for the spoken language, or None to let the
        backend detect it.
    backend: Backend to use, or None for the configured default.

Returns:
    The transcript.

#### `status`

```python
def status(self) -> dict[str, Any]
```

Return backend readiness: loaded model, device and sample rate.
