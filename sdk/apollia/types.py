"""Public types for Apollia agents.

This module exposes the ``Ctx`` Protocol - the unique surface an agent
sees at runtime - plus the multi-modal content types and the
small set of helpers used to build vision messages.

Legacy ``AIPResult`` is kept here for backward compatibility with the
agents under :mod:`apollia.agents`.  New
agents should not depend on it directly: handlers return plain values
and the dispatch boundary builds the canonical AIPResult dict.
"""

from __future__ import annotations

import base64
import mimetypes
from dataclasses import dataclass, field
from typing import Any, Literal, Protocol, TypedDict, runtime_checkable

from apollia.context.a2a import A2AInterface
from apollia.context.budget import BudgetView
from apollia.context.datasources import DatasourcesInterface
from apollia.context.events import EventsInterface
from apollia.context.llm import LlmProxy, LlmResponse
from apollia.context.logger import Logger
from apollia.context.mail import MailInterface, MailMessage
from apollia.context.memory import MemoryInterface
from apollia.context.notify import NotifyInterface
from apollia.context.profile import ProfileInterface
from apollia.context.secrets import SecretsInterface
from apollia.context.stt import SttInterface
from apollia.context.templates import TemplatesInterface
from apollia.context.tools import ToolProxy
from apollia.context.workspace import WorkspaceContext

# ──────────────────────────────────────────────────────────────────────
# Multi-modal content (Vision API typing)
# ──────────────────────────────────────────────────────────────────────


class TextContent(TypedDict):
    """A text content block inside an LLM message."""

    type: Literal["text"]
    text: str


class ImageSourceBase64(TypedDict):
    """Inline base64-encoded image source."""

    type: Literal["base64"]
    media_type: str  # e.g. ``image/png``
    data: str  # base64-encoded payload


class ImageSourceUrl(TypedDict):
    """Remote image source (HTTPS URL)."""

    type: Literal["url"]
    url: str


class ImageContent(TypedDict):
    """An image content block inside an LLM message."""

    type: Literal["image"]
    source: ImageSourceBase64 | ImageSourceUrl


#: Union of all supported content block kinds.
MessageContent = TextContent | ImageContent


class LlmMessage(TypedDict, total=False):
    """A single message in an LLM conversation.

    ``content`` may be either a plain string (text-only fast path) or a
    list of :data:`MessageContent` blocks (multi-modal path).
    """

    role: Literal["system", "user", "assistant"]
    content: list[MessageContent] | str


class MapItemResult(TypedDict, total=False):
    """One result of :meth:`ctx.llm.map`, order-preserving with the input items.

    ``index`` and ``ok`` are always present. On success ``text`` and ``usage``
    are set; on failure ``error`` is set. A single failing item never aborts the
    batch, so consumers branch on ``ok``.
    """

    index: int
    ok: bool
    text: str
    error: str
    usage: dict[str, Any]


class Message(TypedDict):
    """A conversational message exchanged with a user-facing agent."""

    role: Literal["user", "assistant"]
    content: str


# ──────────────────────────────────────────────────────────────────────
# Vision helpers
# ──────────────────────────────────────────────────────────────────────


def text(content: str) -> TextContent:
    """Create a text content block for multi-modal LLM messages."""
    return {"type": "text", "text": content}


def image_from_path(path: str) -> ImageContent:
    """Load an image file from disk and encode it as base64.

    Raises :class:`ValueError` if the file's MIME type cannot be inferred
    or is not an ``image/*`` type.
    """
    mime, _ = mimetypes.guess_type(path)
    if mime is None or not mime.startswith("image/"):
        raise ValueError(f"Cannot determine image MIME type for: {path}")
    with open(path, "rb") as f:
        data = base64.b64encode(f.read()).decode("ascii")
    return {
        "type": "image",
        "source": {"type": "base64", "media_type": mime, "data": data},
    }


def image_from_bytes(data: bytes, mime: str) -> ImageContent:
    """Wrap raw bytes as a base64 image content block.

    Raises :class:`ValueError` if ``mime`` does not start with ``image/``.
    """
    if not mime.startswith("image/"):
        raise ValueError(f"Invalid image MIME type: {mime}")
    return {
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": mime,
            "data": base64.b64encode(data).decode("ascii"),
        },
    }


