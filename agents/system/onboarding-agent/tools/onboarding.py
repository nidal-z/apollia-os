"""The tool of the onboarding agent: record what the person said, then close.

The onboarding agent is a declarative agent. Its mission leads a short
conversation in two phases; this module holds the one thing the mission cannot
do on its own, writing the facts down. The agent calls ``propose_profile`` with
the facts the person just gave and follows the state the tool returns.

Phase 1 (mandatory) collects the four calibration facts:

  user.name                         first name
  user.role                         role
  user.agents.hitl                  always | critical-only | never
  user.constraints.sovereignty      local-only | local-preferred | cloud-ok

Phase 2 (optional, skippable) enriches the profile with the keys of
``TIER2_KEYS``. Closed-vocabulary values are mapped to their canonical option.

Closure happens once the four calibration facts are collected AND an end
signal arrives: the agent asks to close (``close=True``), or every optional
key is collected. The meta keys are then written in strict order, the last
one being what the desktop watches:

  1. onboarding.profile_type        (operator | builder)
  2. onboarding.version             (ONBOARDING_VERSION)
  3. onboarding.suggested_agents    (JSON list)
  4. onboarding.proposed_rules      (JSON list, the permission rules proposed)
  5. onboarding.completed_at        (ISO 8601, LAST, the desktop signal)

``user.*`` keys go to the user profile through ``ctx.profile``; the meta keys
go to the memory of the onboarding (``ctx.memory``). The host grants both to
this module only, when it is the exact code the binary ships.
"""

import json
from datetime import datetime, timezone
from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MEMORY_SOURCE: str = "onboarding"
ONBOARDING_VERSION: str = "3.0"

CONFIDENCE_EXPLICIT: float = 0.9

MAX_VALUE_LENGTH: int = 500

TIER1_KEYS: tuple[str, ...] = (
    "user.name",
    "user.role",
    "user.agents.hitl",
    "user.constraints.sovereignty",
)

# PII categories the onboarding never collects.
PII_KEY_PREFIXES: tuple[str, ...] = (
    "user.email",
    "user.phone",
    "user.tel",
    "user.address",
    "user.adresse",
    "user.iban",
    "user.card",
    "user.ssn",
    "user.financial",
    "user.finance",
)

# Substrings whose presence in a value makes the tool refuse it. Detection is
# conservative on purpose: a false positive only drops one answer.
INJECTION_PATTERNS: tuple[str, ...] = (
    "; rm",
    "$(",
    "`",
    "ignore previous",
    "ignore the previous",
    "system prompt",
    "--system",
)

VALID_HITL = {"always", "critical-only", "never"}
VALID_SOVEREIGNTY = {"local-only", "local-preferred", "cloud-ok"}
VALID_PROFICIENCY = {"debutant", "intermediaire", "avance", "expert"}
VALID_LANGUAGE = {"fr", "en"}

# Optional profile keys, most useful first. They map to the canonical keys of
# the profile schema (the `user.` prefix is stripped on write by ctx.profile).
TIER2_KEYS: tuple[str, ...] = (
    "user.goals",
    "user.domain.sector",
    "user.tech.proficiency",
    "user.tools.daily",
    "user.domain.team_size",
    "user.tech.stack",
    "user.tech.integrations",
    "user.constraints.compliance",
    "user.preferences.language",
    "user.preferences.llm",
    "user.agents.domains",
    "user.agents.trigger",
)

PROFILE_KEYS: frozenset[str] = frozenset(TIER1_KEYS + TIER2_KEYS)

TIER2_LABELS: dict[str, str] = {
    "user.goals": "what they want to accomplish with Apollia",
    "user.domain.sector": "their industry or field",
    "user.tech.proficiency": (
        "technical comfort, mapped to debutant | intermediaire | avance | expert"
    ),
    "user.tools.daily": "the tools they use daily",
    "user.domain.team_size": "team size (solo, small team, larger org)",
    "user.tech.stack": "their technical stack (languages, frameworks)",
    "user.tech.integrations": (
        "SENSITIVE: services to connect (e.g. github, slack, notion, gmail); "
        "restate and confirm before recording"
    ),
    "user.constraints.compliance": (
        "SENSITIVE: compliance constraints (e.g. GDPR, HIPAA); restate and confirm before recording"
    ),
    "user.preferences.language": "preferred language, mapped to fr | en",
    "user.preferences.llm": "preferred LLM model, if any",
    "user.agents.domains": "the domains where agents would help",
    "user.agents.trigger": "how agents should be triggered (manual, scheduled...)",
}

