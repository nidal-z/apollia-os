"""Shared helpers for Apollia assistants.

Minimal surface — only genuinely shared, reusable pure functions live
here. No base classes, no bootstrap machinery. Each assistant composes
these helpers inline in its ``run()``; the abstraction ceiling stays
low.
"""

from __future__ import annotations

from typing import Any


def workspace_rules(ctx: Any) -> str:
    """Return the workspace rules string (``APOLLIA.md`` content) or ``""``.

    Populated by the runtime on every task via ``ctx.workspace.rules``.
    Stable across a session — safe to inject directly into the system
    prompt without any persistence layer.
    """
    ws = getattr(ctx, "workspace", None)
    if ws is None:
        return ""
    return ws.rules or ""


async def discover_task_specs(ctx: Any) -> list[str]:
    """Return the list of TaskSpec slugs already saved under ``.apollia/tasks/``.

    Performs one ``bash_executor ls`` call. Returns an empty list when
    the workspace has no such directory or when ``ctx.tools`` is absent
    (graceful degradation). Cheap enough to call once per chat turn — no
    memoisation layer needed.
    """
    if ctx.tools is None:
        return []
    try:
        result = await ctx.tools.call("bash_executor", {
            "command": "ls .apollia/tasks/*.md 2>/dev/null | head -50",
            "timeout_secs": 5,
        })
    except Exception:
        return []
    if not result:
        return []
    stdout = result.get("stdout", "")
    if not isinstance(stdout, str):
        return []
    specs: list[str] = []
    for line in stdout.split("\n"):
        name = line.strip()
        if name.endswith(".md"):
            slug = name.rsplit("/", 1)[-1][:-3]
            if slug:
                specs.append(slug)
    return specs


__all__ = ["workspace_rules", "discover_task_specs"]
