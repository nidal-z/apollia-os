"""HITL (Human-In-The-Loop) resume utilities for ReAct agents.

Provides helpers for extracting pending tool calls from a resumed task
so the ReAct loop can continue where it was suspended.
"""

from __future__ import annotations

from typing import Any


def resume_pending_tool(task: dict[str, Any]) -> dict[str, Any] | None:
    """Extract the pending tool call from a resumed task.

    Returns ``None`` if the task was not resumed or has no pending tool
    context.  Intended for use inside ``run()``::

        pending = resume_pending_tool(task)
        result  = await self.react(task, ctx, user_msg, pending_tool=pending)
    """
    if not task.get("is_resumed"):
        return None
    ir = task.get("input_response")
    if ir is None:
        return None
    context: dict[str, Any] = getattr(ir, "context", None) or {}
    tool_name = context.get("tool")
    tool_args: dict[str, Any] = context.get("args", {})
    if not tool_name:
        return None
    return {"tool": tool_name, "args": tool_args}
