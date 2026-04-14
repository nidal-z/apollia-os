"""Tests de dev-assistant."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup — doit s'exécuter avant tout import apollia
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.testing import MockContext  # noqa: E402

# Chargement via importlib (le tiret dans le nom interdit l'import direct)
_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "dev-assistant.py"
_dev_spec = importlib.util.spec_from_file_location("dev_assistant", _AGENT_PATH)
assert _dev_spec is not None and _dev_spec.loader is not None
dev_assistant = importlib.util.module_from_spec(_dev_spec)
_dev_spec.loader.exec_module(dev_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class MockA2A:
    """Mock minimal de l'interface A2A pour les tests unitaires."""

    def __init__(self, responses: dict[str, Any] | None = None) -> None:
        self.calls: list[dict[str, Any]] = []
        self.responses: dict[str, Any] = responses or {}

    async def call(
        self,
        agent: str,
        skill: str,
        input: dict[str, Any],
    ) -> dict[str, Any]:
        """Enregistre l'appel et retourne la réponse pré-configurée."""
        self.calls.append({"agent": agent, "skill": skill, "input": input})
        return self.responses.get(agent, {"output": f"[mock] {agent}/{skill} done"})

    def called_agents(self) -> list[str]:
        """Retourne la liste ordonnée des agents appelés."""
        return [c["agent"] for c in self.calls]


def _inject_a2a(ctx: Any, a2a: MockA2A) -> Any:
    """Injecte un mock A2A dans le contexte."""
    ctx.a2a = a2a
    return ctx


def _make_task(text: str, history: list[dict[str, str]] | None = None) -> dict[str, Any]:
    """Construit un dict de tâche au format runtime Apollia."""
    t: dict[str, Any] = {"input": {"text": text}}
    if history:
        t["history"] = history
    return t


def _agent() -> Any:
    """Retourne une instance fraîche de DevAssistant pour chaque test."""
    return dev_assistant.DevAssistant()


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    """GIVEN dev-assistant
    WHEN le manifest est chargé
    THEN tous les champs requis sont présents et corrects."""
    m = dev_assistant.manifest()

    assert m["name"] == "dev-assistant"
    assert m["version"] == "1.0.0"
    assert m["memory_namespace"] == "dev-assistant"
    assert "bash_executor" in m["tools_requiring_approval"]
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "conversational"
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]


def test_manifest_has_skills() -> None:
    """GIVEN le manifest
    WHEN les skills sont inspectés
    THEN deux skills sont déclarés avec les champs requis."""
    m = dev_assistant.manifest()
    skills = m.get("skills", [])

    assert len(skills) == 2
    skill_ids = {s["id"] for s in skills}
    assert skill_ids == {"implement", "explore"}
    for skill in skills:
        assert "name" in skill
        assert "description" in skill
        assert "input_modes" in skill
        assert "output_modes" in skill


def test_manifest_description_mentions_pipeline() -> None:
    """GIVEN le manifest
    WHEN la description est inspectée
    THEN elle mentionne le pipeline et le contrat pré-tâche."""
    m = dev_assistant.manifest()
    desc = m["description"].lower()

    assert "pipeline" in desc
    assert "taskspec" in desc or "task" in desc


def test_manifest_instance_matches_module_function() -> None:
    """GIVEN l'instance module-level et la fonction module-level
    WHEN les deux sont appelées
    THEN elles retournent le même nom et les mêmes skill IDs."""
    instance_m = dev_assistant.agent.manifest()
    module_m = dev_assistant.manifest()

    assert instance_m["name"] == module_m["name"]
    assert instance_m["version"] == module_m["version"]
    assert {s["id"] for s in instance_m["skills"]} == {
        s["id"] for s in module_m["skills"]
    }


# ---------------------------------------------------------------------------
# Détection de langue
# ---------------------------------------------------------------------------


def test_detect_language_french() -> None:
    """GIVEN un message clairement français
    WHEN _detect_language est appelé
    THEN il retourne 'fr'."""
    assert dev_assistant._detect_language("Implémente un système d'auth JWT") == "fr"


