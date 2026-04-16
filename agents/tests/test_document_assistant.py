"""Tests de document-assistant."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.testing import MockContext  # noqa: E402

_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "document-assistant.py"
_module_spec = importlib.util.spec_from_file_location("document_assistant", _AGENT_PATH)
assert _module_spec is not None and _module_spec.loader is not None
document_assistant = importlib.util.module_from_spec(_module_spec)
_module_spec.loader.exec_module(document_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------


class MockA2A:
    """Minimal A2A mock for document-assistant unit tests.

    Responses are keyed by agent name.  When a response is an ``Exception``
    instance, ``call()`` raises it instead of returning it.
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
        response = self.responses.get(agent, {"output": f"[mock] {agent}/{skill} done"})
        if isinstance(response, Exception):
            raise response
        return response

    def called_agents(self) -> list[str]:
        """Return the ordered list of agent names that were called."""
        return [c["agent"] for c in self.calls]


def _inject_a2a(ctx: Any, a2a: MockA2A) -> Any:
    """Attach *a2a* to *ctx* and return *ctx*."""
    ctx.a2a = a2a
    return ctx


def _make_task(text: str, history: list[dict[str, str]] | None = None) -> dict[str, Any]:
    """Build a task dict in the Apollia runtime format."""
    t: dict[str, Any] = {"input": {"text": text}}
    if history:
        t["history"] = history
    return t


def _agent() -> Any:
    """Return a fresh DocumentAssistant instance for each test."""
    return document_assistant.DocumentAssistant()


def _output_text(result: dict[str, Any]) -> str:
    """Extract the plain-text output from a completed AIPResult dict."""
    return result["output"][0]["text"]


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    """GIVEN document-assistant
    WHEN manifest() est appelé
    THEN champs requis présents, tag généraliste, supports_a2a True."""
    m = document_assistant.manifest()

    assert m["name"] == "document-assistant"
    assert m["version"] == "1.0.0"
    assert m["memory_namespace"] == "document-assistant"
    assert "généraliste" in m["tags"]
    assert m["supports_a2a"] is True
    assert m["supports_streaming"] is True
    assert m["execution_mode"] == "auto"
    assert m["agent_type"] == "assistant"
    assert "file_read" in m["tools_required"]


def test_manifest_has_analyze_skill() -> None:
    """GIVEN le manifest
    WHEN les skills sont inspectés
    THEN le skill 'analyze' est déclaré avec les champs requis."""
    m = document_assistant.manifest()
    skills = m.get("skills", [])

    assert any(s["id"] == "analyze" for s in skills)
    for skill in skills:
        assert "name" in skill
        assert "description" in skill
        assert "input_modes" in skill
        assert "output_modes" in skill


def test_manifest_instance_matches_module_function() -> None:
    """GIVEN l'instance module-level et la fonction manifest()
    WHEN les deux sont appelées
    THEN elles retournent le même nom et la même version."""
    inst = document_assistant.agent.manifest()
    mod = document_assistant.manifest()

    assert inst["name"] == mod["name"]
    assert inst["version"] == mod["version"]



# ---------------------------------------------------------------------------
# _extract_file_paths
# ---------------------------------------------------------------------------


def test_extract_file_paths_xlsx() -> None:
    """GIVEN un message contenant un fichier .xlsx
    WHEN _extract_file_paths est appelé
    THEN le fichier est extrait."""
    paths = document_assistant._extract_file_paths(
        "Quel est le total de la colonne Ventes dans ventes-2026.xlsx ?"
    )

    assert "ventes-2026.xlsx" in paths


def test_extract_file_paths_pdf() -> None:
    """GIVEN un message contenant un fichier .pdf
    WHEN _extract_file_paths est appelé
    THEN le fichier est extrait."""
    paths = document_assistant._extract_file_paths("Résume ce rapport de 40 pages : rapport.pdf")

    assert "rapport.pdf" in paths


def test_extract_file_paths_multiple_types() -> None:
    """GIVEN un message mentionnant un .xlsx et un .pdf
    WHEN _extract_file_paths est appelé
    THEN les deux fichiers sont extraits."""
    paths = document_assistant._extract_file_paths(
        "Compare les données de ventes.xlsx avec le rapport.pdf du trimestre"
    )

    assert "ventes.xlsx" in paths
    assert "rapport.pdf" in paths


