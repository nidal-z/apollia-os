---
sidebar_position: 4
title: ctx.a2a
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.a2a`

Service type: `A2AInterface` (from `apollia.context.a2a`).

### `A2AInterface`

_Bases: Protocol_

Inter-agent invocation API.

``ctx.a2a.invoke`` is the single typed entry point for a synchronous
agent-to-agent call: it runs a skill on another agent and awaits a typed
result. Asynchronous, non-blocking messaging lives on a separate service,
``ctx.mail``.

#### `invoke`

```python
async def invoke(self, skill_id: str, input: dict[str, Any] | None, *, timeout_secs: int | None=None) -> dict[str, Any]
```

Invoke an A2A skill and return the full invocation envelope.

On success the return value is the A2A envelope, not the skill's
payload directly::

    {
        "result": {
            "task_id": "...",
            "status": "completed",
            "output": [{"type": "data", "data": {...}}],
            "error": None,
            "artifacts": [],
            "input_required_data": None,
        },
        "agent_name": "...",
        "skill_id": "...",
        "duration_ms": 123,
    }

The skill's returned dict lives at ``result.output[0].data``. Use
`apollia.utils.formatting.a2a_result_data` to unwrap it, or
`apollia.utils.formatting.aip_result_text` on ``result`` for the
text parts. ``timeout_secs=None`` uses the backend default.

On error the return value is a failed ``AIPResult`` dict
(``{"status": "failed", "error": {...}, ...}``), not the envelope.

#### `discover`

```python
async def discover(self, skill_id: str) -> dict[str, Any] | None
```

Return the skill card for ``skill_id``, or None if unknown.

#### `list_skills`

```python
async def list_skills(self) -> list[dict[str, Any]]
```

Return a skill card for every skill reachable from this context.

#### `skill_as_tool`

```python
async def skill_as_tool(self, skill_id: str) -> dict[str, Any]
```

Return an LLM tool descriptor for an A2A skill.

The descriptor the bridge emits is
``{"name", "description", "input_schema"}``, plus ``"examples"`` when
the skill ships sample payloads. ``name`` is the encoded form
``a2a__<skill_id>`` with every dot replaced by a double underscore,
which is what the bridge decodes back on dispatch.

The schema key is ``input_schema``, and ``ctx.llm.run_tools`` reads
``parameters``. Passing this descriptor straight to ``run_tools``, or
to ``apollia.react``, therefore announces the tool's name and
description and nothing about its arguments: the missing key falls back
to the empty schema ``{}``, so on a local backend the generated grammar
constrains nothing. Rename the key before handing it on::

    card = await ctx.a2a.skill_as_tool("pdf.read_text")
    tool = {
        "name": card["name"],
        "description": card["description"],
        "parameters": card["input_schema"],
    }

The method is ``async``: the bridge resolves the skill against
the in-process A2A registry. Always call with ``await``.

### `SkillCard`

_Bases: Protocol_

Discovered skill metadata returned by `A2AInterface.discover`.

| Field | Type | Default |
| --- | --- | --- |
| `skill_id` | `str` |  |
| `name` | `str` |  |
| `description` | `str` |  |
| `agent_name` | `str` |  |
| `input_schema` | `dict[str, Any]` |  |
| `output_schema` | `dict[str, Any]` |  |
