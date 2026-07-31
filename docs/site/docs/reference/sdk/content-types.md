---
sidebar_position: 16
title: Content types and helpers
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# Content types and helpers

Multi-modal content blocks, message shapes, and the legacy `AIPResult`, defined in `sdk/apollia/types.py`.

### `TextContent`

_Bases: TypedDict_

A text content block inside an LLM message.

| Field | Type | Default |
| --- | --- | --- |
| `type` | `Literal['text']` |  |
| `text` | `str` |  |

### `ImageSourceBase64`

_Bases: TypedDict_

Inline base64-encoded image source.

| Field | Type | Default |
| --- | --- | --- |
| `type` | `Literal['base64']` |  |
| `media_type` | `str` |  |
| `data` | `str` |  |

### `ImageSourceUrl`

_Bases: TypedDict_

Remote image source (HTTPS URL).

| Field | Type | Default |
| --- | --- | --- |
| `type` | `Literal['url']` |  |
| `url` | `str` |  |

### `ImageContent`

_Bases: TypedDict_

An image content block inside an LLM message.

| Field | Type | Default |
| --- | --- | --- |
| `type` | `Literal['image']` |  |
| `source` | `ImageSourceBase64 | ImageSourceUrl` |  |

### `LlmMessage`

_Bases: TypedDict_

A single message in an LLM conversation.

``content`` may be either a plain string (text-only fast path) or a
list of :data:`MessageContent` blocks (multi-modal path).

| Field | Type | Default |
| --- | --- | --- |
| `role` | `Literal['system', 'user', 'assistant']` |  |
| `content` | `list[MessageContent] | str` |  |

### `MapItemResult`

_Bases: TypedDict_

One result of :meth:`ctx.llm.map`, order-preserving with the input items.

``index`` and ``ok`` are always present. On success ``text`` and ``usage``
are set; on failure ``error`` is set. A single failing item never aborts the
batch, so consumers branch on ``ok``.

| Field | Type | Default |
| --- | --- | --- |
| `index` | `int` |  |
| `ok` | `bool` |  |
| `text` | `str` |  |
| `error` | `str` |  |
| `usage` | `dict[str, Any]` |  |

### `Message`

_Bases: TypedDict_

A conversational message exchanged with a user-facing agent.

| Field | Type | Default |
| --- | --- | --- |
| `role` | `Literal['user', 'assistant']` |  |
| `content` | `str` |  |

### `AIPResult`

_Bases: object_

Result returned by an agent's ``run()`` method to the Apollia runtime.

The runtime deserializes this via :meth:`to_dict` to determine the
outcome of a task execution.  Three factory methods cover the common
cases:

* :meth:`completed` - successful execution
* :meth:`failed` - error with structured info
* :meth:`input_required` - HITL pause awaiting user input

| Field | Type | Default |
| --- | --- | --- |
| `status` | `str` |  |
| `text` | `str | None` | `None` |
| `error_code` | `str | None` | `None` |
| `error_message` | `str | None` | `None` |
| `input_prompt` | `str | None` | `None` |
| `input_context` | `dict[str, Any] | None` | `None` |
| `data` | `dict[str, Any]` | `field(default_factory=dict)` |

#### `completed`

```python
def completed(text: str, data: dict[str, Any] | None=None) -> 'AIPResult'
```

Create a successful result.

#### `failed`

```python
def failed(code: str, message: str) -> 'AIPResult'
```

Create a failure result.

#### `input_required`

```python
def input_required(prompt: str, context: dict[str, Any] | None=None) -> 'AIPResult'
```

Create an input-required result (HITL).

#### `to_dict`

```python
def to_dict(self) -> dict[str, Any]
```

Serialize to dict for runtime consumption.

Only fields with non-``None`` values (and non-empty ``data``) are
included so the runtime receives a compact payload.

## Helpers

### `text`

```python
def text(content: str) -> TextContent
```

Create a text content block for multi-modal LLM messages.

### `image_from_path`

```python
def image_from_path(path: str) -> ImageContent
```

Load an image file from disk and encode it as base64.

Raises :class:`ValueError` if the file's MIME type cannot be inferred
or is not an ``image/*`` type.

### `image_from_bytes`

```python
def image_from_bytes(data: bytes, mime: str) -> ImageContent
```

Wrap raw bytes as a base64 image content block.

Raises :class:`ValueError` if ``mime`` does not start with ``image/``.

### `image_from_url`

```python
def image_from_url(url: str) -> ImageContent
```

Reference a remote image by URL.
