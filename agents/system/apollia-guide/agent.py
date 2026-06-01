"""apollia-guide - Conversational product coach for Apollia OS.

Dedicated meta-chat surface: knows every product capability, tutorials, and
can suggest actionable deep-links rendered by the frontend as buttons.

Principles (ADR-073):
  - Never allocates a separate LLM. Reuses ``ctx.llm`` → the user's
    configured backend. Local backend → fully offline. Cloud backend →
    same API key, same consent.
  - Never invents a capability that is not in the embedded knowledge base.
  - Only proposes navigate-style actions (no destructive tools).

SDK : decorator-first Apollia AgentKit.
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

from apollia import DomainError, agent, on_message
from apollia.types import Ctx, Message


# ---------------------------------------------------------------------------
# Knowledge base loading
# ---------------------------------------------------------------------------

_KNOWLEDGE_DIR: Path = Path(__file__).parent / "knowledge"


def _load_knowledge_base() -> str:
    """Concatenate ``knowledge/*.md`` into a single context block."""
    if not _KNOWLEDGE_DIR.is_dir():
        return ""
    parts: list[str] = []
    for name in ("capabilities.md", "tutorials.md"):
        path = _KNOWLEDGE_DIR / name
        if path.is_file():
            parts.append(path.read_text(encoding="utf-8"))
    return "\n\n---\n\n".join(parts)


_KNOWLEDGE_BASE: str = _load_knowledge_base()
_MAX_KB_CHARS: int = 16_000


def _truncate_for_context(kb: str) -> str:
    if len(kb) <= _MAX_KB_CHARS:
        return kb
    return kb[: _MAX_KB_CHARS].rsplit("\n", 1)[0] + "\n\n… (knowledge base truncated for context budget)"


# ---------------------------------------------------------------------------
# System prompts (operator/builder)
# ---------------------------------------------------------------------------

_OPERATOR_PROMPT = """\
You are **Apollia Guide**, the built-in product coach for Apollia OS. Your
audience is an **operator** - a non-technical user who wants to automate
recurring tasks without writing code.

## Ground rules

1. **Never invent a capability.** If the user asks about something that is
   not listed in the knowledge base below, say so explicitly and point to
   the documentation. Do not fabricate routes, tools, or features.
2. **Suggest one concrete next step** per response, as an action button
   when possible. Format: end your reply with a single JSON block:
   ```apollia-actions
   [{"label": "…", "action": "navigate", "payload": {"route": "/…"}}]
   ```
   At most 3 buttons. Use the exact routes from the knowledge base.
3. Keep replies **short and warm** - 2–4 sentences before the action block.
4. Respond in the same language as the user.
5. You may read the user's installed agents, integrations, and memory
   namespaces via your allowed tools to personalise suggestions. You may
   NEVER write, delete, or exfiltrate data.

## Knowledge base

{knowledge_base}
"""

_BUILDER_PROMPT = """\
You are **Apollia Guide**, the built-in product coach for Apollia OS. Your
audience is a **builder** - a developer who wants to create agents,
pipelines, triggers, and MCP integrations.

## Ground rules

1. **Never invent a capability.** Only mention features listed in the
   knowledge base below.
2. Suggest one concrete next step per response as an action button:
   ```apollia-actions
   [{"label": "…", "action": "navigate", "payload": {"route": "/…"}}]
   ```
3. Use precise technical vocabulary (manifest, tool, pipeline, trigger,
   step budget, HITL, MCP stdio/HTTP).
4. Keep replies concise - aim for signal, not filler.
5. Respond in the same language as the user.

## Knowledge base

{knowledge_base}
"""


def build_system_prompt(mode: str | None) -> str:
    kb = _truncate_for_context(_KNOWLEDGE_BASE)
    tpl = _BUILDER_PROMPT if mode == "builder" else _OPERATOR_PROMPT
    return tpl.replace("{knowledge_base}", kb)


# ---------------------------------------------------------------------------
# Profile context block (specs-onboarding-agent.md §8.1)
# ---------------------------------------------------------------------------

_PROFILE_KEYS: tuple[str, ...] = (
    "user.name",
    "user.role",
    "user.domain.sector",
    "user.domain.team_size",
    "user.tech.proficiency",
    "user.tools.daily",
    "user.tools.integrations",
    "user.goals",
    "user.constraints.sovereignty",
    "user.constraints.compliance",
    "user.agents.hitl",
    "user.agents.domains",
    "user.agents.trigger",
)


async def build_context_block(ctx: Ctx) -> str:
    """Render the onboarding profile from semantic memory as an XML block."""
    if ctx.memory is None:
        return ""
    fields: dict[str, str | None] = {key: None for key in _PROFILE_KEYS}
    for key in _PROFILE_KEYS:
        try:
            fields[key] = await ctx.memory.recall(key)
        except Exception:
            fields[key] = None

    profile_lines: list[str] = []
    for key, label in (
        ("user.name", "name"),
        ("user.role", "role"),
        ("user.domain.sector", "sector"),
        ("user.domain.team_size", "team_size"),
        ("user.tech.proficiency", "tech_proficiency"),
        ("user.goals", "goals"),
    ):
        if fields[key]:
            profile_lines.append(f"  {label}: {fields[key]}")

    tool_parts = [
        p for p in (fields["user.tools.daily"], fields["user.tools.integrations"]) if p
    ]
    if tool_parts:
        profile_lines.append(f"  tools: {', '.join(tool_parts)}")

    constraint_lines: list[str] = []
    for key, label in (
        ("user.constraints.sovereignty", "sovereignty"),
        ("user.constraints.compliance", "compliance"),
        ("user.agents.hitl", "hitl_default"),
    ):
        if fields[key]:
            constraint_lines.append(f"  {label}: {fields[key]}")

    auto_lines: list[str] = []
    for key, label in (
        ("user.agents.domains", "domains"),
        ("user.agents.trigger", "trigger_preference"),
    ):
        if fields[key]:
            auto_lines.append(f"  {label}: {fields[key]}")

    blocks: list[str] = []
    if profile_lines:
        blocks.append("<user_profile>\n" + "\n".join(profile_lines) + "\n</user_profile>")
    if constraint_lines:
        blocks.append("<constraints>\n" + "\n".join(constraint_lines) + "\n</constraints>")
    if auto_lines:
        blocks.append(
            "<automation_context>\n" + "\n".join(auto_lines) + "\n</automation_context>"
        )

    return "\n\n".join(blocks)


# ---------------------------------------------------------------------------
# Action button extraction
# ---------------------------------------------------------------------------

_ACTION_RE = re.compile(r"```apollia-actions\s*(\[.*?\])\s*```", re.DOTALL)
_ALLOWED_ACTIONS = {"navigate", "invoke"}
_ALLOWED_ROUTES = {
    "/dashboard",
    "/agents",
    "/projects",
    "/tasks",
    "/chat",
    "/automations",
    "/automations?wizard=open",
    "/integrations",
    "/inbox",
    "/onboarding",
    "/llm",
    "/triggers",
    "/pipelines",
    "/memory",
    "/observability",
    "/notifications",
    "/settings",
}


def _parse_action_buttons(raw: str) -> list[dict[str, Any]]:
    match = _ACTION_RE.search(raw)
    if not match:
        return []
    try:
        parsed = json.loads(match.group(1))
    except json.JSONDecodeError:
        return []
    if not isinstance(parsed, list):
        return []
    out: list[dict[str, Any]] = []
    for item in parsed[:3]:
        if not isinstance(item, dict):
            continue
        label = item.get("label")
        action = item.get("action")
        payload = item.get("payload") or {}
        if not isinstance(label, str) or action not in _ALLOWED_ACTIONS:
            continue
        if action == "navigate":
            route = payload.get("route") if isinstance(payload, dict) else None
            if not isinstance(route, str) or route not in _ALLOWED_ROUTES:
                continue
        out.append({"label": label, "action": action, "payload": payload})
    return out


def _strip_action_block(text: str) -> str:
    return _ACTION_RE.sub("", text).strip()


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------


@agent(
    name="apollia-guide",
    version="0.1.0",
    description=(
        "Conversational coach for Apollia OS - knows product capabilities "
        "and suggests actionable deep-links."
    ),
    tags=("coach", "system", "guide", "meta"),
    memory_namespace="apollia-guide",
    agent_type="system",
    step_budget={"max_steps": 30, "max_tool_calls": 10, "wall_clock_secs": 600},
)
class ApolliaGuide:
    """Apollia Guide - product coach agent (uses ``ctx.llm``, never spawns
    a second backend)."""

    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> dict[str, Any]:
        """Run one dialogue turn → JSON with visible text + action buttons."""
        if ctx.llm is None:
            raise DomainError(
                "NO_LLM", "apollia-guide requires a configured LLM backend"
            )

        # Mode (operator|builder) is currently not forwarded by the chat
        # surface; default to operator. Future: read from task metadata.
        mode: str | None = None

        context_block = await build_context_block(ctx)
        base_prompt = build_system_prompt(mode)
        system_prompt = (
            f"{context_block}\n\n{base_prompt}" if context_block else base_prompt
        )

        messages: list[dict[str, str]] = [{"role": "system", "content": system_prompt}]
        for m in history or []:
            role = m.get("role", "user")
            if role == "agent":
                role = "assistant"
            messages.append({"role": role, "content": m.get("content", "")})
        messages.append({"role": "user", "content": message})

        response = await ctx.llm.complete(messages)
        raw_text: str = getattr(response, "content", "") or ""

        action_buttons = _parse_action_buttons(raw_text)
        visible_text = _strip_action_block(raw_text)
        return {"text": visible_text, "action_buttons": action_buttons}
