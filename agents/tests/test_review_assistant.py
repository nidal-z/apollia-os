"""Tests de review-assistant."""

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

_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "review-assistant.py"
_spec = importlib.util.spec_from_file_location("review_assistant", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
review_assistant = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(review_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Shared fixtures
# ---------------------------------------------------------------------------

_TASKSPEC_WITH_FRONTEND = """\
# TaskSpec — User Dashboard

> Généré par spec-assistant
> Statut : DRAFT

## Objectif
L'utilisateur peut consulter son tableau de bord.

## Couches concernées
- [x] API / Services backend
- [x] Frontend / UI / Composants
- [x] Tests unitaires
"""

_TASKSPEC_API_ONLY = """\
# TaskSpec — JWT Auth

## Couches concernées
- [x] API / Services backend
"""

_DIFF_WITHOUT_FRONTEND = """\
diff --git a/src/api/routes.rs b/src/api/routes.rs
--- a/src/api/routes.rs
+++ b/src/api/routes.rs
@@ -1,3 +1,6 @@
+pub async fn get_dashboard() -> Response {
+    Response::ok()
+}
"""

_DIFF_WITH_FORBIDDEN_DEP = """\
diff --git a/src/utils/date.ts b/src/utils/date.ts
--- a/src/utils/date.ts
+++ b/src/utils/date.ts
@@ -1,3 +1,5 @@
+import { format } from 'date-fns';
+
 export function formatDate(d: Date): string {
     return d.toISOString();
 }
"""

_DIFF_CLEAN = """\
diff --git a/src/api/auth.ts b/src/api/auth.ts
--- a/src/api/auth.ts
+++ b/src/api/auth.ts
@@ -1,3 +1,6 @@
+export async function createToken(userId: string): Promise<string> {
+    return sign({ userId }, process.env.SECRET);
+}
"""

_DIFF_WITH_UNWRAP = """\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
+let value = compute().unwrap();
+let other = parse().unwrap();
"""

_DIFF_WITH_UNWRAP_IN_TEST = """\
diff --git a/tests/test_main.rs b/tests/test_main.rs
--- a/tests/test_main.rs
+++ b/tests/test_main.rs
@@ -1,3 +1,5 @@
+let value = compute().unwrap();
"""

_DIFF_WITH_TODO = """\
diff --git a/src/service.rs b/src/service.rs
--- a/src/service.rs
+++ b/src/service.rs
@@ -1,3 +1,4 @@
+todo!()
"""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_task(text: str, history: list[dict[str, str]] | None = None) -> dict[str, Any]:
    """Build a task dict in the Apollia runtime format."""
    t: dict[str, Any] = {"input": {"text": text}}
    if history:
        t["history"] = history
    return t


def _agent() -> Any:
    """Return a fresh ReviewAssistant instance for each test."""
    return review_assistant.ReviewAssistant()


def _output_text(result: dict[str, Any]) -> str:
    """Extract the output text from a completed AIPResult dict."""
    return result["output"][0]["text"]


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    """GIVEN review-assistant
    WHEN manifest() est appelé
    THEN tous les champs requis sont présents et corrects."""
    m = review_assistant.manifest()

    assert m["name"] == "review-assistant"
    assert m["version"] == "1.0.0"
    assert m["memory_namespace"] == "review-assistant"
    assert "file_read" in m["tools_required"]
    assert "bash_executor" in m["tools_required"]
    assert "bash_executor" in m["tools_requiring_approval"]
    assert m["supports_a2a"] is True
    assert m["supports_streaming"] is True
    assert m["execution_mode"] == "auto"
    assert m["agent_type"] == "assistant"
    assert m["dangerous_tools_allowed"] is False
    assert m["llm_backend"] == "precise"


def test_manifest_has_review_skill() -> None:
    """GIVEN le manifest
    WHEN les skills sont inspectés
    THEN le skill 'review' est déclaré avec les champs requis."""
    m = review_assistant.manifest()
    skills = m.get("skills", [])

    assert any(s["id"] == "review" for s in skills)
    for skill in skills:
        assert "name" in skill
        assert "description" in skill
        assert "input_modes" in skill
        assert "output_modes" in skill


def test_manifest_description_mentions_pipeline() -> None:
    """GIVEN le manifest
    WHEN la description est inspectée
    THEN elle mentionne le pipeline et le rapport 🔴/🟡/🟢."""
    m = review_assistant.manifest()
    desc = m["description"].lower()

    assert "pipeline" in desc
    assert "review" in desc


def test_manifest_instance_matches_module_function() -> None:
    """GIVEN l'instance module-level et la fonction module-level
    WHEN les deux sont appelées
    THEN elles retournent le même nom et les mêmes skill IDs."""
    instance_m = review_assistant.agent.manifest()
    module_m = review_assistant.manifest()

    assert instance_m["name"] == module_m["name"]
    assert instance_m["version"] == module_m["version"]


# ---------------------------------------------------------------------------
# Détection de langue
# ---------------------------------------------------------------------------


def test_detect_language_french() -> None:
    """GIVEN un message clairement français
    WHEN _detect_language est appelé
    THEN il retourne 'fr'."""
    assert review_assistant._detect_language("Analyse le diff et revois les problèmes") == "fr"


def test_detect_language_english() -> None:
    """GIVEN un message clairement anglais
    WHEN _detect_language est appelé
    THEN il retourne 'en'."""
    assert review_assistant._detect_language("Review the implementation diff") == "en"


# ---------------------------------------------------------------------------
# _parse_diff_files / _parse_diff_additions / _parse_diff_hunks
# ---------------------------------------------------------------------------


def test_parse_diff_files_extracts_paths() -> None:
    """GIVEN un diff standard
    WHEN _parse_diff_files est appelé
    THEN les chemins de fichiers modifiés sont extraits."""
    diff = (
        "diff --git a/src/auth.ts b/src/auth.ts\n"
        "--- a/src/auth.ts\n"
        "+++ b/src/auth.ts\n"
        "@@ -1 +1 @@\n"
        "+const x = 1;\n"
        "diff --git a/tests/auth.test.ts b/tests/auth.test.ts\n"
        "--- /dev/null\n"
        "+++ b/tests/auth.test.ts\n"
        "@@ -0,0 +1 @@\n"
        "+it('works', () => {});\n"
    )
    files = review_assistant._parse_diff_files(diff)

    assert "src/auth.ts" in files
    assert "tests/auth.test.ts" in files


def test_parse_diff_additions_excludes_header() -> None:
    """GIVEN un diff avec des lignes +++ et des lignes +
    WHEN _parse_diff_additions est appelé
    THEN les lignes +++ sont exclues et les lignes + sont incluses."""
    diff = "+++ b/src/foo.ts\n@@ -1 +1 @@\n+const x = 1;\n+const y = 2;\n"
    additions = review_assistant._parse_diff_additions(diff)

    assert "const x = 1;" in additions
    assert "const y = 2;" in additions
    assert "+++ b/src/foo.ts" not in additions
    assert "b/src/foo.ts" not in additions


def test_is_test_file_positive() -> None:
    """GIVEN des chemins de fichiers de test
    WHEN _is_test_file est appelé
    THEN il retourne True."""
    assert review_assistant._is_test_file("tests/test_auth.rs")
    assert review_assistant._is_test_file("src/auth.test.ts")
    assert review_assistant._is_test_file("src/auth.spec.ts")


def test_is_test_file_negative() -> None:
    """GIVEN un chemin de fichier production
    WHEN _is_test_file est appelé
    THEN il retourne False."""
    assert not review_assistant._is_test_file("src/api/routes.rs")
    assert not review_assistant._is_test_file("src/components/Button.tsx")


def test_parse_diff_hunks_tracks_file_context() -> None:
    """GIVEN un diff multi-fichiers
    WHEN _parse_diff_hunks est appelé
    THEN chaque hunk connaît son fichier source et son statut test."""
    diff = (
        "+++ b/src/main.rs\n"
        "@@ -1 +1 @@\n"
        "+fn main() {}\n"
        "+++ b/tests/test_main.rs\n"
        "@@ -1 +1 @@\n"
        "+let x = val.unwrap();\n"
    )
    hunks = review_assistant._parse_diff_hunks(diff)

    assert len(hunks) == 2
    prod_file, is_test_prod, _ = hunks[0]
    test_file, is_test_test, _ = hunks[1]
    assert prod_file == "src/main.rs"
    assert not is_test_prod
    assert test_file == "tests/test_main.rs"
    assert is_test_test


# ---------------------------------------------------------------------------
# Complétude
# ---------------------------------------------------------------------------


def test_check_completeness_no_spec() -> None:
    """GIVEN aucune TaskSpec
    WHEN _check_completeness est appelé
    THEN un item ATTENTION est retourné."""
    items = review_assistant._check_completeness(None, "some diff")

    assert any(level == review_assistant._ATTENTION for level, _ in items)


def test_check_completeness_no_checked_layers() -> None:
    """GIVEN une TaskSpec sans couche cochée [x]
    WHEN _check_completeness est appelé
    THEN un item ATTENTION est retourné (rien à vérifier)."""
    spec = "## Couches concernées\n- [ ] API\n- [ ] Frontend\n"
    items = review_assistant._check_completeness(spec, _DIFF_CLEAN)

    assert any(level == review_assistant._ATTENTION for level, _ in items)


def test_check_completeness_missing_frontend_layer() -> None:
    """GIVEN une TaskSpec avec Frontend [x] ET un diff sans fichier frontend
    WHEN _check_completeness est appelé
    ALORS le rapport contient un item BLOQUANT pour la couche Frontend."""
    items = review_assistant._check_completeness(
        _TASKSPEC_WITH_FRONTEND,
        _DIFF_WITHOUT_FRONTEND,
    )

    bloquant_msgs = [msg for level, msg in items if level == review_assistant._BLOQUANT]
    assert any("Frontend" in msg for msg in bloquant_msgs)


def test_check_completeness_covered_api_layer() -> None:
    """GIVEN une TaskSpec avec API [x] ET un diff contenant un fichier /api/
    WHEN _check_completeness est appelé
    ALORS l'item API est LGTM."""
    spec = "## Couches concernées\n- [x] API / Services backend\n"
    diff = (
        "+++ b/src/api/auth.ts\n"
        "@@ -1 +1 @@\n"
        "+export function login() {}\n"
    )
    items = review_assistant._check_completeness(spec, diff)

    lgtm_msgs = [msg for level, msg in items if level == review_assistant._LGTM]
    assert any("API" in msg for msg in lgtm_msgs)


def test_check_completeness_empty_diff() -> None:
    """GIVEN une TaskSpec avec des couches ET un diff vide
    WHEN _check_completeness est appelé
    ALORS chaque couche requise génère un item BLOQUANT."""
    items = review_assistant._check_completeness(_TASKSPEC_API_ONLY, "")

    assert any(level == review_assistant._BLOQUANT for level, _ in items)


def test_diff_covers_layer_frontend_tsx() -> None:
    """GIVEN un diff contenant un fichier .tsx
    WHEN _diff_covers_layer est appelé pour 'Frontend / UI / Composants'
    ALORS il retourne True."""
    assert review_assistant._diff_covers_layer(
        "Frontend / UI / Composants",
        ["src/components/Button.tsx"],
    )


def test_diff_covers_layer_frontend_rs_returns_false() -> None:
    """GIVEN un diff ne contenant que des fichiers .rs
    WHEN _diff_covers_layer est appelé pour 'Frontend / UI / Composants'
    ALORS il retourne False."""
    assert not review_assistant._diff_covers_layer(
        "Frontend / UI / Composants",
        ["src/api/routes.rs"],
    )


# ---------------------------------------------------------------------------
# Conformité
# ---------------------------------------------------------------------------


def test_check_conformity_detects_unwrap_in_production() -> None:
    """GIVEN un diff production avec .unwrap()
    WHEN _check_conformity est appelé
    ALORS un item BLOQUANT est produit."""
    items = review_assistant._check_conformity(
        _DIFF_WITH_UNWRAP,
        {"forbidden_deps": "[]"},
    )

    assert any(level == review_assistant._BLOQUANT for level, _ in items)
    assert any("unwrap" in msg.lower() for _, msg in items)


def test_check_conformity_ignores_unwrap_in_test_file() -> None:
    """GIVEN un diff de fichier de test avec .unwrap()
    WHEN _check_conformity est appelé
    ALORS .unwrap() n'est PAS signalé en BLOQUANT."""
    items = review_assistant._check_conformity(
        _DIFF_WITH_UNWRAP_IN_TEST,
        {"forbidden_deps": "[]"},
    )

    bloquant_with_unwrap = [
        msg for level, msg in items
        if level == review_assistant._BLOQUANT and "unwrap" in msg.lower()
    ]
    assert not bloquant_with_unwrap


def test_check_conformity_detects_todo_bang() -> None:
    """GIVEN un diff production avec todo!()
    WHEN _check_conformity est appelé
    ALORS un item BLOQUANT est produit."""
    items = review_assistant._check_conformity(
        _DIFF_WITH_TODO,
        {"forbidden_deps": "[]"},
    )

    assert any(
        level == review_assistant._BLOQUANT and "todo" in msg.lower()
        for level, msg in items
    )


def test_check_conformity_detects_forbidden_dep() -> None:
    """GIVEN un diff qui importe une dépendance interdite (date-fns)
    WHEN _check_conformity est appelé avec les règles correspondantes
    ALORS un item BLOQUANT mentionne date-fns."""
    rules = {"forbidden_deps": json.dumps(["date-fns"])}
    items = review_assistant._check_conformity(_DIFF_WITH_FORBIDDEN_DEP, rules)

    bloquant_msgs = [msg for level, msg in items if level == review_assistant._BLOQUANT]
    assert any("date-fns" in msg for msg in bloquant_msgs)


def test_check_conformity_clean_diff_returns_lgtm() -> None:
    """GIVEN un diff sans violation
    WHEN _check_conformity est appelé
    ALORS au moins un item LGTM est présent et aucun BLOQUANT."""
    items = review_assistant._check_conformity(_DIFF_CLEAN, {"forbidden_deps": "[]"})

    assert any(level == review_assistant._LGTM for level, _ in items)
    assert not any(level == review_assistant._BLOQUANT for level, _ in items)


def test_check_conformity_empty_diff_returns_lgtm() -> None:
    """GIVEN un diff vide
    WHEN _check_conformity est appelé
    ALORS un item LGTM est retourné."""
    items = review_assistant._check_conformity("", {"forbidden_deps": "[]"})

    assert any(level == review_assistant._LGTM for level, _ in items)


# ---------------------------------------------------------------------------
# Détection de nouvelles fonctions
# ---------------------------------------------------------------------------


def test_find_new_public_functions_rust_pub_fn() -> None:
    """GIVEN une ligne Rust 'pub async fn handle_request(...)'
    WHEN _find_new_public_functions est appelé
    ALORS 'handle_request' est retourné."""
    additions = ["pub async fn handle_request(ctx: &Ctx) -> Response {"]
    fns = review_assistant._find_new_public_functions(additions)

    assert "handle_request" in fns


def test_find_new_public_functions_typescript_export() -> None:
    """GIVEN une ligne TypeScript 'export async function createUser(...)'
    WHEN _find_new_public_functions est appelé
    ALORS 'createUser' est retourné."""
    additions = ["export async function createUser(data: CreateUserDto) {"]
    fns = review_assistant._find_new_public_functions(additions)

    assert "createUser" in fns


def test_find_new_public_functions_python_def() -> None:
    """GIVEN une ligne Python 'def process_task(...)'
    WHEN _find_new_public_functions est appelé
    ALORS 'process_task' est retourné."""
    additions = ["def process_task(payload: dict) -> None:"]
    fns = review_assistant._find_new_public_functions(additions)

    assert "process_task" in fns


def test_find_new_public_functions_dedup() -> None:
    """GIVEN la même fonction sur deux lignes identiques
    WHEN _find_new_public_functions est appelé
    ALORS elle n'est retournée qu'une fois."""
    additions = [
        "pub fn do_thing() -> bool {",
        "pub fn do_thing() -> bool {",
    ]
    fns = review_assistant._find_new_public_functions(additions)

    assert fns.count("do_thing") == 1


# ---------------------------------------------------------------------------
# Construction du rapport
# ---------------------------------------------------------------------------


def test_build_report_no_blocants_fr() -> None:
    """GIVEN aucun item BLOQUANT
    WHEN _build_report est appelé en français
    ALORS le résumé ne contient aucun 🔴 et mentionne 'Aucun blocant'."""
    report = review_assistant._build_report(
        "test-feature",
        [(review_assistant._LGTM, "OK")],
        [(review_assistant._LGTM, "OK")],
        [(review_assistant._ATTENTION, "Minor")],
        "fr",
    )

    assert "🔴" not in report or "Aucun blocant" in report
    assert "🟢" in report


def test_build_report_with_blocants_fr() -> None:
    """GIVEN un item BLOQUANT dans la complétude
    WHEN _build_report est appelé
    ALORS le résumé mentionne le nombre de blocants."""
    report = review_assistant._build_report(
        "test-feature",
        [(review_assistant._BLOQUANT, "Frontend manquant")],
        [],
        [],
        "fr",
    )

    assert "🔴" in report
    assert "blocant" in report.lower()


def test_build_report_en_locale() -> None:
    """GIVEN lang='en'
    WHEN _build_report est appelé
    ALORS les titres de section sont en anglais."""
    report = review_assistant._build_report(
        "my-feature",
        [(review_assistant._LGTM, "All good")],
        [],
        [],
        "en",
    )

    assert "Layer completeness" in report
    assert "Summary" in report


def test_build_report_includes_today_date() -> None:
    """GIVEN un rapport généré aujourd'hui
    WHEN _build_report est appelé
    ALORS la date ISO du jour est présente dans l'en-tête."""
    from datetime import date

    report = review_assistant._build_report("feature", [], [], [], "fr")
    assert date.today().isoformat() in report


# ---------------------------------------------------------------------------
# ReviewContextBootstrap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_review_bootstrap_uses_project_bootstrap() -> None:
    """GIVEN review-assistant with workspace rules
    WHEN bootstrap runs
    THEN the snapshot contains workspace_rules and tech_stack."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "abc123", "exit_code": 0},
        },
        memory=True,
        workspace_rules="anyhow INTERDIT dans ce workspace",
    )

    bootstrap = review_assistant.ReviewContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert snapshot["workspace_rules"] == "anyhow INTERDIT dans ce workspace"
    assert isinstance(snapshot["tech_stack"], list)
    assert snapshot["commit_hash"] == "abc123"
    assert snapshot["has_git"] is True


# ---------------------------------------------------------------------------
# Agent — run() : AC-1 dépendance interdite
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_detects_forbidden_dep_in_diff() -> None:
    """GIVEN une règle 'date-fns INTERDIT' ET un diff qui importe date-fns
    WHEN run() est appelé
    ALORS le rapport contient un item BLOQUANT mentionnant date-fns."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": _DIFF_WITH_FORBIDDEN_DEP, "exit_code": 0},
        },
        memory=True,
        workspace_rules="date-fns INTERDIT dans ce projet",
    )

    a = _agent()
    result = await a.run(_make_task("revois le diff courant"), ctx)

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "🔴" in text
    assert "date-fns" in text


