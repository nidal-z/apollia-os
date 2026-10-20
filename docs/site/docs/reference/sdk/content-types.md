---
sidebar_position: 17
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

No backend reachable through ``ctx.llm`` reads the image itself today. The
bridge flattens every image block to the literal placeholder
``[image: <media_type or url>]`` before the message leaves the process and
logs ``llm.proxy.vision.unsupported``; a model asked about the image is
therefore answering about that placeholder. The typing is kept so a vision
backend can be wired without changing the API.

| Field | Type | Default |
| --- | --- | --- |
| `type` | `Literal['image']` |  |
| `source` | `ImageSourceBase64 \| ImageSourceUrl` |  |

### `LlmMessage`

_Bases: TypedDict_

A single message in an LLM conversation.

``content`` may be either a plain string (text-only fast path) or a
list of `MessageContent` blocks (multi-modal path).

| Field | Type | Default |
| --- | --- | --- |
| `role` | `Literal['system', 'user', 'assistant']` |  |
| `content` | `list[MessageContent] \| str` |  |

### `MapItemResult`

_Bases: TypedDict_

One result of `ctx.llm.map`, order-preserving with the input items.

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

### `InputFormat`

_Bases: object_

What a text input of a workflow holds, beyond being text.

Carried as ``Annotated`` metadata by `LongText` and
`FileInput`, and written as the JSON Schema ``format`` of the input,
so the launch form picks its field and the runtime checks the value before
the run. A date needs no marker: annotate the input ``datetime.date``.

Attributes:
    format: ``long-text`` or ``file-path``.

| Field | Type | Default |
| --- | --- | --- |
| `format` | `str` |  |

### `AIPResult`

_Bases: object_

The result shape of the former agent ``run()`` contract.

The runtime no longer reads it: an agent is a record of ``apollia.toml``,
and a ``@tool`` or ``@workflow`` function returns a plain value. A pause
on a human is raised as `NeedHumanInput` (see
`apollia.hitl`). The class stays for the assertion helpers of
`apollia.testing` and for code written against it. Three factory
methods build its three statuses:

* `completed` - successful execution
* `failed` - error with structured info
* `input_required` - HITL pause awaiting user input

| Field | Type | Default |
| --- | --- | --- |
| `status` | `str` |  |
| `text` | `str \| None` | `None` |
| `error_code` | `str \| None` | `None` |
| `error_message` | `str \| None` | `None` |
| `input_prompt` | `str \| None` | `None` |
| `input_context` | `dict[str, Any] \| None` | `None` |
| `data` | `dict[str, Any]` | `field(default_factory=dict)` |
| `input_payload` | `dict[str, Any] \| None` | `None` |

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
def input_required(prompt: str, context: dict[str, Any] | None=None, payload: QuestionPayload | ApprovalPayload | None=None) -> 'AIPResult'
```

Create an input-required result (HITL).

Args:
    prompt: What the human is asked.
    context: State given back verbatim on resume.
    payload: A typed question or approval, see `apollia.hitl`.

#### `to_dict`

```python
def to_dict(self) -> dict[str, Any]
```

Serialize to dict for runtime consumption.

Only fields with non-``None`` values (and non-empty ``data``) are
included so the runtime receives a compact payload.

## Aliases and constants

| Name | Value | Meaning |
| --- | --- | --- |
| `MessageContent` | `TextContent \| ImageContent` | Union of all supported content block kinds. |
| `DATE_FORMAT` | `'date'` | JSON Schema ``format`` of a date input, ``datetime.date``: ``YYYY-MM-DD``. |
| `LONG_TEXT_FORMAT` | `'long-text'` | JSON Schema ``format`` of a long text input, `LongText`. |
| `FILE_PATH_FORMAT` | `'file-path'` | JSON Schema ``format`` of a file input, `FileInput`. |
| `LongText` | `Annotated[str, InputFormat(LONG_TEXT_FORMAT)]` | A text input written over several lines (a brief, a note); the form shows a text area. The workflow receives a ``str``. |
| `FileInput` | `Annotated[str, InputFormat(FILE_PATH_FORMAT)]` | A file the user picks; the form shows a file chooser. The workflow receives the absolute path of the file as a ``str``, and the runtime refuses a launch whose file does not exist. |

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

Raises `ValueError` if the file's MIME type cannot be inferred
or is not an ``image/*`` type. The block it returns is flattened to a text
placeholder before it reaches a model, see `ImageContent`.

### `image_from_bytes`

```python
def image_from_bytes(data: bytes, mime: str) -> ImageContent
```

Wrap raw bytes as a base64 image content block.

Raises `ValueError` if ``mime`` does not start with ``image/``. The
block it returns is flattened to a text placeholder before it reaches a
model, see `ImageContent`.

### `image_from_url`

```python
def image_from_url(url: str) -> ImageContent
```

Reference a remote image by URL.

The block it returns is flattened to a text placeholder before it reaches a
model, see `ImageContent`.