def test_detect_language_english() -> None:
    """GIVEN un message clairement anglais
    WHEN _detect_language est appelé
    THEN il retourne 'en'."""
    assert dev_assistant._detect_language("Implement JWT authentication") == "en"


def test_detect_language_no_false_positive() -> None:
    """GIVEN un message anglais contenant un marqueur français en sous-chaîne
    WHEN _detect_language est appelé
    THEN il retourne 'en' (pas de faux positif par sous-chaîne)."""
    # 'feature' contient 'tu' mais ce n'est pas un token entier
    assert dev_assistant._detect_language("Add a feature for the export button") == "en"


# ---------------------------------------------------------------------------
# Détection d'intention
# ---------------------------------------------------------------------------


def test_detect_intent_implement_from_verb() -> None:
    """GIVEN un message avec un verbe d'implémentation explicite
    WHEN _detect_intent est appelé
    THEN il retourne 'implement'."""
    assert dev_assistant._detect_intent("Implémente l'auth JWT") == "implement"
    assert dev_assistant._detect_intent("implement the login feature") == "implement"


def test_detect_intent_explore_from_question() -> None:
    """GIVEN une question sur la codebase
    WHEN _detect_intent est appelé
    THEN il retourne 'explore'."""
    assert dev_assistant._detect_intent("Comment fonctionne l'auth ?") == "explore"
    assert dev_assistant._detect_intent("How does auth work?") == "explore"


def test_detect_intent_explore_is_default() -> None:
    """GIVEN un message ambigu sans marqueur d'implémentation clair
    WHEN _detect_intent est appelé
    THEN il retourne 'explore' (défaut sécurisé)."""
    assert dev_assistant._detect_intent("L'auth JWT") == "explore"


def test_detect_intent_question_mark_favours_explore() -> None:
    """GIVEN un message avec un point d'interrogation
    WHEN _detect_intent est appelé
    THEN il retourne 'explore' même si un marqueur implement est présent."""
    assert dev_assistant._detect_intent("Pourquoi ne pas créer l'auth ?") == "explore"


# ---------------------------------------------------------------------------
# Slug generation
# ---------------------------------------------------------------------------


def test_slugify_basic() -> None:
    """GIVEN un titre avec espaces et majuscules
    WHEN _slugify est appelé
    THEN le résultat est minuscule et séparé par des tirets."""
    assert dev_assistant._slugify("User Authentication") == "user-authentication"


def test_slugify_accents() -> None:
    """GIVEN un titre avec accents français
    WHEN _slugify est appelé
    THEN les accents sont supprimés et le slug est ASCII."""
    slug = dev_assistant._slugify("Implémentation d'un bouton d'export")
    assert slug.isascii()
    assert " " not in slug


def test_slugify_max_length() -> None:
    """GIVEN un titre très long
    WHEN _slugify est appelé
    THEN le résultat fait au plus 64 caractères."""
    assert len(dev_assistant._slugify("a" * 200)) <= 64


# ---------------------------------------------------------------------------
# TaskSpec — extraction des couches
# ---------------------------------------------------------------------------

_SAMPLE_TASKSPEC = """\
# TaskSpec — JWT Auth

> Généré par spec-assistant
> Statut : DRAFT

## Objectif
L'utilisateur peut s'authentifier via JWT.

## Couches concernées
- [ ] Base de données / Modèle de données
- [x] API / Services backend
- [x] Types / Interfaces / Contrats
- [ ] Frontend / UI / Composants
- [x] Tests unitaires

## Définition de "Fini"
- [ ] L'endpoint /auth/login retourne un token JWT valide
- [ ] Les tests d'intégration passent
"""

_IMPLEMENTED_TASKSPEC = """\
## Couches concernées
- [x] API / Services backend ✅
- [x] Tests unitaires
"""