def test_extract_file_paths_dedup() -> None:
    """GIVEN le même fichier mentionné deux fois
    WHEN _extract_file_paths est appelé
    ALORS il n'est retourné qu'une seule fois."""
    paths = document_assistant._extract_file_paths(
        "analyse data.csv puis montre les colonnes de data.csv"
    )

    assert paths.count("data.csv") == 1


def test_extract_file_paths_empty_when_no_document() -> None:
    """GIVEN un message sans fichier document
    WHEN _extract_file_paths est appelé
    ALORS la liste est vide."""
    paths = document_assistant._extract_file_paths("analyse mes données")

    assert paths == []


# ---------------------------------------------------------------------------
# _route_by_extension
# ---------------------------------------------------------------------------


def test_route_xlsx_to_excel_worker() -> None:
    assert document_assistant._route_by_extension("ventes.xlsx") == "excel-worker"


def test_route_xlsm_to_excel_worker() -> None:
    assert document_assistant._route_by_extension("macro.xlsm") == "excel-worker"


def test_route_xls_to_excel_worker() -> None:
    assert document_assistant._route_by_extension("old.xls") == "excel-worker"


def test_route_csv_to_csv_worker() -> None:
    assert document_assistant._route_by_extension("data.csv") == "csv-data-worker"


def test_route_tsv_to_csv_worker() -> None:
    assert document_assistant._route_by_extension("data.tsv") == "csv-data-worker"


def test_route_pdf_to_pdf_worker() -> None:
    assert document_assistant._route_by_extension("rapport.pdf") == "pdf-worker"


def test_route_sqlite_to_sql_worker() -> None:
    assert document_assistant._route_by_extension("mydb.sqlite") == "sql-worker"


def test_route_sqlite3_to_sql_worker() -> None:
    assert document_assistant._route_by_extension("mydb.sqlite3") == "sql-worker"


def test_route_db_to_sql_worker() -> None:
    assert document_assistant._route_by_extension("app.db") == "sql-worker"


def test_route_unknown_extension_returns_none() -> None:
    assert document_assistant._route_by_extension("document.docx") is None


def test_route_extension_case_insensitive() -> None:
    assert document_assistant._route_by_extension("REPORT.PDF") == "pdf-worker"


# ---------------------------------------------------------------------------
# _humanize_error
# ---------------------------------------------------------------------------


def test_humanize_error_file_not_found_fr() -> None:
    """GIVEN une erreur 'no such file'
    WHEN _humanize_error est appelé en français
    ALORS le message est lisible et ne contient pas de jargon technique."""
    err = FileNotFoundError("no such file or directory: ventes.xlsx")
    msg = document_assistant._humanize_error(err, "ventes.xlsx", "fr")

    assert "ventes.xlsx" in msg
    assert "Traceback" not in msg
    assert "FileNotFoundError" not in msg
    assert "trouvé" in msg.lower() or "introuvable" in msg.lower()


def test_humanize_error_file_not_found_en() -> None:
    """GIVEN une erreur 'not found'
    WHEN _humanize_error est appelé en anglais
    ALORS le message est en anglais sans jargon."""
    err = FileNotFoundError("file not found: data.csv")
    msg = document_assistant._humanize_error(err, "data.csv", "en")

    assert "data.csv" in msg
    assert "FileNotFoundError" not in msg
    assert "found" in msg.lower()


def test_humanize_error_corrupt_file() -> None:
    """GIVEN une erreur 'corrupt or invalid'
    WHEN _humanize_error est appelé
    ALORS il mentionne l'intégrité ou le format du fichier."""
    err = ValueError("corrupt or invalid file format")
    msg = document_assistant._humanize_error(err, "report.xlsx", "fr")

    assert "report.xlsx" in msg
    assert "corrupt" not in msg  # Jargon anglais absent
    assert len(msg) < 400


def test_humanize_error_column_not_found() -> None:
    """GIVEN une erreur 'column not found'
    WHEN _humanize_error est appelé
    ALORS il mentionne la colonne."""
    err = KeyError("column 'Ventes' not found")
    msg = document_assistant._humanize_error(err, "data.xlsx", "fr")

    assert "data.xlsx" in msg
    assert "colonne" in msg.lower()


