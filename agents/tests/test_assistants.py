"""Smoke tests transversaux pour les 4 assistants du sprint 39.

Trois niveaux de garantie croissante :

1. **Manifest** — champs requis présents et cohérents.
2. **Comportement de base** — l'assistant effectue l'action attendue sur un
   cas représentatif (création de TaskSpec, demande de contrat, routage A2A).
3. **Résistance au crash** — aucun assistant ne lève d'exception non gérée
   sur un message simple.

Ces tests ne mesurent pas la qualité des réponses LLM.
"""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup — must precede any apollia import
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for _extra in (str(_REPO_ROOT / "sdk"), str(_REPO_ROOT / "agents")):
    if _extra not in sys.path:
        sys.path.insert(0, _extra)

from apollia.testing import MockContext  # noqa: E402


# ---------------------------------------------------------------------------
# Module loading (hyphens in filenames prohibit direct import)
# ---------------------------------------------------------------------------


def _load_agent_module(name: str) -> Any:
    """Load an assistant module by its hyphenated filename.

    Uses ``importlib.util.spec_from_file_location`` so that filenames
    containing hyphens (e.g. ``spec-assistant.py``) can be imported under
    a valid Python identifier.
    """
    path = _REPO_ROOT / "agents" / "assistants" / f"{name}.py"
    module_name = name.replace("-", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None and spec.loader is not None, (
        f"Could not locate assistant module: {path}"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


_spec_assistant = _load_agent_module("spec-assistant")
_dev_assistant = _load_agent_module("dev-assistant")
_review_assistant = _load_agent_module("review-assistant")
_document_assistant = _load_agent_module("document-assistant")

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

ASSISTANTS = [
    ("spec-assistant", _spec_assistant),
    ("dev-assistant", _dev_assistant),
    ("review-assistant", _review_assistant),
    ("document-assistant", _document_assistant),
]

REQUIRED_MANIFEST_FIELDS = [
    "name",
    "version",
    "description",
    "tools_required",
    "supports_a2a",
    "memory_namespace",
    "execution_mode",
    "agent_type",
]


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------


class _MockA2A:
    """Minimal A2A stub that records calls and returns pre-configured responses.

    Responses are keyed by agent name.  Any agent not explicitly configured
    returns a generic success response so that routing tests remain focused
    on *which* worker is called rather than *what* it returns.
    """

    def __init__(self, responses: dict[str, Any] | None = None) -> None:
        self.calls: list[dict[str, Any]] = []
        self.responses: dict[str, Any] = responses or {}

    async def call(
        self,
        agent: str,
        skill: str,
        input: dict[str, Any],
    ) -> dict[str, Any]:
        """Record the invocation and return the pre-configured response."""
        self.calls.append({"agent": agent, "skill": skill, "input": input})
        return self.responses.get(agent, {"output": f"[mock] {agent}/{skill} ok"})

    def called_agents(self) -> list[str]:
        """Return the ordered list of agent names called so far."""
        return [c["agent"] for c in self.calls]


def _make_task(text: str) -> dict[str, Any]:
    """Build a minimal task dict in the Apollia runtime format."""
    return {"input": {"text": text}}


# ---------------------------------------------------------------------------
# Niveau 1 — Manifests
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
def test_manifest_has_required_fields(agent_name: str, module: Any) -> None:
    """GIVEN an assistant
    WHEN manifest() is called
    THEN all required fields are present, non-None and non-empty for strings."""
    m = module.manifest()

    for field in REQUIRED_MANIFEST_FIELDS:
        assert field in m, f"{agent_name}: champ '{field}' absent de manifest()"
        assert m[field] is not None, f"{agent_name}: champ '{field}' est None"
        if isinstance(m[field], str):
            assert len(m[field]) > 0, f"{agent_name}: champ '{field}' est une chaîne vide"

    assert m["name"] == agent_name, (
        f"{agent_name}: manifest()['name'] retourne '{m['name']}' au lieu de '{agent_name}'"
    )
    assert m["supports_a2a"] is True, f"{agent_name}: supports_a2a doit être True"


# ---------------------------------------------------------------------------
# Niveau 2 — Comportement de base : spec-assistant
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_assistant_creates_taskspec() -> None:
    """GIVEN a workspace with CLAUDE.md rules and an LLM that emits a SPEC block
    WHEN spec-assistant receives a spec creation request
    THEN file_write is invoked for .apollia/tasks/<slug>.md
    AND the confirmation path appears in the response text."""
    spec_body = (
        "# TaskSpec — Bouton export CSV\n"
        "> Statut : DRAFT\n\n"
        "## Objectif\nAjouter un bouton d'export CSV à la vue principale.\n\n"
        "## Couches concernées\n- [x] Frontend / UI / Composants\n\n"
        "## Définition de \"Fini\"\n- [ ] Le bouton est visible en production\n\n"
        "## Historique des révisions\n- DRAFT : spec créée"
    )
    llm_response = f"Voici la TaskSpec :\n\n[SPEC:export-csv]{spec_body}[/SPEC]"
    ctx = MockContext.create(
        tools={
            "file_read": {"content": "# Règles\n- anyhow INTERDIT\n"},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123"},
        },
        llm_responses=[{"content": llm_response}],
        memory=True,
    )

    result = await _spec_assistant.SpecAssistant().run(
        _make_task("crée une spec pour un bouton d'export CSV"),
        ctx,
    )

    assert result["status"] == "completed"
    assert ctx.tools is not None
    taskspec_writes = [
        args
        for name, args in ctx.tools.calls
        if name == "file_write"
        and ".apollia/tasks/" in str(args.get("path", ""))
    ]
    assert len(taskspec_writes) >= 1, "file_write vers .apollia/tasks/ non appelé"
    response_text: str = result["output"][0]["text"]
    assert "export-csv.md" in response_text, (
        "Le chemin de confirmation de la TaskSpec est absent de la réponse"
    )


# ---------------------------------------------------------------------------
# Niveau 2 — Comportement de base : dev-assistant
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_dev_assistant_requires_taskspec_before_implementing() -> None:
    """GIVEN a workspace without a TaskSpec
    WHEN dev-assistant receives an implementation request
    THEN it returns input_required
    AND no code files (.py / .ts / .rs) are written via file_write."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123"},
        },
        memory=True,
    )

    result = await _dev_assistant.DevAssistant().run(
        _make_task("implémente une feature de login"),
        ctx,
    )

    assert result["status"] == "input_required", (
        f"Statut attendu 'input_required', obtenu '{result['status']}'"
    )
    assert ctx.tools is not None
    code_writes = [
        args
        for name, args in ctx.tools.calls
        if name == "file_write"
        and any(
            str(args.get("path", "")).endswith(ext)
            for ext in (".py", ".ts", ".tsx", ".js", ".rs")
        )
    ]
    assert len(code_writes) == 0, (
        f"Des fichiers de code ont été créés sans TaskSpec validée : {code_writes}"
    )


# ---------------------------------------------------------------------------
# Niveau 2 — Comportement de base : document-assistant routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_document_assistant_routes_xlsx_to_excel_worker() -> None:
    """GIVEN a request mentioning a .xlsx file
    WHEN document-assistant processes it
    THEN excel-worker is called via A2A."""
    ctx = MockContext.create(memory=True)
    a2a = _MockA2A({"excel-worker": {"output": "Total : 42 580 €"}})
    ctx.a2a = a2a  # type: ignore[attr-defined]

    result = await _document_assistant.DocumentAssistant().run(
        _make_task("Quel est le total de la colonne A dans ventes.xlsx ?"),
        ctx,
    )

    assert result["status"] == "completed"
    assert "excel-worker" in a2a.called_agents(), (
        f"excel-worker non appelé ; agents appelés : {a2a.called_agents()}"
    )


@pytest.mark.asyncio
async def test_document_assistant_routes_pdf_to_pdf_worker() -> None:
    """GIVEN a request mentioning a .pdf file
    WHEN document-assistant processes it
    THEN pdf-worker is called via A2A."""
    ctx = MockContext.create(memory=True)
    a2a = _MockA2A({"pdf-worker": {"output": "Points clés : +12 % de croissance."}})
    ctx.a2a = a2a  # type: ignore[attr-defined]

    result = await _document_assistant.DocumentAssistant().run(
        _make_task("Résume le fichier rapport.pdf"),
        ctx,
    )

    assert result["status"] == "completed"
    assert "pdf-worker" in a2a.called_agents(), (
        f"pdf-worker non appelé ; agents appelés : {a2a.called_agents()}"
    )


# ---------------------------------------------------------------------------
# Niveau 3 — Résistance au crash
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
async def test_agent_does_not_crash_on_hello(agent_name: str, module: Any) -> None:
    """GIVEN any assistant with a minimal runtime context
    WHEN it receives 'hello'
    THEN no unhandled exception is raised
    AND the result has a recognised status."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "abc123"},
        },
        llm_responses=[{"content": "Bonjour ! Comment puis-je vous aider ?"}],
        memory=True,
    )

    try:
        result = await module.agent.run(_make_task("hello"), ctx)
    except Exception as exc:
        pytest.fail(f"{agent_name} a levé une exception sur 'hello' : {exc!r}")

    assert result is not None, f"{agent_name}: run() a retourné None"
    assert result.get("status") in (
        "completed",
        "input_required",
        "failed",
    ), f"{agent_name}: statut inattendu '{result.get('status')}'"


