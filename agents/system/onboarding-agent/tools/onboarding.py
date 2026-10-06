"""The tool of the onboarding agent: turn a short conversation into a proposal.

The onboarding agent is a declarative agent. Its mission leads a short
conversation, every question of which may be skipped; this module holds the one
thing the mission cannot do on its own, handing the desktop a profile the
person can read over. The agent calls ``propose_profile`` once, at the end,
with what the person actually said.

Nothing here writes the user profile. The tool returns a proposal, the fields
of a profile and its free text, none of them required:

  name          how agents should address the person
  role          their job or position
  function      the department or field they work in
  tools         the tools of their day
  preferences   the language, tone and answer format they prefer
  free_text     anything else agents should know

The desktop reads the proposal in the result of the call, shows it in the
profile form, and writes the profile only when the person saves it.

Two meta keys go to the memory of the onboarding (``ctx.memory``), the second
one being what the desktop watches to learn the conversation is over:

  1. onboarding.version             (ONBOARDING_VERSION)
  2. onboarding.completed_at        (ISO 8601, LAST, the desktop signal)
"""

from datetime import datetime, timezone
from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MEMORY_SOURCE: str = "onboarding"
ONBOARDING_VERSION: str = "4.0"

CONFIDENCE_EXPLICIT: float = 0.9

MAX_VALUE_LENGTH: int = 500
MAX_FREE_TEXT_LENGTH: int = 2000

# The fields of a profile, in the order the form shows them.
PROFILE_FIELDS: tuple[str, ...] = (
    "name",
    "role",
    "function",
    "tools",
    "preferences",
    "free_text",
)

# Substrings whose presence in a value makes the tool refuse it. Detection is
# conservative on purpose: a false positive only drops one answer, which the
# person can still type in the form.
INJECTION_PATTERNS: tuple[str, ...] = (
    "; rm",
    "$(",
    "`",
    "ignore previous",
    "ignore the previous",
    "system prompt",
    "--system",
)

NEXT_STEP: str = (
    "The proposal is on the person's screen, in a form they can correct. Tell them in "
    "one or two sentences that their profile is ready to review and that nothing is "
    "saved until they save it. Do not list the fields and do not ask anything else."
)

# ---------------------------------------------------------------------------
# Guards and normalisation
# ---------------------------------------------------------------------------


def _is_suspicious_value(value: str) -> bool:
    low = value.lower()
    return any(pat in low for pat in INJECTION_PATTERNS)


def _truncate(field: str, value: str) -> str:
    limit = MAX_FREE_TEXT_LENGTH if field == "free_text" else MAX_VALUE_LENGTH
    return value if len(value) <= limit else value[:limit]


def refusal_reason(value: str) -> str | None:
    """Why ``value`` is left out of the proposal, or ``None`` when it may enter."""
    if _is_suspicious_value(value):
        return "the value looks like an instruction, not an answer"
    return None


def build_proposal(answers: dict[str, str | None]) -> tuple[dict[str, str], dict[str, str]]:
    """The proposal and the refusals drawn from ``answers``.

    A field the person did not speak about is absent from both: no field is
    required, and an empty value proposes nothing.
    """
    proposal: dict[str, str] = {}
    refused: dict[str, str] = {}
    for field in PROFILE_FIELDS:
        raw = answers.get(field)
        value = str(raw).strip() if raw is not None else ""
        if not value:
            continue
        reason = refusal_reason(value)
        if reason is not None:
            refused[field] = reason
            continue
        proposal[field] = _truncate(field, value)
    return proposal, refused


# ---------------------------------------------------------------------------
# Stores
# ---------------------------------------------------------------------------


async def _remember(ctx: Ctx, key: str, value: str) -> None:
    await ctx.memory.remember(
        key=key,
        value=value,
        source=MEMORY_SOURCE,
        confidence=CONFIDENCE_EXPLICIT,
    )


async def _close(ctx: Ctx) -> None:
    """Write the meta keys in strict order, ``completed_at`` last."""
    await _remember(ctx, "onboarding.version", ONBOARDING_VERSION)
    await _remember(ctx, "onboarding.completed_at", datetime.now(timezone.utc).isoformat())


# ---------------------------------------------------------------------------
# Tool
# ---------------------------------------------------------------------------


@tool(
    "propose_profile",
    "Propose a profile from what the person said in the onboarding conversation. "
    "Writes nothing to the profile: the person reads the proposal in a form, corrects "
    "it, and saves it or not. Call it once, when the conversation is over.",
    action="write_or_delete",
    examples=[
        {"name": "Lea", "role": "office manager", "tools": "Excel, Outlook"},
        {"role": "sales lead", "preferences": "short answers, in French"},
        {},
    ],
)
async def propose_profile(
    ctx: Ctx,
    name: Annotated[
        str | None,
        "How agents should address the person. Leave out when they did not say.",
    ] = None,
    role: Annotated[
        str | None,
        "Their job or position. Leave out when they did not say.",
    ] = None,
    function: Annotated[
        str | None,
        "The department or field they work in. Leave out when they did not say.",
    ] = None,
    tools: Annotated[
        str | None,
        "The tools of their day, comma separated. Leave out when they did not say.",
    ] = None,
    preferences: Annotated[
        str | None,
        "The language, tone and answer format they prefer. Leave out when they did not say.",
    ] = None,
    free_text: Annotated[
        str | None,
        "Anything else they want agents to know. Leave out when there is nothing.",
    ] = None,
) -> dict[str, Any]:
    """Return the proposal drawn from the conversation and mark it closed."""
    proposal, refused = build_proposal(
        {
            "name": name,
            "role": role,
            "function": function,
            "tools": tools,
            "preferences": preferences,
            "free_text": free_text,
        }
    )
    await _close(ctx)
    return {
        "proposal": proposal,
        "refused": refused,
        "completed": True,
        "next": NEXT_STEP,
    }