def test_humanize_error_generic_fallback_is_short() -> None:
    """GIVEN une erreur générique imprévue
    WHEN _humanize_error est appelé
    ALORS un message court et lisible est retourné."""
    err = RuntimeError("unexpected internal worker failure")
    msg = document_assistant._humanize_error(err, "data.db", "fr")

    assert "data.db" in msg
    assert len(msg) < 400


# ---------------------------------------------------------------------------
# run() — AC-1 routing .xlsx → excel-worker
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_routes_xlsx_to_excel_worker() -> None:
    """GIVEN une demande sur un fichier .xlsx
    WHEN run() est appelé
    ALORS ctx.a2a.call est invoqué avec agent='excel-worker'
    ET la réponse est completed."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"excel-worker": {"output": "Le total des ventes est 142 580 €."}})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(
        _make_task("Quel est le total de la colonne Ventes dans ventes-2026.xlsx ?"),
        ctx,
    )

    assert result["status"] == "completed"
    assert "excel-worker" in a2a.called_agents()


@pytest.mark.asyncio
async def test_routes_pdf_to_pdf_worker() -> None:
    """GIVEN une demande sur un fichier .pdf
    WHEN run() est appelé
    ALORS ctx.a2a.call est invoqué avec agent='pdf-worker'."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"pdf-worker": {"output": "Points clés : croissance +12 %."}})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(
        _make_task("Résume ce rapport de 40 pages : rapport.pdf"),
        ctx,
    )

    assert result["status"] == "completed"
    assert "pdf-worker" in a2a.called_agents()


@pytest.mark.asyncio
async def test_routes_csv_to_csv_worker() -> None:
    """GIVEN une demande sur un fichier .csv
    WHEN run() est appelé
    ALORS ctx.a2a.call est invoqué avec agent='csv-data-worker'."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"csv-data-worker": {"output": "Nombre de lignes : 1 234"}})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(_make_task("Combien de lignes dans data.csv ?"), ctx)

    assert result["status"] == "completed"
    assert "csv-data-worker" in a2a.called_agents()


@pytest.mark.asyncio
async def test_routes_sqlite_to_sql_worker() -> None:
    """GIVEN une demande sur une base .sqlite
    WHEN run() est appelé
    ALORS ctx.a2a.call est invoqué avec agent='sql-worker'."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"sql-worker": {"output": "15 tables trouvées."}})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(_make_task("Liste les tables de ma base app.sqlite"), ctx)

    assert result["status"] == "completed"
    assert "sql-worker" in a2a.called_agents()


@pytest.mark.asyncio
async def test_a2a_skill_is_analyze() -> None:
    """GIVEN une demande valide
    WHEN run() appelle A2A
    ALORS le skill utilisé est 'analyze'."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"excel-worker": {"output": "Résultat."}})
    _inject_a2a(ctx, a2a)

    await _agent().run(_make_task("Analyse ventes.xlsx"), ctx)

    assert len(a2a.calls) == 1
    assert a2a.calls[0]["skill"] == "analyze"


@pytest.mark.asyncio
async def test_a2a_input_contains_file_and_task() -> None:
    """GIVEN une demande valide
    WHEN run() appelle A2A
    ALORS l'input contient 'file' et 'task'."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"pdf-worker": {"output": "ok"}})
    _inject_a2a(ctx, a2a)

    await _agent().run(_make_task("Résume rapport.pdf"), ctx)

    call_input = a2a.calls[0]["input"]
    assert "file" in call_input
    assert "task" in call_input
    assert "rapport.pdf" in call_input["file"]


# ---------------------------------------------------------------------------
# run() — AC-3 humanisation des erreurs worker
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_humanizes_worker_error_no_stack_trace() -> None:
    """GIVEN un worker qui lève FileNotFoundError
    WHEN run() est appelé
    ALORS la réponse est completed en langage naturel sans stack trace."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"excel-worker": FileNotFoundError("no such file: ventes.xlsx")})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(_make_task("Analyse ventes.xlsx"), ctx)

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "Traceback" not in text
    assert "FileNotFoundError" not in text
    assert "ventes.xlsx" in text


@pytest.mark.asyncio
async def test_partial_success_when_one_worker_fails() -> None:
    """GIVEN deux fichiers, dont un dont le worker échoue
    WHEN run() est appelé
    ALORS le résultat du fichier valide est présent ET l'erreur est humanisée."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({
        "excel-worker": {"output": "Total : 42 €"},
        "pdf-worker": FileNotFoundError("no such file: rapport.pdf"),
    })
    _inject_a2a(ctx, a2a)

    result = await _agent().run(
        _make_task("Compare ventes.xlsx avec rapport.pdf"),
        ctx,
    )

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "42" in text
    assert "rapport.pdf" in text
    assert "Traceback" not in text