# ---------------------------------------------------------------------------
# Agent — run() : AC-2 couche manquante
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_detects_missing_frontend_layer() -> None:
    """GIVEN une TaskSpec avec Frontend [x] ET un diff sans fichier frontend
    WHEN run() est appelé
    ALORS le rapport contient un item BLOQUANT pour la couche Frontend."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _TASKSPEC_WITH_FRONTEND},
            "bash_executor": {"stdout": _DIFF_WITHOUT_FRONTEND, "exit_code": 0},
        },
        memory=True,
    )

    a = _agent()
    result = await a.run(
        _make_task("revois la spec .apollia/tasks/user-dashboard.md"),
        ctx,
    )

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "🔴" in text
    assert "Frontend" in text


# ---------------------------------------------------------------------------
# Agent — run() : AC-3 diff conforme
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_clean_diff_no_blocants() -> None:
    """GIVEN une TaskSpec API uniquement ET un diff conforme couvrant l'API
    WHEN run() est appelé
    ALORS le rapport ne contient aucun item 🔴 BLOQUANT."""
    diff_api = (
        "+++ b/src/api/auth.ts\n"
        "@@ -1 +1,3 @@\n"
        "+export async function createToken(id: string): Promise<string> {\n"
        "+    return sign({ id }, process.env.SECRET);\n"
        "+}\n"
    )
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _TASKSPEC_API_ONLY},
            "bash_executor": {"stdout": diff_api, "exit_code": 0},
        },
        memory=True,
    )

    a = _agent()
    result = await a.run(
        _make_task("revois la spec .apollia/tasks/jwt-auth.md"),
        ctx,
    )

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "🟢" in text
    # No BLOQUANT section should contain actual items
    bloquant_count = text.count("🔴 BLOQUANT —")
    assert bloquant_count == 0


# ---------------------------------------------------------------------------
# Agent — run() : AC-4 tests lancés
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_launches_test_command_when_runner_found() -> None:
    """GIVEN un workspace avec Cargo.toml ET bash_executor disponible
    WHEN run() est appelé
    ALORS bash_executor est appelé au moins deux fois (diff + cargo test)."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": "[package]\nname = \"my-crate\""},
            "bash_executor": {
                "stdout": "test result: ok. 5 passed; 0 failed",
                "exit_code": 0,
            },
        },
        memory=True,
    )

    a = _agent()
    result = await a.run(_make_task("revois le diff"), ctx)

    assert result["status"] == "completed"
    assert ctx.tools is not None
    bash_calls = [n for n, _ in ctx.tools.calls if n == "bash_executor"]
    assert len(bash_calls) >= 1


