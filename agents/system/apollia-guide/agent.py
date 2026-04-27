"""apollia-guide — Conversational product coach for Apollia OS.

Dedicated meta-chat surface: knows every product capability, tutorials, and
can suggest actionable deep-links rendered by the frontend as buttons.

Principles (ADR-073):
  - Never allocates a separate LLM. Reuses ``ctx.llm`` → the user's
    configured backend. Local backend → fully offline. Cloud backend →
    same API key, same consent.
  - Never invents a capability that is not in the embedded knowledge base.
  - Only proposes navigate-style actions (no destructive tools).

Quick start:
  apollia-os run apollia-guide --input "How do I automate a daily summary?"
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent


# ---------------------------------------------------------------------------
# Knowledge base loading
# ---------------------------------------------------------------------------

_KNOWLEDGE_DIR: Path = Path(__file__).parent / "knowledge"


def _load_knowledge_base() -> str:
    """Concatenate ``knowledge/*.md`` into a single context block.

    Markdown files are kept small enough (< 8 KB each) to fit within typical
    local LLM context windows. If the resulting block exceeds the configured
    budget, :func:`_truncate_for_context` trims trailing sections.
    """
    if not _KNOWLEDGE_DIR.is_dir():
        return ""
    parts: list[str] = []
    for name in ("capabilities.md", "tutorials.md"):
        path = _KNOWLEDGE_DIR / name
        if path.is_file():
            parts.append(path.read_text(encoding="utf-8"))
    return "\n\n---\n\n".join(parts)


_KNOWLEDGE_BASE: str = _load_knowledge_base()

# Rough budget — 8k tokens ≈ 32k chars, keep half for conversation + system.
_MAX_KB_CHARS: int = 16_000


def _truncate_for_context(kb: str) -> str:
    """Cap the knowledge base to :data:`_MAX_KB_CHARS` chars (tail-dropped)."""
    if len(kb) <= _MAX_KB_CHARS:
        return kb
    return kb[: _MAX_KB_CHARS].rsplit("\n", 1)[0] + "\n\n… (knowledge base truncated for context budget)"


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_OPERATOR_PROMPT = f"""\
You are **Apollia Guide**, the built-in product coach for Apollia OS. Your
audience is an **operator** — a non-technical user who wants to automate
recurring tasks without writing code.

## Ground rules

1. **Never invent a capability.** If the user asks about something that is
   not listed in the knowledge base below, say so explicitly and point to
   the documentation. Do not fabricate routes, tools, or features.
2. **Suggest one concrete next step** per response, as an action button
   when possible. Format: end your reply with a single JSON block:
   ```apollia-actions
   [{{"label": "…", "action": "navigate", "payload": {{"route": "/…"}}}}]
   ```
   At most 3 buttons. Use the exact routes from the knowledge base.
3. Keep replies **short and warm** — 2–4 sentences before the action block.
4. Respond in the same language as the user.
5. You may read the user's installed agents, integrations, and memory
   namespaces via your allowed tools to personalise suggestions. You may
   NEVER write, delete, or exfiltrate data.

## Knowledge base

{{knowledge_base}}
"""

_BUILDER_PROMPT = f"""\
You are **Apollia Guide**, the built-in product coach for Apollia OS. Your
audience is a **builder** — a developer who wants to create agents,
pipelines, triggers, and MCP integrations.

## Ground rules

1. **Never invent a capability.** Only mention features listed in the
   knowledge base below.
2. Suggest one concrete next step per response as an action button:
   ```apollia-actions
   [{{"label": "…", "action": "navigate", "payload": {{"route": "/…"}}}}]
   ```
3. Use precise technical vocabulary (manifest, tool, pipeline, trigger,
   step budget, HITL, MCP stdio/HTTP).
4. Keep replies concise — aim for signal, not filler.
5. Respond in the same language as the user.

## Knowledge base

{{knowledge_base}}
"""


def build_system_prompt(mode: str | None) -> str:
    """Render the system prompt for the given UI mode (``operator`` / ``builder``)."""
    kb = _truncate_for_context(_KNOWLEDGE_BASE)
    tpl = _BUILDER_PROMPT if mode == "builder" else _OPERATOR_PROMPT
    return tpl.replace("{knowledge_base}", kb)


# ---------------------------------------------------------------------------
# Action button extraction
# ---------------------------------------------------------------------------

_ACTION_RE = re.compile(r"```apollia-actions\s*(\[.*?\])\s*```", re.DOTALL)

# Whitelisted actions — frontend enforces the same list; keep in sync.
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
    """Extract ``apollia-actions`` JSON blocks and validate against the whitelist.

    Returns an empty list if no block is found or the JSON is malformed —
    the frontend renders the plain text answer only in that case.
    """
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
    """Remove the ``apollia-actions`` fenced block from the user-visible text."""
    return _ACTION_RE.sub("", text).strip()


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------


class ApolliaGuideAgent(ConversationalAgent):
    """Apollia Guide — product coach agent.

    Always uses ``ctx.llm`` (the user's configured backend). Never spawns
    a second LLM, never bypasses the user's consent.
    """

    MAX_TURNS = 30
    TEMPERATURE = 0.5

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "apollia-guide",
            "version": "0.1.0",
            "description": (
                "Conversational coach for Apollia OS — knows product "
                "capabilities and suggests actionable deep-links."
            ),
            "execution_mode": "auto",
            "agent_type": "system",
            "tools_required": [],
            "tools_optional": [
                "navigate",
                "read_memory_namespace",
                "get_user_integrations",
                "get_installed_agents",
            ],
            "memory_namespace": "apollia-guide",
            "max_concurrent_tasks": 1,
            "dangerous_tools_allowed": False,
            "tags": ["coach", "system", "guide"],
        }

    async def converse(
        self,
        ctx: Any,
        user_message: str,
        history: list[dict[str, str]] | None = None,
        mode: str | None = None,
    ) -> tuple[str, list[dict[str, Any]], list[dict[str, str]]]:
        """Run one dialogue turn and return ``(text, action_buttons, messages)``."""
        if ctx.llm is None:
            raise RuntimeError(
                "ApolliaGuideAgent requires ctx.llm — no LLM backend configured"
            )

        system_prompt = build_system_prompt(mode)
        messages: list[dict[str, str]] = list(history) if history else []
        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": system_prompt})
        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        raw_text: str = getattr(response, "content", "") or ""

        action_buttons = _parse_action_buttons(raw_text)
        visible_text = _strip_action_block(raw_text)
        messages.append({"role": "assistant", "content": visible_text})
        return visible_text, action_buttons, messages

    async def run(self, task: Any, ctx: Any) -> AIPResult:
        """Execute a single turn. Task input may carry a ``mode`` metadata field."""
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM", "apollia-guide requires a configured LLM backend"
            )

        task_input = task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
        if task_input is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        if isinstance(task_input, dict):
            parts = task_input.get("parts", [])
            input_text = parts[0]["text"] if parts else str(task_input)
        elif hasattr(task_input, "parts"):
            parts = task_input.parts
            input_text = parts[0].text if parts else str(task_input)
        else:
            input_text = str(task_input)

        metadata = task.get("metadata", {}) if isinstance(task, dict) else getattr(task, "metadata", {})
        mode = metadata.get("mode") if isinstance(metadata, dict) else None

        raw_history = task.get("history", []) if isinstance(task, dict) else getattr(task, "history", [])
        history: list[dict[str, str]] = []
        for msg in (raw_history or []):
            if isinstance(msg, dict):
                role_raw = msg.get("role", "user")
                role = "assistant" if role_raw == "agent" else role_raw
                parts = msg.get("parts", [])
                text = parts[0]["text"] if parts and isinstance(parts[0], dict) else str(msg)
                history.append({"role": role, "content": text})

        text, buttons, _ = await self.converse(ctx, input_text, history or None, mode)
        payload = {"text": text, "action_buttons": buttons}
        return AIPResult.completed(json.dumps(payload))


agent = ApolliaGuideAgent()