def image_from_url(url: str) -> ImageContent:
    """Reference a remote image by URL."""
    return {"type": "image", "source": {"type": "url", "url": url}}


# ──────────────────────────────────────────────────────────────────────
# Ctx Protocol - the unique surface the agent sees
# ──────────────────────────────────────────────────────────────────────


@runtime_checkable
class Ctx(Protocol):
    """Runtime context passed to every agent handler.

    Exposes 100% of the Apollia backend through 14 typed services.
    Use type hints to get IDE autocomplete::

        @skill("foo.bar")
        async def bar(self, name: str, ctx: Ctx) -> dict:
            user = await ctx.memory.recall(f"user.{name}")
            ...
    """

    llm: LlmProxy
    memory: MemoryInterface
    tools: ToolProxy
    a2a: A2AInterface
    mail: MailInterface
    datasources: DatasourcesInterface
    templates: TemplatesInterface
    secrets: SecretsInterface
    events: EventsInterface
    logger: Logger
    profile: ProfileInterface
    workspace: WorkspaceContext
    stt: SttInterface
    notify: NotifyInterface
    budget: BudgetView

    # NOTE: ReAct lives as a free function `apollia.react(ctx, ...)`,
    # not on the Ctx protocol. Keeping it off the Ctx surface avoids a
    # PyO3 injection layer and lets users compose alternative reasoning
    # loops without subclassing the runtime context.


# ──────────────────────────────────────────────────────────────────────
# Legacy AIPResult (kept for backward compatibility - see module docstring)
# ──────────────────────────────────────────────────────────────────────


@dataclass
class AIPResult:
    """Result returned by an agent's ``run()`` method to the Apollia runtime.

    The runtime deserializes this via :meth:`to_dict` to determine the
    outcome of a task execution.  Three factory methods cover the common
    cases:

    * :meth:`completed` - successful execution
    * :meth:`failed` - error with structured info
    * :meth:`input_required` - HITL pause awaiting user input
    """

    status: str
    text: str | None = None
    error_code: str | None = None
    error_message: str | None = None
    input_prompt: str | None = None
    input_context: dict[str, Any] | None = None
    data: dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def completed(text: str, data: dict[str, Any] | None = None) -> AIPResult:
        """Create a successful result."""
        return AIPResult(status="completed", text=text, data=data or {})

    @staticmethod
    def failed(code: str, message: str) -> AIPResult:
        """Create a failure result."""
        return AIPResult(status="failed", error_code=code, error_message=message)

    @staticmethod
    def input_required(prompt: str, context: dict[str, Any] | None = None) -> AIPResult:
        """Create an input-required result (HITL)."""
        return AIPResult(status="input_required", input_prompt=prompt, input_context=context)

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dict for runtime consumption.

        Only fields with non-``None`` values (and non-empty ``data``) are
        included so the runtime receives a compact payload.
        """
        result: dict[str, Any] = {"status": self.status}
        if self.text is not None:
            result["text"] = self.text
        if self.error_code is not None:
            result["error_code"] = self.error_code
        if self.error_message is not None:
            result["error_message"] = self.error_message
        if self.input_prompt is not None:
            result["input_prompt"] = self.input_prompt
        if self.input_context is not None:
            result["input_context"] = self.input_context
        if self.data:
            result["data"] = self.data
        return result


# Re-exports utiles
__all__ = [
    # Ctx surface
    "Ctx",
    # Multi-modal types
    "Message",
    "LlmMessage",
    "MessageContent",
    "TextContent",
    "ImageContent",
    "ImageSourceBase64",
    "ImageSourceUrl",
    # Vision helpers
    "text",
    "image_from_path",
    "image_from_bytes",
    "image_from_url",
    # Re-exports of the ctx Protocol classes (handy for type hints)
    "LlmProxy",
    "LlmResponse",
    "MemoryInterface",
    "ToolProxy",
    "A2AInterface",
    "MailInterface",
    "MailMessage",
    "DatasourcesInterface",
    "TemplatesInterface",
    "SecretsInterface",
    "EventsInterface",
    "Logger",
    "ProfileInterface",
    "WorkspaceContext",
    "SttInterface",
    "NotifyInterface",
    "BudgetView",
    # Legacy
    "AIPResult",
]