def test_extract_layers_to_implement_reads_checked_layers() -> None:
    """GIVEN une TaskSpec avec des couches cochées [x] et non cochées [ ]
    WHEN extract_layers_to_implement est appelé
    THEN seules les couches [x] sans ✅ sont retournées."""
    layers = dev_assistant.extract_layers_to_implement(_SAMPLE_TASKSPEC)

    assert "API / Services backend" in layers
    assert "Types / Interfaces / Contrats" in layers
    assert "Tests unitaires" in layers
    assert "Base de données / Modèle de données" not in layers
    assert "Frontend / UI / Composants" not in layers


def test_extract_layers_excludes_already_implemented() -> None:
    """GIVEN une TaskSpec avec une couche déjà marquée ✅
    WHEN extract_layers_to_implement est appelé
    THEN la couche ✅ est exclue."""
    layers = dev_assistant.extract_layers_to_implement(_IMPLEMENTED_TASKSPEC)

    assert "API / Services backend" not in layers
    assert "Tests unitaires" in layers


def test_extract_objective_from_spec() -> None:
    """GIVEN une TaskSpec avec une section Objectif
    WHEN extract_objective est appelé
    THEN l'objectif est retourné."""
    obj = dev_assistant.extract_objective(_SAMPLE_TASKSPEC)
    assert "JWT" in obj


def test_extract_objective_fallback() -> None:
    """GIVEN du texte sans section Objectif
    WHEN extract_objective est appelé
    THEN un fallback est retourné."""
    obj = dev_assistant.extract_objective("## Couches\n- [x] API")
    assert obj != ""


def test_mark_layer_implemented_adds_checkmark() -> None:
    """GIVEN une TaskSpec avec une couche [x]
    WHEN mark_layer_implemented est appelé pour cette couche
    THEN la couche est marquée ✅."""
    content = "- [x] API / Services backend\n- [x] Tests unitaires"
    updated = dev_assistant.mark_layer_implemented(content, "API / Services backend")

    assert "- [x] API / Services backend ✅" in updated
    assert "- [x] Tests unitaires" in updated  # inchangée


def test_mark_layer_implemented_idempotent() -> None:
    """GIVEN une couche déjà marquée ✅
    WHEN mark_layer_implemented est appelé à nouveau
    THEN le contenu n'est pas dupliqué."""
    content = "- [x] API / Services backend ✅"
    updated = dev_assistant.mark_layer_implemented(content, "API / Services backend")

    assert updated.count("✅") == 1


def test_mark_layer_implemented_missing_layer() -> None:
    """GIVEN du contenu sans la couche cible
    WHEN mark_layer_implemented est appelé
    THEN le contenu est retourné inchangé."""
    content = "- [x] Tests unitaires"
    updated = dev_assistant.mark_layer_implemented(content, "API non existante")

    assert updated == content


# ---------------------------------------------------------------------------
# create_minimal_task_spec
# ---------------------------------------------------------------------------


def test_create_minimal_task_spec_contains_objective() -> None:
    """GIVEN un objectif et des règles
    WHEN create_minimal_task_spec est appelé
    THEN la spec contient l'objectif et les dépendances interdites."""
    rules = {
        "raw": "anyhow INTERDIT",
        "forbidden_deps": json.dumps(["anyhow"]),
        "patterns": "",
        "comment_convention": "",
    }
    spec = dev_assistant.create_minimal_task_spec("jwt-auth", "Auth JWT", rules)

    assert "Auth JWT" in spec
    assert "anyhow" in spec
    assert "DRAFT" in spec
    assert ".apollia/tasks" not in spec  # pas de référence auto


