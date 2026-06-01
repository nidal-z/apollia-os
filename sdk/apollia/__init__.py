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
    Message,
    MessageContent,
    TextContent,
    image_from_bytes,
    image_from_path,
    image_from_url,
    text,
)

__version__ = "0.5.0"

__all__ = [
    # Decorators
    "agent",
    "skill",
    "on_message",
    "orchestrated",
    # ReAct utility
    "react",
    # Exceptions
    "AgentError",
    "DomainError",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "SkillNotFound",
    "AgentConfigError",
    # Ctx Protocol surface
    "Ctx",
    # Multi-modal types
    "Message",
    "LlmMessage",
    "MessageContent",
    "TextContent",
    "ImageContent",
    # Vision helpers
    "text",
    "image_from_path",
    "image_from_bytes",
    "image_from_url",
    # Version
    "__version__",
]
