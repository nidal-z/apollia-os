"""Tests for the heuristic value recovery in onboarding-agent v2.2.

When the LLM omits or mangles a `[REMEMBER user.agents.hitl=...]` tag,
the agent falls back to scanning the user's raw reply. Without this
fallback the agent re-asks the same question forever (cf. plan v2 §1
"Q3 loop" finding).
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest


_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "system" / "onboarding-agent" / "agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent_v22_recover", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _mod
_spec.loader.exec_module(_mod)

recover = _mod._heuristic_value_from_user_text


# ── HITL ────────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "text,expected",
    [
        ("toujours valider", "always"),
        ("Toujours", "always"),
        ("(1)", "always"),
        ("option 1", "always"),
        ("critique seulement", "critical-only"),
        ("juste les actions critiques", "critical-only"),
        ("(2)", "critical-only"),
        ("option 2", "critical-only"),
        ("jamais", "never"),
        ("never", "never"),
        ("autonomie complète", "never"),
        ("laisser faire", "never"),
        ("(3)", "never"),
    ],
)
def test_recover_hitl(text: str, expected: str) -> None:
    assert recover("user.agents.hitl", text) == expected


def test_recover_hitl_returns_none_on_garbage() -> None:
    assert recover("user.agents.hitl", "bonjour Apollia") is None


# ── Sovereignty ─────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "text,expected",
    [
        ("local par défaut", "local-preferred"),
        ("local préféré", "local-preferred"),
        ("local d'abord", "local-preferred"),
        ("préférer local", "local-preferred"),
        ("cloud OK", "cloud-ok"),
        ("cloud autorisé", "cloud-ok"),
        ("OpenAI ou Anthropic c'est bien", "cloud-ok"),
        ("apis cloud sont OK", "cloud-ok"),
        ("local uniquement", "local-only"),
        ("local seulement", "local-only"),
        ("tout en local", "local-only"),
        ("local-only", "local-only"),
        ("local", "local-only"),
    ],
)
def test_recover_sovereignty(text: str, expected: str) -> None:
    assert recover("user.constraints.sovereignty", text) == expected


def test_recover_sovereignty_returns_none_on_garbage() -> None:
    assert recover("user.constraints.sovereignty", "j'aime les chats") is None


# ── Other keys are not handled by this helper ──────────────────────────


def test_recover_returns_none_for_unsupported_key() -> None:
    assert recover("user.name", "Nidal") is None
    assert recover("user.role", "CTO") is None