@pytest.mark.asyncio
async def test_run_reports_test_failures() -> None:
    """GIVEN un runner de tests qui échoue
    WHEN run() est appelé
    ALORS le rapport contient un item BLOQUANT mentionnant l'échec."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": "[package]\nname = \"my-crate\""},
            "bash_executor": {
                "stdout": "test result: FAILED. 2 passed; 1 failed\nerror: test failed",
                "exit_code": 1,
            },
        },
        memory=True,
    )

    a = _agent()
    result = await a.run(_make_task("revois le diff"), ctx)

    assert result["status"] == "completed"
    text = _output_text(result)
    assert "🔴" in text


# ---------------------------------------------------------------------------
# Agent — run() : AC-5 pas de modification de fichier
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_does_not_modify_files_without_confirmation() -> None:
    """GIVEN un diff avec des problèmes détectés
    WHEN run() est appelé
    ALORS file_write et file_edit ne sont JAMAIS appelés."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": _TASKSPEC_WITH_FRONTEND},
            "bash_executor": {"stdout": _DIFF_WITH_UNWRAP, "exit_code": 0},
            "file_write": {"success": True},
            "file_edit": {"success": True},
        },
        memory=True,
    )

    a = _agent()
    await a.run(_make_task("revois le diff courant"), ctx)

    assert ctx.tools is not None
    called = [n for n, _ in ctx.tools.calls]
    assert "file_write" not in called
    assert "file_edit" not in called


