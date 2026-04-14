"""Tests for spec-assistant."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup — must run before any apollia import
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.testing import MockContext  # noqa: E402

# Load spec-assistant via importlib (hyphen in filename prevents direct import).
_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "spec-assistant.py"
_spec_module = importlib.util.spec_from_file_location("spec_assistant", _AGENT_PATH)
assert _spec_module is not None and _spec_module.loader is not None
spec_assistant = importlib.util.module_from_spec(_spec_module)
_spec_module.loader.exec_module(spec_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _llm_text(text: str) -> dict[str, str]:
    """Return a MockLlm response dict with *text* as content."""
    return {"content": text}


def _agent_instance() -> Any:
    """Return a fresh SpecAssistant instance for each test."""
    return spec_assistant.SpecAssistant()


def _called_tools(ctx: Any) -> list[str]:
    """Return the ordered list of tool names called on *ctx*."""
    assert ctx.tools is not None
    return [name for name, _ in ctx.tools.calls]


# ---------------------------------------------------------------------------
# Manifest validation
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    """GIVEN spec-assistant
    WHEN the manifest is loaded
    THEN required fields are present and have correct values."""
    m = spec_assistant.manifest()

    assert m["name"] == "spec-assistant"
    assert m["version"] == "1.0.0"
    assert m["memory_namespace"] == "spec-assistant"
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]
    assert m.get("execution_mode") == "conversational"
    assert m.get("supports_a2a") is True
    assert m.get("dangerous_tools_allowed") is False


def test_manifest_instance_method_matches_module_function() -> None:
    """GIVEN the module-level agent instance
    WHEN manifest() is called on the instance vs the module
    THEN both return the same name and version."""
    instance_m = spec_assistant.agent.manifest()
    module_m = spec_assistant.manifest()

    assert instance_m["name"] == module_m["name"]
    assert instance_m["version"] == module_m["version"]


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------


def test_detect_language_french() -> None:
    """GIVEN a clearly French message
    WHEN _detect_language is called
    THEN it returns 'fr'."""
    assert spec_assistant._detect_language("Bonjour, je voudrais une spec") == "fr"


def test_detect_language_english() -> None:
    """GIVEN a clearly English message
    WHEN _detect_language is called
    THEN it returns 'en'."""
    assert spec_assistant._detect_language("Create a spec for the login feature") == "en"


# ---------------------------------------------------------------------------
# Slugify
# ---------------------------------------------------------------------------


def test_slugify_basic() -> None:
    """GIVEN a title with spaces and uppercase
    WHEN _slugify is called
    THEN the result is lowercase and hyphen-separated."""
    assert spec_assistant._slugify("User Authentication") == "user-authentication"


def test_slugify_accents() -> None:
    """GIVEN a title with French accents
    WHEN _slugify is called
    THEN accents are stripped and slug is ASCII."""
    slug = spec_assistant._slugify("Ajout d'un bouton d'export")
    assert slug.isascii()
    assert " " not in slug


def test_slugify_max_length() -> None:
    """GIVEN a very long title
    WHEN _slugify is called
    THEN the result is at most 64 characters."""
    long_title = "a" * 200
    assert len(spec_assistant._slugify(long_title)) <= 64


# ---------------------------------------------------------------------------
# parse_project_rules
# ---------------------------------------------------------------------------


def test_parse_rules_extracts_forbidden_deps() -> None:
    """GIVEN a CLAUDE.md with an INTERDIT keyword
    WHEN parse_project_rules is called
    THEN the forbidden dep appears in the forbidden_deps JSON list."""
    import json

    raw = "## Règles\n`anyhow` INTERDIT dans le workspace\n`unwrap()` interdit"
    result = spec_assistant.parse_project_rules(raw)

    deps = json.loads(result["forbidden_deps"])
    assert "anyhow" in deps


def test_parse_rules_raw_truncated_at_limit() -> None:
    """GIVEN raw text longer than _MAX_RULES_CHARS
    WHEN parse_project_rules is called
    THEN result['raw'] does not exceed the limit."""
    raw = "x" * (spec_assistant._MAX_RULES_CHARS + 1_000)
    result = spec_assistant.parse_project_rules(raw)

    assert len(result["raw"]) <= spec_assistant._MAX_RULES_CHARS + 100


# ---------------------------------------------------------------------------
# process_spec_blocks
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_process_spec_blocks_writes_file() -> None:
    """GIVEN an LLM response with a [SPEC:slug][/SPEC] block
    WHEN process_spec_blocks is called
    THEN file_write is called with the correct path."""
    ctx = MockContext.create(
        tools={"file_write": {"success": True}},
    )
    text = "Here is your spec:\n[SPEC:export-button]# TaskSpec\n## Objectif\nAjouter un bouton.[/SPEC]"

    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "fr")

    assert "file_write" in _called_tools(ctx)
    assert "[SPEC:" not in cleaned
    assert "[/SPEC]" not in cleaned
    assert ".apollia/tasks/export-button.md" in cleaned


@pytest.mark.asyncio
async def test_process_spec_blocks_no_tools_graceful() -> None:
    """GIVEN a context with no tools
    WHEN process_spec_blocks is called with a SPEC block
    THEN it returns a warning message without raising."""
    ctx = MockContext.create()

    text = "[SPEC:my-feature]content[/SPEC]"
    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "en")

    assert "[SPEC:" not in cleaned
    assert "my-feature.md" in cleaned


@pytest.mark.asyncio
async def test_process_spec_blocks_no_match_returns_unchanged() -> None:
    """GIVEN text with no [SPEC:...] block
    WHEN process_spec_blocks is called
    THEN the text is returned unchanged."""
    ctx = MockContext.create()
    text = "This is just a regular response with no spec block."

    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "en")

    assert cleaned == text


# ---------------------------------------------------------------------------
# load_project_rules — memory hit
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_load_project_rules_returns_from_memory() -> None:
    """GIVEN memory already containing project_rules
    WHEN load_project_rules is called
    THEN file_read is NOT called (memory cache is used)."""
    ctx = MockContext.create(
        tools={"file_read": {"content": "should not be read"}},
        memory=True,
    )
    assert ctx.memory is not None
    await ctx.memory.remember(
        key=spec_assistant.MEMORY_KEY_PROJECT_RULES,
        value="anyhow INTERDIT dans ce projet",
        source="test",
        confidence=0.9,
    )

    rules = await spec_assistant.load_project_rules(ctx)

    assert "anyhow INTERDIT" in rules["raw"]
    assert ctx.tools is not None
    file_read_calls = [n for n, _ in ctx.tools.calls if n == "file_read"]
    assert len(file_read_calls) == 0


@pytest.mark.asyncio
async def test_load_project_rules_reads_files_when_no_memory() -> None:
    """GIVEN no cached rules in memory but CLAUDE.md present in workspace
    WHEN load_project_rules is called
    THEN file_read is called and the rules are returned."""
    claude_md = "# Rules\nanyhow INTERDIT\nzero unwrap()"
    ctx = MockContext.create(
        tools={
            "file_read": {"content": claude_md},
        },
        memory=True,
    )

    rules = await spec_assistant.load_project_rules(ctx)

    assert rules["raw"] != ""
    assert ctx.tools is not None
    assert ctx.tools.tool_call_count() > 0


@pytest.mark.asyncio
async def test_load_project_rules_empty_when_no_sources() -> None:
    """GIVEN no memory and no workspace files (all return empty content)
    WHEN load_project_rules is called
    THEN the returned dict has empty 'raw' and '[]' for forbidden_deps."""
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}},
        memory=True,
    )

    rules = await spec_assistant.load_project_rules(ctx)

    assert rules["raw"] == ""
    assert rules["forbidden_deps"] == "[]"


# ---------------------------------------------------------------------------
# build_system_prompt
# ---------------------------------------------------------------------------


def test_build_system_prompt_fr_contains_rules() -> None:
    """GIVEN rules with a forbidden dep
    WHEN build_system_prompt is called for French
    THEN the prompt mentions the forbidden dep."""
    import json

    rules = {
        "raw": "anyhow INTERDIT dans le workspace",
        "forbidden_deps": json.dumps(["anyhow"]),
        "patterns": "",
        "comment_convention": "",
    }
    prompt = spec_assistant.build_system_prompt("fr", rules)

    assert "anyhow" in prompt
    assert "[SPEC:" in prompt


def test_build_system_prompt_en_no_rules_message() -> None:
    """GIVEN empty rules
    WHEN build_system_prompt is called for English
    THEN the prompt includes the 'no rules found' fallback message."""
    rules = {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
    prompt = spec_assistant.build_system_prompt("en", rules)

    assert "No rules file found" in prompt


# ---------------------------------------------------------------------------
# Agent — full run() integration
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_no_code_in_spec_output() -> None:
    """GIVEN a request that might trigger code generation
    WHEN run() is called on spec-assistant
    THEN the result does not contain Markdown code fences."""
    llm_response = (
        "Mon rôle est la conception uniquement, pas l'implémentation. "
        "Pour coder cette feature, utilisez dev-assistant. "
        "Souhaitez-vous que je rédige d'abord une TaskSpec ?"
    )
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
        },
        llm_responses=[_llm_text(llm_response)],
        memory=True,
    )

    agent = _agent_instance()
    result = await agent.run(
        {"input": {"text": "Implémente la feature de login en Python"}},
        ctx,
    )

    assert result["status"] == "completed"
    output_text = result["output"][0]["text"]
    assert "```" not in output_text


@pytest.mark.asyncio
async def test_taskspec_file_created() -> None:
    """GIVEN a workspace with CLAUDE.md and a user request
    WHEN the LLM responds with a [SPEC:slug] block
    THEN file_write is called and the task spec path appears in the output."""
    spec_body = (
        "# TaskSpec — Export Button\n\n"
        "> Généré par spec-assistant\n"
        "> Statut : DRAFT\n\n"
        "## Objectif\nAjouter un bouton d'export CSV sur le tableau de bord.\n\n"
        "## Couches concernées\n- [x] Frontend / UI / Composants\n\n"
        "## Règles du projet\n### Dépendances interdites\nAucune\n\n"
        "## Périmètre explicite\n### Dans le scope\n- Bouton export CSV\n\n"
        "## Définition de \"Fini\"\n- [ ] Le bouton est visible et fonctionnel\n\n"
        "## Historique\n- spec créée par spec-assistant"
    )
    llm_response = (
        f"Voici la TaskSpec pour le bouton d'export :\n\n"
        f"[SPEC:export-button]{spec_body}[/SPEC]"
    )
    ctx = MockContext.create(
        tools={
            "file_read": {"content": "# Règles\nanyhow INTERDIT"},
            "file_write": {"success": True},
        },
        llm_responses=[_llm_text(llm_response)],
        memory=True,
    )

    agent = _agent_instance()
    result = await agent.run(
        {"input": {"text": "Ajoute un bouton d'export CSV"}},
        ctx,
    )

    assert result["status"] == "completed"
    assert "file_write" in _called_tools(ctx)
    output_text = result["output"][0]["text"]
    assert ".apollia/tasks/export-button.md" in output_text


@pytest.mark.asyncio
async def test_memory_loaded_on_second_session() -> None:
    """GIVEN a session where rules were already persisted to memory
    WHEN load_project_rules is called with those rules pre-populated
    THEN file_read is NOT invoked (memory cache is authoritative)."""
    ctx = MockContext.create(
        tools={"file_read": {"content": "fallback — should not be read"}},
        memory=True,
    )
    assert ctx.memory is not None
    await ctx.memory.remember(
        key=spec_assistant.MEMORY_KEY_PROJECT_RULES,
        value="no-react INTERDIT dans ce projet frontend",
        source="spec-assistant",
        confidence=0.9,
    )
    await ctx.memory.remember(
        key=spec_assistant.MEMORY_KEY_FORBIDDEN_DEPS,
        value='["no-react"]',
        source="spec-assistant",
        confidence=0.9,
    )

    rules = await spec_assistant.load_project_rules(ctx)

    assert "no-react INTERDIT" in rules["raw"]
    assert ctx.tools is not None
    file_read_calls = [n for n, _ in ctx.tools.calls if n == "file_read"]
    assert len(file_read_calls) == 0


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    """GIVEN the module after execution
    WHEN the agent attribute is accessed
    THEN it is a SpecAssistant with a valid manifest."""
    assert isinstance(spec_assistant.agent, spec_assistant.SpecAssistant)
    assert spec_assistant.agent.manifest()["name"] == "spec-assistant"


def test_module_imports_without_error() -> None:
    """GIVEN the module is loaded
    WHEN key symbols are accessed
    THEN all are present and non-empty."""
    assert callable(spec_assistant.manifest)
    assert callable(spec_assistant.load_project_rules)
    assert callable(spec_assistant.persist_rules)
    assert callable(spec_assistant.process_spec_blocks)
    assert callable(spec_assistant.build_system_prompt)
    assert isinstance(spec_assistant._SPEC_BLOCK_RE, type(spec_assistant._SPEC_BLOCK_RE))
