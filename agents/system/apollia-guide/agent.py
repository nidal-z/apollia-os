"""apollia-guide, the built-in product coach for Apollia OS.

A conversational meta-surface that knows the product's real capabilities and
suggests actionable deep-links the desktop renders as buttons.

Design:
  - Reuses ``ctx.llm`` (the user's configured backend). A local backend stays
    fully offline; a cloud backend uses the key and consent the user already
    gave. The agent never allocates a second model.
  - Answers only from the embedded knowledge base (``knowledge/*.md``); it does
    not invent routes, tools, or features.
  - Suggests navigate-only actions, validated against a route allowlist. It
    never proposes a destructive action.

SDK usage: ``@agent`` + ``@on_message`` (returns the reply string), ``ctx.llm``,
``ctx.memory``, ``ctx.logger``.
"""

import json
import re
from pathlib import Path

from apollia import DomainError, agent, on_message
from apollia.types import Ctx, Message

# --------------------------------------------------------------------------
# Knowledge base
# --------------------------------------------------------------------------

_KNOWLEDGE_DIR = Path(__file__).parent / "knowledge"
_MAX_KB_CHARS = 16_000


def _load_knowledge_base() -> str:
    """Concatenate ``knowledge/*.md`` into one context block."""
    parts: list[str] = []
    for name in ("capabilities.md", "tutorials.md"):
        path = _KNOWLEDGE_DIR / name
        if path.is_file():
            parts.append(path.read_text(encoding="utf-8"))
    kb = "\n\n---\n\n".join(parts)
    if len(kb) > _MAX_KB_CHARS:
        kb = kb[:_MAX_KB_CHARS].rsplit("\n", 1)[0] + "\n\n(knowledge base truncated)"
    return kb


_KNOWLEDGE_BASE = _load_knowledge_base()

# --------------------------------------------------------------------------
# System prompts
# --------------------------------------------------------------------------

_OPERATOR_PROMPT = """\
You are Apollia Guide, the built-in product coach for Apollia OS. Your audience
is an operator: a non-technical user who wants to automate recurring tasks
without writing code.

Rules:
1. Never invent a capability. Ground every answer in the knowledge base below
   and, when present, the live `<available_tools>` and `<available_agent_skills>`
   blocks above (the tools and agents installed right now). If something is in
   neither, say so and point to the documentation. Do not fabricate routes,
   tools, or agents.
2. Suggest one concrete next step, when useful, as an action block at the very
   end of your reply:
   ```apollia-actions
   [{"label": "...", "action": "navigate", "payload": {"route": "/..."}}]
   ```
   At most three buttons, using only routes from the knowledge base.
3. Keep replies short and warm: a few sentences, then the optional action block.
4. Answer in the user's language.

Knowledge base:

{knowledge_base}
"""

_BUILDER_PROMPT = """\
You are Apollia Guide, the built-in product coach for Apollia OS. Your audience
is a builder: a developer creating agents, skills, triggers, and MCP
integrations.

Rules:
1. Never invent a capability. Mention only features in the knowledge base below
   and the live `<available_tools>` / `<available_agent_skills>` blocks above
   (the tools and agents installed right now).
2. Suggest one concrete next step as an action block when useful:
   ```apollia-actions
   [{"label": "...", "action": "navigate", "payload": {"route": "/..."}}]
   ```
3. Use precise vocabulary (agent, skill, tool, trigger, step budget, HITL, MCP).
4. Keep replies concise. Answer in the user's language.

Knowledge base:

{knowledge_base}
"""


def build_system_prompt(mode: str | None) -> str:
    template = _BUILDER_PROMPT if mode == "builder" else _OPERATOR_PROMPT
    return template.replace("{knowledge_base}", _KNOWLEDGE_BASE)


# --------------------------------------------------------------------------
# Profile context (read from the shared profile the onboarding agent writes)
# --------------------------------------------------------------------------

_PROFILE_KEYS = ("user.name", "user.role", "user.constraints.sovereignty")


async def build_context_block(ctx: Ctx) -> str:
    """Render a short profile block from ``ctx.profile`` to personalise replies."""
    if ctx.profile is None:
        return ""
    lines: list[str] = []
    for key, label in (
        ("user.name", "name"),
        ("user.role", "role"),
        ("user.constraints.sovereignty", "sovereignty"),
    ):
        value = await ctx.profile.get(key)
        if value:
            lines.append(f"  {label}: {value}")
    if not lines:
        return ""
    return "<user_profile>\n" + "\n".join(lines) + "\n</user_profile>"


