"""Apollia SDK - the Python a package brings to Apollia OS: tools and workflows."""

from apollia.errors import (
    AgentConfigError,
    AgentError,
    DomainError,
    NeedHumanInput,
    PayloadError,
    SchemaError,
    StructuredOutputError,
)
from apollia.tool_decorator import tool
from apollia.types import (
    Ctx,
    FileInput,
    ImageContent,
    LlmMessage,
    LongText,
    MapItemResult,
    Message,
    MessageContent,
    TextContent,
    image_from_bytes,
    image_from_path,
    image_from_url,
    text,
)
from apollia.workflow_decorator import WorkflowCtx, workflow

# REASON: the SDK ships with the runtime and carries the product version, not a
# lifecycle of its own. The string is the one the tag and the changelog use.
__version__ = "0.3.0"

__all__ = [
    "AgentConfigError",
    # Exceptions
    "AgentError",
    # Ctx Protocol surface
    "Ctx",
    "DomainError",
    "FileInput",
    "ImageContent",
    "LlmMessage",
    "LongText",
    "MapItemResult",
    # Multi-modal types
    "Message",
    "MessageContent",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "StructuredOutputError",
    "TextContent",
    "WorkflowCtx",
    # Version
    "__version__",
    "image_from_bytes",
    "image_from_path",
    "image_from_url",
    # Vision helpers
    "text",
    # Decorators
    "tool",
    "workflow",
]