def test_create_minimal_task_spec_no_checked_layers() -> None:
    """GIVEN une spec minimale fraîchement créée
    WHEN les couches sont inspectées
    THEN aucune n'est cochée [x] — l'utilisateur doit valider."""
    rules = {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
    spec = dev_assistant.create_minimal_task_spec("new-task", "Objectif", rules)

    layers = dev_assistant.extract_layers_to_implement(spec)
    assert layers == []


# ---------------------------------------------------------------------------
# _extract_spec_slug
# ---------------------------------------------------------------------------


def test_extract_spec_slug_from_path() -> None:
    """GIVEN un message contenant un chemin .apollia/tasks/
    WHEN _extract_spec_slug est appelé
    THEN le slug est extrait."""
    slug = dev_assistant._extract_spec_slug(
        "Commence l'implémentation de .apollia/tasks/jwt-auth.md"
    )
    assert slug == "jwt-auth"


def test_extract_spec_slug_from_keyword() -> None:
    """GIVEN un message avec 'spec jwt-auth'
    WHEN _extract_spec_slug est appelé
    THEN le slug est extrait."""
    slug = dev_assistant._extract_spec_slug("implémente la spec jwt-auth s'il te plaît")
    assert slug == "jwt-auth"


def test_extract_spec_slug_none_when_absent() -> None:
    """GIVEN un message sans référence de spec
    WHEN _extract_spec_slug est appelé
    THEN None est retourné."""
    slug = dev_assistant._extract_spec_slug("Implémente l'auth JWT")
    assert slug is None


# ---------------------------------------------------------------------------
# load_project_rules — cache mémoire
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_load_project_rules_from_memory() -> None:
    """GIVEN de la mémoire contenant déjà project_rules
    WHEN load_project_rules est appelé
    THEN file_read n'est PAS appelé (cache mémoire utilisé)."""
    ctx = MockContext.create(
        tools={"file_read": {"content": "ne doit pas être lu"}},
        memory=True,
    )
    assert ctx.memory is not None
    await ctx.memory.remember(
        key=dev_assistant.MEMORY_KEY_PROJECT_RULES,
        value="anyhow INTERDIT dans ce projet",
        source="test",
        confidence=0.9,
    )

    rules = await dev_assistant.load_project_rules(ctx)

    assert "anyhow INTERDIT" in rules["raw"]
    assert ctx.tools is not None
    assert len([n for n, _ in ctx.tools.calls if n == "file_read"]) == 0


@pytest.mark.asyncio
async def test_load_project_rules_from_files_when_no_memory() -> None:
    """GIVEN aucune règle en mémoire mais des fichiers workspace présents
    WHEN load_project_rules est appelé
    THEN file_read est appelé et les règles sont retournées."""
    claude_md = "# Règles\nanyhow INTERDIT\nzero unwrap()"
    ctx = MockContext.create(
        tools={"file_read": {"content": claude_md}},
        memory=True,
    )

    rules = await dev_assistant.load_project_rules(ctx)

    assert rules["raw"] != ""
    assert ctx.tools is not None
    assert ctx.tools.tool_call_count() > 0


@pytest.mark.asyncio
async def test_load_project_rules_empty_when_no_sources() -> None:
    """GIVEN aucune mémoire et aucun fichier workspace (tous vides)
    WHEN load_project_rules est appelé
    THEN le dict retourné a 'raw' vide et '[]' pour forbidden_deps."""
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}},
        memory=True,
    )

    rules = await dev_assistant.load_project_rules(ctx)

    assert rules["raw"] == ""
    assert rules["forbidden_deps"] == "[]"


# ---------------------------------------------------------------------------
# build_explore_system_prompt
# ---------------------------------------------------------------------------


def test_build_explore_system_prompt_fr_contains_rules() -> None:
    """GIVEN des règles avec une dépendance interdite
    WHEN build_explore_system_prompt est appelé pour le français
    THEN le prompt contient la dépendance interdite."""
    rules = {
        "raw": "anyhow INTERDIT dans le workspace",
        "forbidden_deps": json.dumps(["anyhow"]),
        "patterns": "",
        "comment_convention": "",
    }
    prompt = dev_assistant.build_explore_system_prompt("fr", rules)

    assert "anyhow" in prompt
    assert "JAMAIS" in prompt