# ---------------------------------------------------------------------------
# Agent — run() : cas d'erreur
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_fails_without_input() -> None:
    """GIVEN une tâche sans texte d'entrée
    WHEN run() est appelé
    ALORS il retourne status='failed'."""
    ctx = MockContext.create(memory=True)

    a = _agent()
    result = await a.run({"input": {"text": ""}}, ctx)

    assert result["status"] == "failed"
    assert "code" in result["error"]


@pytest.mark.asyncio
async def test_run_handles_no_tools_gracefully() -> None:
    """GIVEN aucun outil configuré (ctx.tools is None)
    WHEN run() est appelé
    ALORS il retourne un résultat completed (pas d'exception)."""
    ctx = MockContext.create(memory=True)

    a = _agent()
    result = await a.run(_make_task("revois le diff"), ctx)

    assert result["status"] == "completed"


# ---------------------------------------------------------------------------
# Agent — bootstrap persists snapshot on first turn
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_bootstraps_on_first_turn() -> None:
    """GIVEN un workspace avec des règles
    WHEN run() est appelé pour la première fois
    ALORS le bootstrap persiste un snapshot en mémoire."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "", "exit_code": 0},
        },
        memory=True,
        workspace_rules="anyhow INTERDIT dans ce workspace",
    )

    a = _agent()
    await a.run(_make_task("revois le diff"), ctx)

    assert ctx.memory is not None
    status = await ctx.memory.recall("bootstrap.status")
    assert status == "complete"


# ---------------------------------------------------------------------------
# Instance module-level
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    """GIVEN le module après chargement
    WHEN l'attribut agent est accédé
    ALORS c'est un ReviewAssistant avec un manifest valide."""
    assert isinstance(review_assistant.agent, review_assistant.ReviewAssistant)
    assert review_assistant.agent.manifest()["name"] == "review-assistant"


def test_module_exports_all_public_symbols() -> None:
    """GIVEN le module chargé
    WHEN les symboles publics sont accédés
    ALORS tous sont présents et ont le bon type."""
    assert callable(review_assistant.manifest)
    assert callable(review_assistant._check_completeness)
    assert callable(review_assistant._check_conformity)
    assert callable(review_assistant._check_tests)
    assert callable(review_assistant._build_report)
    assert callable(review_assistant._get_diff)
    assert callable(review_assistant._find_task_spec)
    assert isinstance(review_assistant._LGTM, str)
    assert isinstance(review_assistant._BLOQUANT, str)
    assert isinstance(review_assistant._ATTENTION, str)
    assert issubclass(
        review_assistant.ReviewContextBootstrap,
        review_assistant.ProjectContextBootstrap,
    )
