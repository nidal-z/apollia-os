"""Apollia SDK — Python toolkit for building Apollia OS agents."""

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
from apollia.skills import skill

__version__ = "0.5.0"

__all__ = [
    # Decorators
    "agent",
    "skill",
    "on_message",
    "orchestrated",
    # Exceptions
    "AgentError",
    "DomainError",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "SkillNotFound",
    "AgentConfigError",
    # Version
    "__version__",
]