# The exact question for each calibration fact still missing.
NEXT_QUESTION: dict[str, str] = {
    "user.name": "Ask ONE open question for their first name and what they do day to day.",
    "user.role": "Ask ONE open question for their first name and what they do day to day.",
    "user.agents.hitl": (
        'Ask exactly: "When an agent is about to send an email or change a file, '
        "do you prefer to (1) always approve, (2) approve only critical actions, "
        'or (3) let the agent act autonomously?"'
    ),
    "user.constraints.sovereignty": (
        'Ask exactly: "Can your agents use cloud APIs (OpenAI, Anthropic...), or '
        'must everything stay on your machine? (local only / local preferred / cloud OK)"'
    ),
}

# Integrations whose API endpoint a proposed rule opens.
INTEGRATION_ENDPOINTS: dict[str, str] = {
    "github": "https://api.github.com",
    "slack": "https://slack.com/api/",
    "notion": "https://api.notion.com",
    "gmail": "https://gmail.googleapis.com",
}

# ---------------------------------------------------------------------------
# Guards and normalisation
# ---------------------------------------------------------------------------


def normalise_key(raw: str) -> str:
    """Lowercase, trim, and replace inner spaces by underscores."""
    key = raw.strip().lower().replace(" ", "_")
    if key and not key.startswith("user."):
        key = f"user.{key}"
    return key


def _is_suspicious_value(value: str) -> bool:
    low = value.lower()
    return any(pat in low for pat in INJECTION_PATTERNS)


def _is_pii_key(key: str) -> bool:
    return any(key.startswith(p) for p in PII_KEY_PREFIXES)


def _truncate(value: str) -> str:
    return value if len(value) <= MAX_VALUE_LENGTH else value[:MAX_VALUE_LENGTH]


def refusal_reason(key: str, value: str) -> str | None:
    """Why ``(key, value)`` is not recorded, or ``None`` when it may be."""
    if not value.strip():
        return "empty value"
    if _is_pii_key(key):
        return "personal data the onboarding never collects"
    if key not in PROFILE_KEYS:
        return "not a key of the onboarding profile"
    if _is_suspicious_value(value):
        return "the value looks like an instruction, not an answer"
    return None


def normalise_select_value(key: str, value: str) -> str | None:
    """Map a closed-vocabulary value to its canonical option, else ``None``.

    Free-text keys pass through unchanged. For the two calibration choices,
    a value in the person's own words ("local par défaut", "critical
    actions") is mapped by :func:`heuristic_value` when it is not already
    canonical.
    """
    canonical = value.strip().lower()
    if key == "user.agents.hitl":
        return canonical if canonical in VALID_HITL else heuristic_value(key, value)
    if key == "user.constraints.sovereignty":
        return canonical if canonical in VALID_SOVEREIGNTY else heuristic_value(key, value)
    if key == "user.tech.proficiency":
        synonyms = {
            "beginner": "debutant",
            "novice": "debutant",
            "débutant": "debutant",
            "debutante": "debutant",
            "intermediate": "intermediaire",
            "intermédiaire": "intermediaire",
            "moyen": "intermediaire",
            "advanced": "avance",
            "avancé": "avance",
            "confirmé": "avance",
            "confirme": "avance",
        }
        mapped = synonyms.get(canonical, canonical)
        return mapped if mapped in VALID_PROFICIENCY else None
    if key == "user.preferences.language":
        synonyms = {
            "french": "fr",
            "français": "fr",
            "francais": "fr",
            "fr": "fr",
            "english": "en",
            "anglais": "en",
            "en": "en",
        }
        mapped = synonyms.get(canonical)
        return mapped if mapped in VALID_LANGUAGE else None
    return value.strip()