def test_build_explore_system_prompt_en_no_impl() -> None:
    """GIVEN des règles vides
    WHEN build_explore_system_prompt est appelé pour l'anglais
    THEN le prompt interdit l'implémentation spontanée."""
    rules = {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
    prompt = dev_assistant.build_explore_system_prompt("en", rules)

    assert "NEVER" in prompt


# ---------------------------------------------------------------------------
# Agent — run() : contrat pré-tâche (AC-1, AC-2)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_refuses_to_implement_without_taskspec() -> None:
    """GIVEN un workspace sans TaskSpec ni référence de spec dans le message
    WHEN l'utilisateur demande 'implémente un système d'auth JWT'
    THEN l'agent crée une TaskSpec minimale et retourne input_required."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
        },
        memory=True,
    )

    agent = _agent()
    result = await agent.run(
        _make_task("Implémente un système d'auth JWT"),
        ctx,
    )

    assert result["status"] == "input_required"
    prompt = result["input_required_data"]["prompt"]
    assert ".apollia/tasks/" in prompt
    assert ctx.tools is not None
    assert "file_write" in [n for n, _ in ctx.tools.calls]


@pytest.mark.asyncio
async def test_reads_project_rules_before_coding() -> None:
    """GIVEN un workspace avec CLAUDE.md contenant des contraintes
    WHEN l'agent démarre une session
    THEN load_project_rules est appelé et les règles sont chargées avant toute action."""
    claude_md = "# Règles\ndate-fns INTERDIT dans ce projet"
    ctx = MockContext.create(
        tools={
            "file_read": {"content": claude_md},
            "file_write": {"success": True},
        },
        memory=True,
    )

    agent = _agent()
    # "Implémente" → implement intent, "la" → French marker
    await agent.run(
        _make_task("Implémente la gestion des dates"),
        ctx,
    )

    assert ctx.memory is not None
    cached_rules = await ctx.memory.recall(dev_assistant.MEMORY_KEY_PROJECT_RULES)
    assert cached_rules is not None
    assert "date-fns INTERDIT" in cached_rules


# ---------------------------------------------------------------------------
# Agent — run() : délégation A2A (AC-4)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_delegates_to_code_worker_via_a2a() -> None:
    """GIVEN une TaskSpec valide avec des couches [x] cochées
    WHEN l'agent implémente via la spec
    THEN ctx.a2a.call(agent='code-worker') est appelé au moins une fois."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _SAMPLE_TASKSPEC},
            "file_write": {"success": True},
        },
        memory=True,
    )
    mock_a2a = MockA2A(
        responses={"code-worker": {"output": "Layer implemented successfully."}}
    )
    _inject_a2a(ctx, mock_a2a)

    agent = _agent()
    result = await agent.run(
        _make_task("Commence l'implémentation de .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    assert result["status"] == "completed"
    assert "code-worker" in mock_a2a.called_agents()


@pytest.mark.asyncio
async def test_a2a_called_once_per_layer() -> None:
    """GIVEN une TaskSpec avec trois couches [x]
    WHEN l'agent implémente
    THEN code-worker est appelé exactement trois fois (une par couche)."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _SAMPLE_TASKSPEC},
            "file_write": {"success": True},
        },
        memory=True,
    )
    mock_a2a = MockA2A(
        responses={"code-worker": {"output": "done"}}
    )
    _inject_a2a(ctx, mock_a2a)

    agent = _agent()
    await agent.run(
        _make_task("Commence .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    # _SAMPLE_TASKSPEC a 3 couches [x] : API, Types, Tests
    code_worker_calls = [c for c in mock_a2a.calls if c["agent"] == "code-worker"]
    assert len(code_worker_calls) == 3


@pytest.mark.asyncio
async def test_a2a_input_contains_spec_and_rules() -> None:
    """GIVEN une TaskSpec et des règles projet
    WHEN code-worker est appelé via A2A
    THEN l'input contient la spec et les dépendances interdites."""
    claude_md = "anyhow INTERDIT dans le workspace"
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _SAMPLE_TASKSPEC},
            "file_write": {"success": True},
        },
        memory=True,
    )
    # Pré-charger les règles en mémoire pour un contrôle précis
    await ctx.memory.remember(
        key=dev_assistant.MEMORY_KEY_PROJECT_RULES,
        value=claude_md,
        source="test",
        confidence=0.9,
    )
    await ctx.memory.remember(
        key=dev_assistant.MEMORY_KEY_FORBIDDEN_DEPS,
        value='["anyhow"]',
        source="test",
        confidence=0.9,
    )

    mock_a2a = MockA2A(responses={"code-worker": {"output": "done"}})
    _inject_a2a(ctx, mock_a2a)

    agent = _agent()
    await agent.run(
        _make_task("Commence .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    assert mock_a2a.calls
    first_call = mock_a2a.calls[0]
    assert first_call["skill"] == "implement"
    assert "spec" in first_call["input"]
    assert "forbidden_deps" in first_call["input"]
    assert "anyhow" in first_call["input"]["forbidden_deps"]


@pytest.mark.asyncio
async def test_taskspec_updated_after_each_layer() -> None:
    """GIVEN une TaskSpec avec plusieurs couches
    WHEN l'agent implémente
    THEN file_write est appelé après chaque couche (suivi de progression)."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _SAMPLE_TASKSPEC},
            "file_write": {"success": True},
        },
        memory=True,
    )
    mock_a2a = MockA2A(responses={"code-worker": {"output": "done"}})
    _inject_a2a(ctx, mock_a2a)

    agent = _agent()
    await agent.run(
        _make_task("Commence .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    assert ctx.tools is not None
    write_calls = [n for n, _ in ctx.tools.calls if n == "file_write"]
    # Au moins une écriture par couche implémentée
    assert len(write_calls) >= 3


# ---------------------------------------------------------------------------
# Agent — run() : mode exploration (AC-6)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_exploration_mode_no_spontaneous_implementation() -> None:
    """GIVEN une question sans demande d'implémentation
    WHEN l'agent répond
    THEN la réponse ne contient pas de proposition d'implémentation."""
    llm_response = (
        "L'authentification dans ce projet utilise JWT. "
        "Les tokens sont générés dans le module auth/tokens.rs "
        "avec une durée de vie de 24h."
    )
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}},
        llm_responses=[{"content": llm_response}],
        memory=True,
    )

    agent = _agent()
    result = await agent.run(
        _make_task("Comment fonctionne l'authentification dans ce projet ?"),
        ctx,
    )

    assert result["status"] == "completed"
    text = result["output"][0]["text"]
    assert "implémente" not in text.lower()
    assert "implement" not in text.lower()


