---
sidebar_position: 17
title: ctx.input_response
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.input_response`

Data member: `InputResponse | None` (from `apollia.hitl`).

The bridge may leave this service unattached; `ctx.input_response` is then `None`.

The operator's response to the pause being resumed, ``None`` on a first run. See `apollia.hitl`.

### `InputResponse`

_Bases: TypedDict_

What a resumed agent reads from `Ctx.input_response`.

``approved`` and ``reason`` are the operator's decision. ``answer`` is the
proposition id, free text or value given to a question, ``None`` for an
approval. ``payload`` is the pause being answered, so an agent that pauses
more than once knows which pause it is resuming from. ``context`` is the
``context`` it passed to `NeedHumanInput`, given back
verbatim: the way to carry what an earlier pause learned into a later one.

| Field | Type | Default |
| --- | --- | --- |
| `approved` | `bool` |  |
| `reason` | `str \| None` |  |
| `answer` | `Any` |  |
| `payload` | `HitlPayload \| None` |  |
| `context` | `dict[str, Any]` |  |
