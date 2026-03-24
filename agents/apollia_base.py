"""Backward-compatibility wrapper — prefer importing from apollia.agents directly.

This module re-exports all public symbols that previously lived in
``agents/apollia_base.py`` so that existing agents using::

    from apollia_base import BaseReActAgent, AIPResult, resume_pending_tool

continue to work without changes.

New agents should import from the SDK package instead::

    from apollia.agents import BaseReActAgent, AIPResult
    from apollia.utils.hitl import resume_pending_tool
"""

from apollia.agents.react import AIPResult, BaseReActAgent
from apollia.tools.schemas import NATIVE_TOOL_SCHEMAS
from apollia.utils.hitl import resume_pending_tool
from apollia.utils.parsing import (
    ActionParseError,
    extract_json,
    validate_action,
)

__all__ = [
    "AIPResult",
    "ActionParseError",
    "BaseReActAgent",
    "NATIVE_TOOL_SCHEMAS",
    "extract_json",
    "resume_pending_tool",
    "validate_action",
]