@pytest.mark.asyncio
async def test_exploration_mode_stores_finding_in_memory() -> None:
    """GIVEN une réponse d'exploration
    WHEN l'agent répond
    THEN la réponse est stockée en mémoire sémantique avec le préfixe 'codebase:'."""
    llm_response = "L'auth utilise JWT avec un secret stocké en variable d'env."
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}},
        llm_responses=[{"content": llm_response}],
        memory=True,
    )

    agent = _agent()
    await agent.run(
        _make_task("Comment fonctionne l'auth ?"),
        ctx,
    )

    assert ctx.memory is not None
    codebase_ops = [
        op for op in ctx.memory.operations
        if op["op"] == "remember"
        and str(op.get("key", "")).startswith(dev_assistant.MEMORY_KEY_CODEBASE_PREFIX)
    ]
    assert len(codebase_ops) > 0


# ---------------------------------------------------------------------------
# Agent — run() : mémoire persistante (AC-7)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_memory_loaded_on_second_session() -> None:
    """GIVEN une session où les règles ont été persistées en mémoire
    WHEN load_project_rules est appelé avec ces règles pré-chargées
    THEN file_read n'est PAS appelé (cache mémoire fait autorité)."""
    ctx = MockContext.create(
        tools={"file_read": {"content": "fallback — ne doit pas être lu"}},
        memory=True,
    )
    assert ctx.memory is not None
    await ctx.memory.remember(
        key=dev_assistant.MEMORY_KEY_PROJECT_RULES,
        value="no-lodash INTERDIT dans ce projet",
        source="dev-assistant",
        confidence=0.9,
    )
    await ctx.memory.remember(
        key=dev_assistant.MEMORY_KEY_FORBIDDEN_DEPS,
        value='["no-lodash"]',
        source="dev-assistant",
        confidence=0.9,
    )

    rules = await dev_assistant.load_project_rules(ctx)

    assert "no-lodash INTERDIT" in rules["raw"]
    assert ctx.tools is not None
    assert len([n for n, _ in ctx.tools.calls if n == "file_read"]) == 0


