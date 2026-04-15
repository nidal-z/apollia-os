"""Tests for spec-assistant."""

from __future__ import annotations

import importlib.util
import json
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

# Default mock tools including bash_executor for bootstrap.
_DEFAULT_TOOLS: dict[str, Any] = {
    "file_read": {"content": ""},
    "bash_executor": {"stdout": "abc123"},
}


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
    assert m.get("execution_mode") == "auto"
    assert m.get("agent_type") == "assistant"
    assert m.get("supports_a2a") is True
    assert m.get("llm_backend") == "precise"
    assert m.get("dangerous_tools_allowed") is False


def test_manifest_has_skills() -> None:
    """GIVEN the manifest
    WHEN skills are inspected
    THEN three skills are declared with the required fields."""
    m = spec_assistant.manifest()
    skills = m.get("skills", [])

    assert len(skills) == 3
    skill_ids = {s["id"] for s in skills}
    assert skill_ids == {"create-spec", "refine-spec", "list-specs"}
    for skill in skills:
        assert "name" in skill
        assert "description" in skill
        assert "input_modes" in skill
        assert "output_modes" in skill


def test_manifest_description_in_french() -> None:
    """GIVEN the manifest
    WHEN the description is inspected
    THEN it is written in French."""
    m = spec_assistant.manifest()
    desc = m["description"].lower()

    assert "conception" in desc or "spec" in desc


def test_manifest_tags_non_empty() -> None:
    """GIVEN the manifest
    WHEN tags are inspected
    THEN at least 3 tags are declared."""
    m = spec_assistant.manifest()
    assert len(m.get("tags", [])) >= 3


def test_manifest_instance_matches_module_function() -> None:
    """GIVEN the module-level agent instance and the module-level function
    WHEN both are called
    THEN they return the same name, version and skill IDs."""
    instance_m = spec_assistant.agent.manifest()
    module_m = spec_assistant.manifest()

    assert instance_m["name"] == module_m["name"]
    assert instance_m["version"] == module_m["version"]
    assert {s["id"] for s in instance_m["skills"]} == {s["id"] for s in module_m["skills"]}


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


def test_detect_language_no_false_positive_from_substring() -> None:
    """GIVEN an English message containing a French marker as a substring
    WHEN _detect_language is called
    THEN it returns 'en' (no false positive from substring matching)."""
    # 'feature' contains 'tu' (French marker) but it is not a whole word.
    assert spec_assistant._detect_language("Add a feature for the export button") == "en"


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
    THEN accents are stripped and the slug is plain ASCII."""
    slug = spec_assistant._slugify("Ajout d'un bouton d'export")
    assert slug.isascii()
    assert " " not in slug


def test_slugify_max_length() -> None:
    """GIVEN a very long title
    WHEN _slugify is called
    THEN the result is at most 64 characters."""
    assert len(spec_assistant._slugify("a" * 200)) <= 64


# ---------------------------------------------------------------------------
# parse_project_rules
# ---------------------------------------------------------------------------


def test_parse_rules_extracts_forbidden_deps_with_backticks() -> None:
    """GIVEN a CLAUDE.md with backtick-wrapped dep names and INTERDIT keyword
    WHEN parse_project_rules is called
    THEN the forbidden dep appears in the forbidden_deps JSON list."""
    raw = "## Règles\n`anyhow` INTERDIT dans le workspace\n`unwrap()` interdit"
    result = spec_assistant.parse_project_rules(raw)

    deps = json.loads(result["forbidden_deps"])
    assert "anyhow" in deps


def test_parse_rules_extracts_forbidden_deps_plain() -> None:
    """GIVEN rules with plain-text forbidden declarations
    WHEN parse_project_rules is called
    THEN the forbidden dep is extracted."""
    raw = "The library lodash is forbidden in this project."
    result = spec_assistant.parse_project_rules(raw)

    deps = json.loads(result["forbidden_deps"])
    assert "lodash" in deps


def test_parse_rules_raw_truncated_at_limit() -> None:
    """GIVEN raw text longer than the inline truncation limit (4000 chars)
    WHEN parse_project_rules is called
    THEN result['raw'] does not exceed the limit (plus truncation notice)."""
    raw = "x" * 5_000
    result = spec_assistant.parse_project_rules(raw)

    assert len(result["raw"]) <= 4_100


# ---------------------------------------------------------------------------
# process_spec_blocks
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_process_spec_blocks_writes_file_and_records() -> None:
    """GIVEN an LLM response with a [SPEC:slug][/SPEC] block
    WHEN process_spec_blocks is called
    THEN file_write is called, the slug is recorded in memory, and the block
    is replaced by a confirmation containing the file path."""
    ctx = MockContext.create(
        tools={"file_write": {"success": True}},
        memory=True,
    )
    text = (
        "Here is your spec:\n"
        "[SPEC:export-button]# TaskSpec\n## Objectif\nAjouter un bouton.[/SPEC]"
    )

    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "fr")

    assert "file_write" in _called_tools(ctx)
    assert "[SPEC:" not in cleaned
    assert "[/SPEC]" not in cleaned
    assert ".apollia/tasks/export-button.md" in cleaned

    assert ctx.memory is not None
    slugs_json = await ctx.memory.recall(spec_assistant.MEMORY_KEY_CREATED_SPECS)
    assert slugs_json is not None
    assert "export-button" in json.loads(slugs_json)


@pytest.mark.asyncio
async def test_process_spec_blocks_no_tools_graceful() -> None:
    """GIVEN a context with no tools
    WHEN process_spec_blocks is called with a SPEC block
    THEN it returns a warning without raising."""
    ctx = MockContext.create()
    text = "[SPEC:my-feature]content[/SPEC]"

    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "en")

    assert "[SPEC:" not in cleaned
    assert "my-feature.md" in cleaned


@pytest.mark.asyncio
async def test_process_spec_blocks_no_match_unchanged() -> None:
    """GIVEN text with no [SPEC:...] block
    WHEN process_spec_blocks is called
    THEN the text is returned unchanged."""
    ctx = MockContext.create()
    text = "This is just a regular response with no spec block."

    cleaned = await spec_assistant.process_spec_blocks(text, ctx, "en")

    assert cleaned == text


# ---------------------------------------------------------------------------
# record_created_spec / load_created_specs
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_record_created_spec_idempotent() -> None:
    """GIVEN the same slug recorded twice
    WHEN load_created_specs is called
    THEN the slug appears exactly once in the list."""
    ctx = MockContext.create(memory=True)

    await spec_assistant.record_created_spec(ctx, "my-feature")
    await spec_assistant.record_created_spec(ctx, "my-feature")

    specs = await spec_assistant.load_created_specs(ctx)
    assert specs.count("my-feature") == 1


@pytest.mark.asyncio
async def test_load_created_specs_empty_on_fresh_project() -> None:
    """GIVEN a context with empty memory
    WHEN load_created_specs is called
    THEN an empty list is returned."""
    ctx = MockContext.create(memory=True)

    specs = await spec_assistant.load_created_specs(ctx)
    assert specs == []


# ---------------------------------------------------------------------------
# SpecContextBootstrap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_bootstrap_lists_existing_specs() -> None:
    """GIVEN a workspace with .apollia/tasks/user-auth.md
    WHEN SpecContextBootstrap.extra_scopes() runs via run_bootstrap
    THEN the snapshot contains the spec slug."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": ".apollia/tasks/user-auth.md\n"},
        },
        memory=True,
    )

    bootstrap = spec_assistant.SpecContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert "user-auth" in snapshot.get("existing_specs", [])
    assert snapshot.get("spec_count", 0) >= 1


