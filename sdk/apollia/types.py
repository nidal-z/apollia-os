"""Public types for Apollia agents.

This module exposes the ``Ctx`` Protocol - the unique surface an agent
sees at runtime - plus the multi-modal content types and the
small set of helpers used to build vision messages.

Legacy ``AIPResult`` is kept here. A tool or a workflow returns a plain
value and never builds one.
"""

import base64
import mimetypes
from dataclasses import dataclass, field
from typing import Annotated, Any, Final, Literal, Protocol, TypedDict, runtime_checkable

from apollia.context.agents import AgentsInterface
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
from apollia.context.user import UserInterface
from apollia.context.workflows import WorkflowsInterface
from apollia.context.workspace import WorkspaceContext
from apollia.hitl import ApprovalPayload, InputResponse, QuestionPayload

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
    """An image content block inside an LLM message.

    No backend reachable through ``ctx.llm`` reads the image itself today. The
    bridge flattens every image block to the literal placeholder
    ``[image: <media_type or url>]`` before the message leaves the process and
    logs ``llm.proxy.vision.unsupported``; a model asked about the image is
    therefore answering about that placeholder. The typing is kept so a vision
    backend can be wired without changing the API.
    """

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
    or is not an ``image/*`` type. The block it returns is flattened to a text
    placeholder before it reaches a model, see :class:`ImageContent`.
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

    Raises :class:`ValueError` if ``mime`` does not start with ``image/``. The
    block it returns is flattened to a text placeholder before it reaches a
    model, see :class:`ImageContent`.
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
    """Reference a remote image by URL.

    The block it returns is flattened to a text placeholder before it reaches a
    model, see :class:`ImageContent`.
    """
    return {"type": "image", "source": {"type": "url", "url": url}}


# ──────────────────────────────────────────────────────────────────────
# Workflow input types
# ──────────────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class InputFormat:
    """What a text input of a workflow holds, beyond being text.

    Carried as ``Annotated`` metadata by :data:`LongText` and
    :data:`FileInput`, and written as the JSON Schema ``format`` of the input,
    so the launch form picks its field and the runtime checks the value before
    the run. A date needs no marker: annotate the input ``datetime.date``.

    Attributes:
        format: ``long-text`` or ``file-path``.
    """

    format: str


#: JSON Schema ``format`` of a date input, ``datetime.date``: ``YYYY-MM-DD``.
DATE_FORMAT: Final = "date"
#: JSON Schema ``format`` of a long text input, :data:`LongText`.
LONG_TEXT_FORMAT: Final = "long-text"
#: JSON Schema ``format`` of a file input, :data:`FileInput`.
FILE_PATH_FORMAT: Final = "file-path"

#: A text input written over several lines (a brief, a note); the form shows a
#: text area. The workflow receives a ``str``.
LongText = Annotated[str, InputFormat(LONG_TEXT_FORMAT)]

#: A file the user picks; the form shows a file chooser. The workflow receives
#: the absolute path of the file as a ``str``, and the runtime refuses a launch
#: whose file does not exist.
FileInput = Annotated[str, InputFormat(FILE_PATH_FORMAT)]


# ──────────────────────────────────────────────────────────────────────
# Ctx Protocol - the unique surface the agent sees
# ──────────────────────────────────────────────────────────────────────


@runtime_checkable
class Ctx(Protocol):
    """Runtime context passed to every tool and every workflow.

    The attributes below are the whole surface the code sees, and their reach
    is not uniform: ``ctx.workspace``, ``ctx.notify`` and ``ctx.stt`` are
    ``None`` on every binary this project ships, and ``ctx.mail`` raises
    outside the desktop application. Each service page states its own reach.
    Use type hints to get IDE autocomplete::

        @tool("greet", "Greet a known user.", action="read_personal")
        async def greet(ctx: Ctx, name: str) -> dict:
            user = await ctx.memory.recall(f"user.{name}")
            ...
    """

    llm: LlmProxy
    memory: MemoryInterface
    tools: ToolProxy
    #: Run an installed agent with a message. Bound in a workflow run.
    agents: AgentsInterface
    #: Run an installed workflow with typed inputs. Bound in a workflow run.
    workflows: WorkflowsInterface
    #: Ask the user a question and wait for the answer. Bound in a workflow run.
    user: UserInterface
    mail: MailInterface
    datasources: DatasourcesInterface
    templates: TemplatesInterface
    secrets: SecretsInterface
    events: EventsInterface
    logger: Logger
    #: The user profile resolved for this execution: the profile designated
    #: for the agent, else the default one. ``None`` unless the profile source
    #: is checked for the agent (package ``sources`` default, replaced by the
    #: user's override); each read is recorded in the execution trace. Five
    #: fields (``name``, ``role``, ``function``, ``tools``, ``preferences``)
    #: and a free text (``free_text``); see
    #: :class:`apollia.context.profile.ProfileInterface`.
    profile: ProfileInterface | None
    workspace: WorkspaceContext
    stt: SttInterface
    notify: NotifyInterface
    budget: BudgetView
    #: ``True`` when this run resumes a task that paused on a human.
    is_resumed: bool
    #: The operator's response to the pause being resumed, ``None`` on a first
    #: run. See :mod:`apollia.hitl`.
    input_response: InputResponse | None


# ──────────────────────────────────────────────────────────────────────
# Legacy AIPResult (kept for backward compatibility - see module docstring)
# ──────────────────────────────────────────────────────────────────────


@dataclass
class AIPResult:
    """The result shape of the former agent ``run()`` contract.

    The runtime no longer reads it: an agent is a record of ``apollia.toml``,
    and a ``@tool`` or ``@workflow`` function returns a plain value. A pause
    on a human is raised as :class:`~apollia.errors.NeedHumanInput` (see
    :mod:`apollia.hitl`). The class stays for the assertion helpers of
    :mod:`apollia.testing` and for code written against it. Three factory
    methods build its three statuses:

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
    input_payload: dict[str, Any] | None = None

    @staticmethod
    def completed(text: str, data: dict[str, Any] | None = None) -> "AIPResult":
        """Create a successful result."""
        return AIPResult(status="completed", text=text, data=data or {})

    @staticmethod
    def failed(code: str, message: str) -> "AIPResult":
        """Create a failure result."""
        return AIPResult(status="failed", error_code=code, error_message=message)

    @staticmethod
    def input_required(
        prompt: str,
        context: dict[str, Any] | None = None,
        payload: QuestionPayload | ApprovalPayload | None = None,
    ) -> "AIPResult":
        """Create an input-required result (HITL).

        Args:
            prompt: What the human is asked.
            context: State given back verbatim on resume.
            payload: A typed question or approval, see :mod:`apollia.hitl`.
        """
        return AIPResult(
            status="input_required",
            input_prompt=prompt,
            input_context=context,
            input_payload=dict(payload) if payload is not None else None,
        )

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
        if self.input_payload is not None:
            result["input_payload"] = self.input_payload
        if self.data:
            result["data"] = self.data
        return result


# Re-exports utiles
__all__ = [
    "DATE_FORMAT",
    "FILE_PATH_FORMAT",
    "LONG_TEXT_FORMAT",
    # Legacy
    "AIPResult",
    "AgentsInterface",
    "BudgetView",
    # Ctx surface
    "Ctx",
    "DatasourcesInterface",
    "EventsInterface",
    "FileInput",
    "ImageContent",
    "ImageSourceBase64",
    "ImageSourceUrl",
    "InputFormat",
    "LlmMessage",
    # Re-exports of the ctx Protocol classes (handy for type hints)
    "LlmProxy",
    "LlmResponse",
    "Logger",
    "LongText",
    "MailInterface",
    "MailMessage",
    "MemoryInterface",
    # Multi-modal types
    "Message",
    "MessageContent",
    "NotifyInterface",
    "ProfileInterface",
    "SecretsInterface",
    "SttInterface",
    "TemplatesInterface",
    "TextContent",
    "ToolProxy",
    "UserInterface",
    "WorkflowsInterface",
    "WorkspaceContext",
    "image_from_bytes",
    "image_from_path",
    "image_from_url",
    # Vision helpers
    "text",
]