# ---------------------------------------------------------------------------
# Agent — run() : tous les couches déjà implémentées
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_all_layers_already_implemented() -> None:
    """GIVEN une TaskSpec dont toutes les couches [x] sont déjà marquées ✅
    WHEN l'agent tente d'implémenter
    THEN il retourne un message 'tout est déjà fait' sans appel A2A."""
    all_done = """\
## Couches concernées
- [x] API / Services backend ✅
- [x] Tests unitaires ✅
"""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": all_done},
            "file_write": {"success": True},
        },
        memory=True,
    )
    mock_a2a = MockA2A()
    _inject_a2a(ctx, mock_a2a)

    agent = _agent()
    result = await agent.run(
        _make_task("Commence .apollia/tasks/done-spec.md"),
        ctx,
    )

    assert result["status"] == "completed"
    assert not mock_a2a.calls


# ---------------------------------------------------------------------------
# Agent — A2A indisponible (dégradation gracieuse)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_graceful_degradation_without_a2a() -> None:
    """GIVEN une TaskSpec valide mais aucun A2A injecté
    WHEN l'agent implémente
    THEN il retourne 'completed' avec un message de repli (pas d'exception)."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _SAMPLE_TASKSPEC},
            "file_write": {"success": True},
        },
        memory=True,
    )

    agent = _agent()
    result = await agent.run(
        _make_task("Commence .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    assert result["status"] == "completed"
    text = result["output"][0]["text"]
    assert "A2A" in text or "manuellement" in text.lower()


# ---------------------------------------------------------------------------
# Agent — cas d'erreur : aucun input
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_fails_without_input() -> None:
    """GIVEN une tâche sans input
    WHEN run() est appelé
    THEN il retourne status='failed'."""
    ctx = MockContext.create(memory=True)

    agent = _agent()
    result = await agent.run({"input": {"text": ""}}, ctx)

    assert result["status"] == "failed"
    assert "code" in result["error"]


# ---------------------------------------------------------------------------
# Instance module-level
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    """GIVEN le module après exécution
    WHEN l'attribut agent est accédé
    THEN c'est un DevAssistant avec un manifest valide."""
    assert isinstance(dev_assistant.agent, dev_assistant.DevAssistant)
    assert dev_assistant.agent.manifest()["name"] == "dev-assistant"


def test_module_exports_all_public_symbols() -> None:
    """GIVEN le module chargé
    WHEN les symboles publics sont accédés
    THEN tous sont présents et ont le bon type."""
    assert callable(dev_assistant.manifest)
    assert callable(dev_assistant.load_project_rules)
    assert callable(dev_assistant.persist_rules)
    assert callable(dev_assistant.load_task_spec)
    assert callable(dev_assistant.write_task_spec)
    assert callable(dev_assistant.list_task_specs)
    assert callable(dev_assistant.create_minimal_task_spec)
    assert callable(dev_assistant.extract_layers_to_implement)
    assert callable(dev_assistant.extract_objective)
    assert callable(dev_assistant.mark_layer_implemented)
    assert callable(dev_assistant.build_explore_system_prompt)
    assert callable(dev_assistant.delegate_to_code_worker)
    assert callable(dev_assistant.delegate_to_git_worker)
    assert isinstance(dev_assistant.MEMORY_KEY_PROJECT_RULES, str)
    assert isinstance(dev_assistant.MEMORY_KEY_CODEBASE_PREFIX, str)
