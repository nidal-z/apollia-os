"""Apollia SDK - Python toolkit for building Apollia OS agents."""

from apollia.agent import agent
from apollia.errors import (
    AgentConfigError,
    AgentError,
    DomainError,
    NeedHumanInput,
    PayloadError,
    SchemaError,
    SkillNotFound,
)
from apollia.messages import on_message
from apollia.orchestration import orchestrated
from apollia.react import react
from apollia.skills import skill
from apollia.types import (
    Ctx,
    ImageContent,
    LlmMessage,
    MapItemResult,
    Message,
    MessageContent,
    TextContent,
    image_from_bytes,
    image_from_path,
    image_from_url,
    text,
)

# REASON: the SDK ships with the runtime and carries the product version, not a
# lifecycle of its own. PEP 440 normalises the "-preview" suffix to "rc0" in the
# built distribution metadata, so `pip show apollia-sdk` reports 0.1.0rc0 while
# this string stays the human-facing one used by the tag and the changelog.
__version__ = "0.1.0-preview"

__all__ = [
    "AgentConfigError",
    # Exceptions
    "AgentError",
    # Ctx Protocol surface
    "Ctx",
    "DomainError",
    "ImageContent",
    "LlmMessage",
    "MapItemResult",
    # Multi-modal types
    "Message",
    "MessageContent",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "SkillNotFound",
    "TextContent",
    # Version
    "__version__",
    # Decorators
    "agent",
    "image_from_bytes",
    "image_from_path",
    "image_from_url",
    "on_message",
    "orchestrated",
    # ReAct utility
    "react",
    "skill",
    # Vision helpers
    "text",
]