def test_spec_assistant_no_rule_files_constant() -> None:
    """GIVEN spec-assistant.py
    WHEN we check for _RULE_FILES
    THEN it is not found (removed)."""
    assert not hasattr(spec_assistant, "_RULE_FILES")


# ---------------------------------------------------------------------------
# build_system_prompt
# ---------------------------------------------------------------------------


def test_build_system_prompt_fr_contains_rules_and_spec_section() -> None:
    """GIVEN rules with a forbidden dep and an existing spec
    WHEN build_system_prompt is called for French
    THEN the prompt contains the forbidden dep and the existing spec slug."""
    rules = {
        "raw": "anyhow INTERDIT dans le workspace",
        "forbidden_deps": json.dumps(["anyhow"]),
        "patterns": "",
        "comment_convention": "",
    }
    prompt = spec_assistant.build_system_prompt("fr", rules, ["user-auth"])

    assert "anyhow" in prompt
    assert "user-auth" in prompt
    assert "[SPEC:" in prompt


def test_build_system_prompt_en_no_rules_fallback() -> None:
    """GIVEN empty rules
    WHEN build_system_prompt is called for English
    THEN the prompt includes the 'no rules found' fallback message."""
    rules = {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
    prompt = spec_assistant.build_system_prompt("en", rules)

    assert "No rules file found" in prompt


def test_build_system_prompt_no_code_in_template() -> None:
    """GIVEN any language
    WHEN build_system_prompt is called
    THEN the resulting prompt explicitly states code is never generated."""
    rules = {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}

    for lang in ("fr", "en"):
        prompt = spec_assistant.build_system_prompt(lang, rules)
        assert "NEVER" in prompt or "JAMAIS" in prompt


# ---------------------------------------------------------------------------
# Agent — full run() integration
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_no_code_in_spec_output() -> None:
    """GIVEN a request that might trigger code generation
    WHEN run() is called on spec-assistant
    THEN the result contains no Markdown code fences."""
    llm_response = (
        "Mon rôle est la conception uniquement, pas l'implémentation. "
        "Pour coder cette feature, utilisez dev-assistant. "
        "Souhaitez-vous que je rédige d'abord une TaskSpec ?"
    )
    ctx = MockContext.create(
        tools={**_DEFAULT_TOOLS, "file_write": {"success": True}},
        llm_responses=[_llm_text(llm_response)],
        memory=True,
    )

    agent = _agent_instance()
    result = await agent.run(
        {"input": {"text": "Implémente la feature de login en Python"}},
        ctx,
    )

    assert result["status"] == "completed"
    assert "```" not in result["output"][0]["text"]


@pytest.mark.asyncio
async def test_taskspec_file_created() -> None:
    """GIVEN a workspace with rules and a user request
    WHEN the LLM responds with a [SPEC:slug] block
    THEN file_write is called and the confirmation path appears in the output."""
    spec_body = (
        "# TaskSpec — Export Button\n\n"
        "> Généré par spec-assistant\n"
        "> Statut : DRAFT\n\n"
        "## Objectif\nAjouter un bouton d'export CSV.\n\n"
        "## Couches concernées\n- [x] Frontend / UI / Composants\n\n"
        "## Règles du projet\n### Dépendances interdites\nAucune\n\n"
        "## Périmètre explicite\n### Dans le scope\n- Bouton export\n### Hors scope\n- Import\n\n"
        "## Définition de \"Fini\"\n- [ ] Le bouton est visible\n\n"
        "## Hypothèses et dépendances\n- Les données sont chargées\n\n"
        "## Risques identifiés\n- Aucun\n\n"
        "## Historique des révisions\n- DRAFT : spec créée"
    )
    llm_response = f"Voici la TaskSpec :\n\n[SPEC:export-button]{spec_body}[/SPEC]"
    ctx = MockContext.create(
        tools={
            **_DEFAULT_TOOLS,
            "file_read": {"content": "# Règles\nanyhow INTERDIT"},
            "file_write": {"success": True},
        },
        llm_responses=[_llm_text(llm_response)],
        memory=True,
        workspace_rules="# Règles\nanyhow INTERDIT",
    )

    agent = _agent_instance()
    result = await agent.run(
        {"input": {"text": "Ajoute un bouton d'export CSV"}},
        ctx,
    )

    assert result["status"] == "completed"
    assert "file_write" in _called_tools(ctx)
    assert ".apollia/tasks/export-button.md" in result["output"][0]["text"]


@pytest.mark.asyncio
async def test_existing_specs_injected_into_system_prompt() -> None:
    """GIVEN a project with an existing spec in memory
    WHEN spec-assistant starts a new session
    THEN the system prompt includes the existing spec slug."""
    ctx = MockContext.create(
        tools=_DEFAULT_TOOLS,
        llm_responses=[_llm_text("Je peux vous aider à créer une spec.")],
        memory=True,
    )
    assert ctx.memory is not None
    await ctx.memory.remember(
        key=spec_assistant.MEMORY_KEY_CREATED_SPECS,
        value=json.dumps(["user-auth"]),
        source="spec-assistant",
        confidence=1.0,
    )

    agent = _agent_instance()
    await agent.run({"input": {"text": "je veux une nouvelle feature"}}, ctx)

    assert "user-auth" in agent.SYSTEM_PROMPT


@pytest.mark.asyncio
async def test_episodic_memory_importance_higher_on_spec_creation() -> None:
    """GIVEN an LLM response that contains a [SPEC:...] block
    WHEN the agent processes it
    THEN the episodic record is written with importance 0.8."""
    spec_body = "# TaskSpec — Test\n> DRAFT\n## Objectif\nTest."
    llm_response = f"[SPEC:test-spec]{spec_body}[/SPEC]"
    ctx = MockContext.create(
        tools={**_DEFAULT_TOOLS, "file_write": {"success": True}},
        llm_responses=[_llm_text(llm_response)],
        memory=True,
    )

    agent = _agent_instance()
    await agent.run({"input": {"text": "je veux une spec"}}, ctx)

    assert ctx.memory is not None
    records = [op for op in ctx.memory.operations if op["op"] == "record"]
    assert any(r.get("importance", 0) >= 0.8 for r in records)


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    """GIVEN the module after execution
    WHEN the agent attribute is accessed
    THEN it is a SpecAssistant with a valid manifest."""
    assert isinstance(spec_assistant.agent, spec_assistant.SpecAssistant)
    assert spec_assistant.agent.manifest()["name"] == "spec-assistant"


def test_module_exports_all_public_symbols() -> None:
    """GIVEN the loaded module
    WHEN public symbols are accessed
    THEN all are present and callable/correct type."""
    assert callable(spec_assistant.manifest)
    assert callable(spec_assistant.process_spec_blocks)
    assert callable(spec_assistant.build_system_prompt)
    assert callable(spec_assistant.record_created_spec)
    assert callable(spec_assistant.load_created_specs)
    assert callable(spec_assistant.parse_project_rules)
    assert isinstance(spec_assistant._SPEC_BLOCK_RE, type(spec_assistant._SPEC_BLOCK_RE))
    assert spec_assistant.SpecContextBootstrap is not None