# ---------------------------------------------------------------------------
# run() — AC-4 demande ambiguë → une seule question
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_asks_clarification_on_ambiguous_request() -> None:
    """GIVEN une demande sans fichier identifiable
    WHEN run() est appelé
    ALORS status='input_required' et aucun worker n'est appelé."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A()
    _inject_a2a(ctx, a2a)

    result = await _agent().run(_make_task("analyse mes données"), ctx)

    assert result["status"] == "input_required"
    assert len(a2a.calls) == 0


@pytest.mark.asyncio
async def test_clarification_contains_single_question() -> None:
    """GIVEN une demande ambiguë
    WHEN run() retourne input_required
    ALORS le prompt ne contient qu'un seul point d'interrogation (une question)."""
    ctx = MockContext.create(memory=True)
    _inject_a2a(ctx, MockA2A())

    result = await _agent().run(_make_task("analyse mes données"), ctx)

    assert result["status"] == "input_required"
    prompt = result["input_required_data"]["prompt"]
    assert prompt.count("?") <= 1


@pytest.mark.asyncio
async def test_no_direct_file_read_on_ambiguous_request() -> None:
    """GIVEN une demande ambiguë sans fichier
    WHEN run() retourne input_required
    ALORS file_read n'est pas appelé."""
    ctx = MockContext.create(tools={"file_read": {"content": ""}}, memory=True)
    _inject_a2a(ctx, MockA2A())

    await _agent().run(_make_task("analyse mes données"), ctx)

    assert ctx.tools is not None
    file_read_calls = [n for n, _ in ctx.tools.calls if n == "file_read"]
    assert len(file_read_calls) == 0


# ---------------------------------------------------------------------------
# run() — AC-5 multi-workers
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_calls_multiple_workers_for_multi_file_request() -> None:
    """GIVEN une demande mentionnant un .xlsx et un .pdf
    WHEN run() est appelé
    ALORS excel-worker ET pdf-worker sont tous les deux appelés."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({
        "excel-worker": {"output": "Total ventes : 142 580 €"},
        "pdf-worker": {"output": "Résumé trimestre : croissance +12 %."},
    })
    _inject_a2a(ctx, a2a)

    result = await _agent().run(
        _make_task("Compare les données de ventes.xlsx avec le rapport.pdf du trimestre"),
        ctx,
    )

    assert result["status"] == "completed"
    agents_called = a2a.called_agents()
    assert "excel-worker" in agents_called
    assert "pdf-worker" in agents_called


@pytest.mark.asyncio
async def test_multi_file_response_contains_both_results() -> None:
    """GIVEN deux fichiers et deux workers
    WHEN run() est appelé
    ALORS la réponse unifiée contient les données des deux fichiers."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({
        "excel-worker": {"output": "Total : 100 €"},
        "pdf-worker": {"output": "Résumé : bon trimestre."},
    })
    _inject_a2a(ctx, a2a)

    result = await _agent().run(
        _make_task("Compare ventes.xlsx avec rapport.pdf"),
        ctx,
    )

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "100" in text
    assert "trimestre" in text


# ---------------------------------------------------------------------------
# run() — AC-6 détection de langue
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_language_detection_english_request() -> None:
    """GIVEN un message en anglais avec un fichier .pdf
    WHEN run() est appelé
    ALORS pdf-worker est appelé ET la réponse a le statut completed."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"pdf-worker": {"output": "Summary: key findings."}})
    _inject_a2a(ctx, a2a)

    result = await _agent().run(_make_task("Summarize this report.pdf"), ctx)

    assert result["status"] == "completed"
    assert "pdf-worker" in a2a.called_agents()


@pytest.mark.asyncio
async def test_worker_instruction_reflects_language() -> None:
    """GIVEN un message en anglais
    WHEN run() construit l'instruction pour le worker
    ALORS l'instruction est en anglais."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"pdf-worker": {"output": "ok"}})
    _inject_a2a(ctx, a2a)

    await _agent().run(_make_task("Summarize report.pdf"), ctx)

    task_instruction: str = a2a.calls[0]["input"]["task"]
    assert "Reply in English" in task_instruction or "File:" in task_instruction