# ---------------------------------------------------------------------------
# Niveau 4 — Session N+1 : bootstrap réutilisé
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_assistant_uses_bootstrap_on_second_session() -> None:
    """GIVEN a first spec-assistant session completed (bootstrap present)
    WHEN a second session starts
    THEN no file_read("APOLLIA.md") in tool invocations
    (the snapshot is loaded from memory instead of re-reading project files)."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123"},
        },
        llm_responses=[
            {"content": "Voici ma réponse."},
            {"content": "Seconde session."},
        ],
        memory=True,
        workspace_rules="# Règles\n- anyhow INTERDIT\n",
    )

    agent1 = _spec_assistant.SpecAssistant()
    await agent1.run(_make_task("crée une spec pour le login"), ctx)

    assert ctx.memory is not None
    status = await ctx.memory.recall("bootstrap.status")
    assert status == "complete", "bootstrap should be complete after first session"

    # Second session: reset tool call log but keep memory
    assert ctx.tools is not None
    ctx.tools.calls.clear()

    agent2 = _spec_assistant.SpecAssistant()
    await agent2.run(_make_task("affine la spec login"), ctx)

    file_read_apollia = [
        args
        for name, args in ctx.tools.calls
        if name == "file_read"
        and "APOLLIA" in str(args.get("path", ""))
    ]
    assert len(file_read_apollia) == 0, (
        f"file_read(APOLLIA.md) called in session N+1: {file_read_apollia}"
    )


@pytest.mark.asyncio
async def test_dev_assistant_snapshot_contains_architecture() -> None:
    """GIVEN a workspace with multiple Cargo.toml markers
    WHEN dev-assistant runs
    THEN the persisted snapshot contains architecture entries."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {
                "stdout": "./crates/core/Cargo.toml\n./crates/runtime/Cargo.toml\n",
            },
        },
        memory=True,
        workspace_rules="anyhow INTERDIT",
    )

    agent = _dev_assistant.DevAssistant()
    await agent.run(_make_task("Implémente l'auth JWT"), ctx)

    assert ctx.memory is not None
    raw_snapshot = await ctx.memory.recall("bootstrap.snapshot")
    assert raw_snapshot is not None, "bootstrap snapshot should be persisted"

    snapshot = json.loads(raw_snapshot)
    assert "architecture" in snapshot, "snapshot must contain architecture"
    assert isinstance(snapshot["architecture"], list)