async def build_environment_block(ctx: Ctx) -> str:
    """List the tools and agent skills available right now.

    This makes the coach aware of the live environment (native + MCP tools,
    reachable agent skills) so it points to what actually exists rather than a
    static list. Best-effort: a missing surface just yields a shorter block.
    """
    sections: list[str] = []

    if ctx.tools is not None:
        try:
            names = ctx.tools.list_tools()
        except Exception:
            names = []
        lines: list[str] = []
        for name in list(names)[:40]:
            desc = ""
            try:
                info = await ctx.tools.describe(name)
                if isinstance(info, dict):
                    desc = str(info.get("description") or "")
            except Exception:
                desc = ""
            lines.append(f"  {name}: {desc}" if desc else f"  {name}")
        if lines:
            sections.append(
                "<available_tools>\n" + "\n".join(lines) + "\n</available_tools>"
            )

    if ctx.a2a is not None:
        try:
            skills = await ctx.a2a.list_skills()
        except Exception:
            skills = []
        lines = []
        for skill in list(skills or [])[:40]:
            if not isinstance(skill, dict):
                continue
            sid = skill.get("skill_id") or skill.get("id") or ""
            desc = skill.get("description") or ""
            if sid:
                lines.append(f"  {sid}: {desc}" if desc else f"  {sid}")
        if lines:
            sections.append(
                "<available_agent_skills>\n" + "\n".join(lines) + "\n</available_agent_skills>"
            )

    return "\n\n".join(sections)


# --------------------------------------------------------------------------
# Action-block validation (navigate-only, allowlisted routes)
# --------------------------------------------------------------------------

_ACTION_RE = re.compile(r"```apollia-actions\s*(\[.*?\])\s*```", re.DOTALL)
_ALLOWED_ROUTES = {
    "/dashboard", "/agents", "/projects", "/tasks", "/chat", "/automations",
    "/integrations", "/inbox", "/onboarding", "/llm", "/triggers", "/pipelines",
    "/memory", "/observability", "/notifications", "/settings",
}


def _has_only_allowed_routes(raw: str) -> bool:
    """True if the reply has no action block, or one with only allowed routes."""
    match = _ACTION_RE.search(raw)
    if not match:
        return True
    try:
        actions = json.loads(match.group(1))
    except json.JSONDecodeError:
        return False
    if not isinstance(actions, list):
        return False
    for item in actions[:3]:
        if not isinstance(item, dict) or item.get("action") != "navigate":
            return False
        route = (item.get("payload") or {}).get("route")
        if route not in _ALLOWED_ROUTES:
            return False
    return True


# --------------------------------------------------------------------------
# Agent
# --------------------------------------------------------------------------


@agent(
    name="apollia-guide",
    version="0.2.0",
    description=(
        "Conversational coach for Apollia OS: knows the product's real "
        "capabilities and suggests actionable deep-links."
    ),
    tags=("coach", "system", "guide", "meta"),
    memory_namespace="apollia-guide",
    agent_type="system",
    step_budget={"max_steps": 8, "max_tool_calls": 4, "wall_clock_secs": 120},
)
class ApolliaGuide:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        """Run one coaching turn and return the reply text.

        The reply may end with an ``apollia-actions`` block the desktop renders
        as buttons. A block referencing a route outside the allowlist is dropped
        before the reply is returned.
        """
        if ctx.llm is None:
            raise DomainError("NO_LLM", "apollia-guide requires a configured LLM backend")

        profile_block = await build_context_block(ctx)
        environment_block = await build_environment_block(ctx)
        system_prompt = build_system_prompt(mode=None)
        preamble = "\n\n".join(b for b in (profile_block, environment_block) if b)
        if preamble:
            system_prompt = f"{preamble}\n\n{system_prompt}"

        messages: list[dict[str, str]] = [{"role": "system", "content": system_prompt}]
        for m in history or []:
            role = m.get("role", "user")
            messages.append({
                "role": "assistant" if role == "agent" else role,
                "content": m.get("content", ""),
            })
        messages.append({"role": "user", "content": message})

        response = await ctx.llm.complete(messages)
        reply = (response.content or "").strip()

        if not _has_only_allowed_routes(reply):
            # Drop an action block that points somewhere it should not.
            reply = _ACTION_RE.sub("", reply).strip()
            ctx.logger.warning("apollia-guide dropped an action block with a disallowed route")

        return reply