# ---------------------------------------------------------------------------
# run() — sans A2A disponible
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_no_a2a_returns_completed_with_fallback() -> None:
    """GIVEN une demande avec fichier identifiable mais sans A2A attaché
    WHEN run() est appelé
    ALORS status='completed' avec un message de repli (pas d'exception)."""
    ctx = MockContext.create(memory=True)

    result = await _agent().run(_make_task("Analyse ventes.xlsx"), ctx)

    assert result["status"] == "completed"
    text = _output_text(result)
    assert len(text) > 0


# ---------------------------------------------------------------------------
# run() — entrée vide
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_fails_without_input() -> None:
    """GIVEN une tâche sans texte d'entrée
    WHEN run() est appelé
    ALORS status='failed' avec un code d'erreur structuré."""
    ctx = MockContext.create(memory=True)

    result = await _agent().run({"input": {"text": ""}}, ctx)

    assert result["status"] == "failed"
    assert "code" in result["error"]


# ---------------------------------------------------------------------------
# Mémoire — recent_files
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_persists_recent_files_after_successful_analysis() -> None:
    """GIVEN une analyse réussie de ventes.xlsx
    WHEN run() se termine
    ALORS ventes.xlsx est dans la mémoire recent_files."""
    ctx = MockContext.create(memory=True)
    a2a = MockA2A({"excel-worker": {"output": "Total : 100 €"}})
    _inject_a2a(ctx, a2a)

    await _agent().run(_make_task("Analyse ventes.xlsx"), ctx)

    assert ctx.memory is not None
    raw = await ctx.memory.recall(document_assistant.MEMORY_KEY_RECENT_FILES)
    assert raw is not None
    files = json.loads(raw)
    assert "ventes.xlsx" in files


@pytest.mark.asyncio
async def test_recent_files_capped_at_max() -> None:
    """GIVEN plusieurs analyses successives
    WHEN la liste de fichiers récents dépasse _MAX_RECENT_FILES
    ALORS la taille reste bornée."""
    ctx = MockContext.create(memory=True)

    for i in range(document_assistant._MAX_RECENT_FILES + 3):
        a2a = MockA2A({"csv-data-worker": {"output": "ok"}})
        _inject_a2a(ctx, a2a)
        await document_assistant._save_recent_files(ctx, [f"file{i}.csv"])

    raw = await ctx.memory.recall(document_assistant.MEMORY_KEY_RECENT_FILES)
    assert raw is not None
    files = json.loads(raw)
    assert len(files) <= document_assistant._MAX_RECENT_FILES


# ---------------------------------------------------------------------------
# Instance module-level
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    """GIVEN le module après chargement
    WHEN l'attribut agent est accédé
    ALORS c'est un DocumentAssistant avec un manifest valide."""
    assert isinstance(document_assistant.agent, document_assistant.DocumentAssistant)
    assert document_assistant.agent.manifest()["name"] == "document-assistant"


def test_module_exports_public_symbols() -> None:
    """GIVEN le module chargé
    WHEN les symboles publics sont accédés
    ALORS tous sont présents et du bon type."""
    assert callable(document_assistant.manifest)
    assert isinstance(document_assistant.ROUTING_TABLE, dict)
    assert isinstance(document_assistant.MEMORY_KEY_FORMAT_PREF, str)
    assert isinstance(document_assistant.MEMORY_KEY_RECENT_FILES, str)
    assert callable(document_assistant._extract_file_paths)
    assert callable(document_assistant._route_by_extension)
    assert callable(document_assistant._route_by_keywords)
    assert callable(document_assistant._humanize_error)
    assert callable(document_assistant._build_worker_instruction)
    assert callable(document_assistant._format_worker_result)
    assert callable(document_assistant._format_combined_response)