def heuristic_value(key: str, text: str) -> str | None:
    """Best-effort mapping of a calibration answer given in plain words.

    Small local models often pass the person's own wording instead of the
    canonical token. Matching goes from the most specific to the most generic
    so one keyword wins.
    """
    low = text.lower()
    tokens = {tok.strip(".,;:!?()") for tok in low.split()}

    if key == "user.agents.hitl":
        if any(
            w in low for w in ("critical-only", "critique", "critical", "important", "sensible")
        ):
            return "critical-only"
        if "2" in tokens or any(w in low for w in ("(2)", "deuxième", "deuxieme", "second")):
            return "critical-only"
        never_words = (
            "never",
            "jamais",
            "autonomie",
            "autonome",
            "autonomous",
            "sans valider",
            "sans confirmation",
            "laisser faire",
        )
        if any(w in low for w in never_words):
            return "never"
        if "3" in tokens or any(w in low for w in ("(3)", "troisième", "troisieme", "third")):
            return "never"
        always_words = (
            "always",
            "toujours",
            "valider tout",
            "valider tous",
            "tout valider",
            "chaque action",
        )
        if any(w in low for w in always_words):
            return "always"
        if "1" in tokens or any(w in low for w in ("(1)", "première", "premiere", "first")):
            return "always"
        return None

    if key == "user.constraints.sovereignty":
        preferred_words = (
            "local par défaut",
            "local par defaut",
            "local préféré",
            "local prefere",
            "local preferred",
            "local-preferred",
            "préférer local",
            "preferer local",
            "local en priorité",
            "local en priorite",
            "local d'abord",
            "local d abord",
        )
        if any(w in low for w in preferred_words):
            return "local-preferred"
        cloud_words = (
            "cloud ok",
            "cloud-ok",
            "cloud autorisé",
            "cloud autorise",
            "cloud accepté",
            "cloud accepte",
            "openai",
            "anthropic",
            "apis cloud",
            "api cloud",
            "cloud d'accord",
            "cloud allowed",
        )
        if any(w in low for w in cloud_words):
            return "cloud-ok"
        only_words = (
            "local-only",
            "local only",
            "local uniquement",
            "local seulement",
            "tout en local",
            "que en local",
            "rien sortir",
            "rester local",
            "reste local",
            "machine locale uniquement",
            "strictement local",
        )
        if any(w in low for w in only_words):
            return "local-only"
        if " local" in f" {low}":
            # A bare "local" reads as the strictest choice.
            return "local-only"
        return None

    return None


# ---------------------------------------------------------------------------
# Profile type and suggestions
# ---------------------------------------------------------------------------

_TECH_KEYWORDS: tuple[str, ...] = (
    "dev",
    "développeur",
    "developpeur",
    "developer",
    "engineer",
    "ingénieur",
    "ingenieur",
    "data",
    "scientist",
    "analyst",
    "analyste",
    "ml",
    "machine learning",
    "ai",
    "ia",
    "devops",
    "sre",
    "backend",
    "frontend",
    "fullstack",
    "full-stack",
    "architect",
    "architecte",
    "programmer",
    "coder",
)

_RSE_KEYWORDS: tuple[str, ...] = (
    "rse",
    "esg",
    "sustainability",
    "durabilité",
    "durabilite",
)


def infer_profile_type(role: str | None) -> str:
    """Return ``operator`` or ``builder``; an empty role reads as operator."""
    if not role:
        return "operator"
    low = role.lower()
    if any(kw in low for kw in _TECH_KEYWORDS):
        return "builder"
    return "operator"


def suggested_agents_for(role: str | None) -> list[str]:
    """The agents suggested at closure when the agent names none."""
    agents = (
        ["veille-ia", "email-triage"] if infer_profile_type(role) == "builder" else ["email-triage"]
    )
    if role and any(kw in role.lower() for kw in _RSE_KEYWORDS):
        agents.append("veille-rse")
    return agents


def proposed_rules(
    sovereignty: str | None, hitl: str | None, integrations_raw: str
) -> list[dict[str, Any]]:
    """The permission rules matching the collected preferences.

    Nothing here creates a rule: the list is stored, the desktop shows each
    rule as a card, and the person creates it or not. The tool holds no
    access to the permission rules.

    Sovereignty
      local-only       deny http_fetch on https:// and http://
      local-preferred  deny http_fetch on the cloud LLM endpoints
      cloud-ok         no network rule
    Supervision
      always           no allow rule
      critical-only    allow file_read
      never            allow file_read (never a wildcard allow)
    Integrations
      github, slack, notion, gmail: allow http_fetch on the matching API
    """
    integrations = {item.strip().lower() for item in integrations_raw.split(",") if item.strip()}
    proposals: list[dict[str, Any]] = []
    if sovereignty == "local-only":
        for prefix in ("https://", "http://"):
            proposals.append(
                {
                    "tool_name": "http_fetch",
                    "action": "deny",
                    "arg_prefix": prefix,
                    "scope": "global",
                }
            )
    elif sovereignty == "local-preferred":
        for endpoint in ("https://api.openai.com", "https://api.anthropic.com"):
            proposals.append(
                {
                    "tool_name": "http_fetch",
                    "action": "deny",
                    "arg_prefix": endpoint,
                    "scope": "global",
                }
            )
    if hitl in {"critical-only", "never"}:
        proposals.append({"tool_name": "file_read", "action": "allow", "scope": "global"})
    for name, endpoint in INTEGRATION_ENDPOINTS.items():
        if name in integrations:
            proposals.append(
                {
                    "tool_name": "http_fetch",
                    "action": "allow",
                    "arg_prefix": endpoint,
                    "scope": "global",
                }
            )
    return proposals


# ---------------------------------------------------------------------------
# Stores
# ---------------------------------------------------------------------------


async def _profile_value(ctx: Ctx, key: str) -> str | None:
    try:
        value = await ctx.profile.get(key)
    except Exception:
        return None
    return value or None


