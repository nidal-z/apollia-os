---
sidebar_position: 1
title: ctx.llm
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.llm`

Service type: `LlmProxy` (from `apollia.context.llm`).

The bridge may leave this service unattached; `ctx.llm` is then `None`.

### `LlmProxy`

_Bases: Protocol_

``ctx.llm`` - LLM backend access.

Four methods: `complete` for single-shot, `chat` for the
system-plus-user case, `map` for a batch sharing one prefix,
`stream` for token iteration.  Stream cleanup propagates
cancellation to the Rust backend. The loop that alternates model calls and
tool calls belongs to an agent: a workflow step that needs one hands the
message to an agent with ``ctx.agents.run``.

`complete` and `chat` take an optional ``schema``: a JSON
Schema the answer must satisfy. The call then returns the validated value
itself, so an agent that needs structure never parses a string::

    REPORT = {
        "type": "object",
        "properties": {"title": {"type": "string"}, "count": {"type": "integer"}},
        "required": ["title", "count"],
    }
    report = await ctx.llm.complete(messages, schema=REPORT)
    ctx.logger.info("%s: %d", report["title"], report["count"])

| Field | Type | Default |
| --- | --- | --- |
| `default_backend` | `str` |  |

#### `complete`

```python
async def complete(self, messages: list[dict[str, Any]], *, backend: str | None=None, temperature: float | None=None, max_tokens: int | None=None, seed: int | None=None, top_p: float | None=None, top_k: int | None=None, min_p: float | None=None, presence_penalty: float | None=None, frequency_penalty: float | None=None, repeat_penalty: float | None=None, stop: list[str] | None=None, schema: dict[str, Any] | None=None) -> LlmResponse | StructuredValue
```

Run a single-shot completion over a message list.

Args:
    messages: Chat messages in OpenAI shape, oldest first.
    backend: Backend to use, or None for `default_backend`.
    temperature: Sampling temperature, or None for the backend default.
    max_tokens: Cap on generated tokens, or None for the backend default.
    seed: Sampling seed, for reproducible output where the backend
        supports it.
    top_p: Nucleus sampling mass, or None for the engine default.
    top_k: Number of most likely tokens kept before sampling, or None
        for the engine default.
    min_p: Minimum probability, relative to the most likely token, a
        token needs to be kept, or None for the engine default.
    presence_penalty: Penalty on tokens already present in the output,
        or None for the engine default.
    frequency_penalty: Penalty growing with how often a token already
        appeared, or None for the engine default.
    repeat_penalty: Multiplicative penalty on repeated tokens, or None
        for the engine default.
    stop: Strings that end generation when produced, or None for none.
        Each setting left to None is not sent, so the call behaves the
        same on the embedded engine and on a remote server. ``top_k``,
        ``min_p`` and ``repeat_penalty`` are not part of the OpenAI
        protocol: they are sent as given, and a server that does not
        know them may refuse the call.
    schema: JSON Schema the answer must satisfy, or None for free-form
        generation. With it the call constrains generation, validates
        what comes back against the same schema, and returns the
        validated value rather than an `LlmResponse`. Supported
        subset: ``object`` and ``array`` nested freely, ``string``,
        ``number``, ``integer``, ``boolean``, ``null``, ``enum``,
        ``required``. ``anyOf``, ``oneOf``, ``allOf`` and ``$ref`` are
        refused by name.

Returns:
    The completed response, with its content, latency and usage; or the
    validated value, as a plain Python object, when ``schema`` is given.

Raises:
    StructuredOutputError: The schema cannot constrain generation, the
        backend has no structured output mode, or the answer does not
        satisfy the schema. Carries the JSON path of the offending node.
        Nothing is retried on your behalf.

#### `chat`

```python
async def chat(self, system: str, user: str, *, backend: str | None=None, temperature: float | None=None, max_tokens: int | None=None, seed: int | None=None, schema: dict[str, Any] | None=None) -> LlmResponse | StructuredValue
```

Run a completion over a system and a user message.

Convenience wrapper over `complete` for the common two-message
case.

Args:
    system: System message.
    user: User message.
    backend: Backend to use, or None for `default_backend`.
    temperature: Sampling temperature, or None for the backend default.
    max_tokens: Cap on generated tokens, or None for the backend default.
    seed: Sampling seed, for reproducible output where the backend
        supports it.
    schema: JSON Schema the answer must satisfy, or None for free-form
        generation. Same contract as `complete`.

Returns:
    The completed response, with its content, latency and usage; or the
    validated value, as a plain Python object, when ``schema`` is given.

Raises:
    StructuredOutputError: Same three situations as `complete`.

#### `map`

```python
async def map(self, prefix: str, items: list[str], *, backend: str | None=None, temperature: float | None=None, max_tokens: int | None=None, max_concurrency: int | None=None) -> list[MapItemResult]
```

Batch a shared-prefix prompt over many items.

Each item is completed as ``[system(prefix), user(item)]``, so the
system message is identical across items. Apollia owns the concurrency
and prefix sharing: on a batching backend (a local ``llama-server``) this
maximizes prefix-cache reuse and continuous batching; elsewhere it
degrades to bounded-concurrent calls. You never touch slots or batching.

Results are order-preserving, one `MapItemResult`
per item. A single failing item never aborts the batch. Typical usage::

    results = await ctx.llm.map(prefix=instructions, items=paragraphs)
    for r in results:
        if r["ok"]:
            use(r["text"])

#### `stream`

```python
async def stream(self, messages: list[dict[str, Any]], *, backend: str | None=None, temperature: float | None=None, max_tokens: int | None=None, seed: int | None=None) -> AsyncIterator[str]
```

Open a streaming completion.

The method itself is ``async`` (it must be ``await``-ed) and
returns an `AsyncIterator` over token
deltas. Typical usage::

    stream = await ctx.llm.stream(messages=[...])
    async for token in stream:
        ctx.events.emit_token(token)

Cancellation propagates to the Rust backend on iterator close.

### `TokenUsage`

_Bases: Protocol_

Token usage stats for an LLM call.

| Field | Type | Default |
| --- | --- | --- |
| `prompt_tokens` | `int` |  |
| `completion_tokens` | `int` |  |
| `cost_usd` | `float` |  |

### `LlmResponse`

_Bases: Protocol_

Synchronous LLM response (after completion).

| Field | Type | Default |
| --- | --- | --- |
| `content` | `str` |  |
| `latency_ms` | `int` |  |
| `usage` | `TokenUsage` |  |