async def _remember(ctx: Ctx, key: str, value: str) -> None:
    await ctx.memory.remember(
        key=key,
        value=value,
        source=MEMORY_SOURCE,
        confidence=CONFIDENCE_EXPLICIT,
    )


async def _completed(ctx: Ctx) -> bool:
    try:
        return bool(await ctx.memory.recall("onboarding.completed_at"))
    except Exception:
        return False


async def _finalize(ctx: Ctx, profile_type: str, suggested: list[str]) -> None:
    """Write the meta keys in strict order, ``completed_at`` last."""
    role = await _profile_value(ctx, "user.role")
    kind = profile_type if profile_type in {"operator", "builder"} else infer_profile_type(role)
    await _remember(ctx, "onboarding.profile_type", kind)
    await _remember(ctx, "onboarding.version", ONBOARDING_VERSION)
    agents = suggested if suggested else suggested_agents_for(role)
    await _remember(ctx, "onboarding.suggested_agents", json.dumps(agents))
    rules = proposed_rules(
        await _profile_value(ctx, "user.constraints.sovereignty"),
        await _profile_value(ctx, "user.agents.hitl"),
        await _profile_value(ctx, "user.tech.integrations") or "",
    )
    await _remember(ctx, "onboarding.proposed_rules", json.dumps(rules))
    await _remember(ctx, "onboarding.completed_at", datetime.now(timezone.utc).isoformat())


def _next_step(missing: list[str], optional_left: list[str], completed: bool) -> str:
    """What the agent does next, from the stored state."""
    if completed:
        return (
            "Onboarding is complete. Summarize in two sentences what you understood, "
            "point the person to the agent catalog in /agents, and say that a few "
            "permission rules matching their choices are proposed next, each with a "
            "confirmation."
        )
    if missing:
        return (
            NEXT_QUESTION[missing[0]]
            + " Do not close the onboarding: the four calibration facts are not all recorded."
        )
    if not optional_left:
        return "Every fact is recorded: call propose_profile with close=true."
    topics = "; ".join(f"{key} ({TIER2_LABELS[key]})" for key in optional_left[:2])
    return (
        "Calibration is complete. Offer the next optional topic(s), one or two per turn, "
        f"and say they can be skipped: {topics}. When the person asks to stop, or after "
        "the optional topics, call propose_profile with close=true."
    )


# ---------------------------------------------------------------------------
# Tool
# ---------------------------------------------------------------------------


@tool(
    "propose_profile",
    "Record the profile facts the person just gave in the onboarding conversation, "
    "and close the onboarding when asked. Returns what is recorded, what is still "
    "missing, and what to ask next.",
    action="write_or_delete",
    examples=[
        {"facts": {"user.name": "Lea", "user.role": "office manager"}},
        {"facts": {"user.agents.hitl": "critical-only"}},
        {"facts": {}, "close": True},
    ],
)
async def propose_profile(
    ctx: Ctx,
    facts: Annotated[
        dict[str, str],
        "Profile facts the person gave this turn, key to value, e.g. "
        '{"user.name": "Lea"}. Only what they actually said; empty when nothing new.',
    ],
    close: Annotated[
        bool,
        "True when the person asked to stop or the conversation is over.",
    ] = False,
    profile_type: Annotated[
        str,
        "operator or builder, when you can tell; empty otherwise.",
    ] = "",
    suggested_agents: Annotated[
        list[str] | None,
        "Agent identifiers to suggest at closure; leave out to let the tool choose.",
    ] = None,
) -> dict[str, Any]:
    """Record ``facts``, finalize when the end has come, return the state."""
    recorded: dict[str, str] = {}
    refused: dict[str, str] = {}
    for raw_key, raw_value in (facts or {}).items():
        key = normalise_key(str(raw_key))
        value = str(raw_value)
        reason = refusal_reason(key, value)
        if reason is not None:
            refused[key] = reason
            continue
        normalised = normalise_select_value(key, value)
        if normalised is None:
            refused[key] = "not one of the accepted choices"
            continue
        await ctx.profile.set(key=key, value=_truncate(normalised))
        recorded[key] = normalised

    missing = [key for key in TIER1_KEYS if not await _profile_value(ctx, key)]
    optional_left = [key for key in TIER2_KEYS if not await _profile_value(ctx, key)]
    completed = await _completed(ctx)
    if not completed and not missing and (close or not optional_left):
        await _finalize(ctx, profile_type.strip().lower(), list(suggested_agents or []))
        completed = True

    return {
        "recorded": recorded,
        "refused": refused,
        "missing_calibration": missing,
        "optional_remaining": optional_left,
        "completed": completed,
        "next": _next_step(missing, optional_left, completed),
    }
