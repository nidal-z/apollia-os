# Tests et validation

Un Worker Agent sans suite de tests n'est pas publiable. Les tests servent deux objectifs distincts : valider la structure statique de l'agent (manifest, guardrails), et valider son comportement dynamique (happy path, cas d'erreur domaine).

Le registre communautaire exige au minimum un test du cas d'erreur. En pratique, trois tests sont le minimum raisonnable.

---

## Structure du fichier de test

```
agents/tests/
├── conftest.py              ← Fixtures partagées (MockCtx, MockTools, MockLlm)
└── test_csv_data_worker.py  ← Tests de csv-data-worker
```

`conftest.py` est partagé entre tous les agents. Il fournit les mocks du runtime — vous n'avez pas besoin de démarrer Apollia OS pour tester votre agent.

---

## Les trois tests minimaux

### Test 1 — Validation du manifest

Vérifie que `manifest()` retourne un dict conforme : champs obligatoires présents, outils déclarés, skills A2A valides.

```python
import importlib.util
from pathlib import Path

_AGENT_PATH = Path(__file__).parent.parent / "csv-data-worker.py"
spec = importlib.util.spec_from_file_location("csv_data_worker", _AGENT_PATH)
assert spec and spec.loader
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def test_manifest_is_valid():
    m = mod.manifest()

    # Identité
    assert m["name"] == "csv-data-worker"
    assert "version" in m and m["version"]

    # Outils
    assert "python_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]

    # Packages
    assert any("pandas" in pkg for pkg in m.get("packages", []))

    # A2A
    assert m["supports_a2a"] is True
    assert len(m["skills"]) >= 1

    skill_ids = [s["id"] for s in m["skills"]]
    assert "analyze-csv" in skill_ids
```

### Test 2 — Validation statique des guardrails

Vérifie que le `SYSTEM_PROMPT` contient bien des guardrails correctement formulés — sans exécuter l'agent.

```python
def test_system_prompt_has_guardrails():
    prompt = mod.SYSTEM_PROMPT

    # Structure de base
    assert "JAMAIS" in prompt or "TOUJOURS" in prompt, \
        "Le SYSTEM_PROMPT doit contenir au moins un JAMAIS ou TOUJOURS"
    assert "RAISON" in prompt, \
        "Chaque guardrail doit avoir une RAISON explicite"

    # Guardrails spécifiques au domaine CSV
    assert "bash_executor" in prompt, \
        "Le guardrail interdisant bash_executor doit être dans le SYSTEM_PROMPT"
    assert "latin-1" in prompt or "latin1" in prompt, \
        "La détection d'encodage latin-1 doit être dans le SYSTEM_PROMPT"
```

### Test 3 — Happy path avec mocks

Vérifie que le ReAct loop s'exécute correctement pour une tâche d'analyse CSV nominale, sans démarrer le runtime.

```python
import json
import pytest
from conftest import MockCtx, MockTools, MockLlm


@pytest.mark.asyncio
async def test_happy_path_analyze_csv(tmp_path):
    # GIVEN — un fichier CSV valide
    csv_file = tmp_path / "ventes.csv"
    csv_file.write_text("region,montant\nNord,1000\nSud,2000\n")

    tools = MockTools()
    tools.set_result("python_executor", {
        "stdout": "lignes=2, colonnes=['region', 'montant']",
        "stderr": "",
        "exit_code": 0,
    })
    tools.set_result("file_read", {"content": csv_file.read_text()})

    llm = MockLlm([
        json.dumps({
            "thought": "Je vais lire le CSV avec pandas",
            "action": "tool_call",
            "tool": "python_executor",
            "args": {
                "code": f"import pandas as pd; df = pd.read_csv('{csv_file}'); print(df.shape)",
                "timeout_secs": 30,
            },
        }),
        json.dumps({
            "thought": "Analyse terminée",
            "action": "final_answer",
            "text": "Fichier lu : 2 lignes, colonnes : region, montant. Total montant : 3000.",
        }),
    ])

    ctx = MockCtx(tools=tools, llm=llm)

    # WHEN
    result = await mod.agent.run(
        {"input": {"text": f"Analyse {csv_file}"}},
        ctx
    )

    # THEN
    assert result["status"] == "completed"
    assert tools.called_with("python_executor")
```

---

## Tests des cas d'erreur domaine

Ce sont les tests les plus importants pour la robustesse. Chaque cas d'erreur domaine identifié dans le `SYSTEM_PROMPT` mérite son propre test.

```python
@pytest.mark.asyncio
async def test_file_not_found_returns_structured_error():
    # GIVEN — un chemin inexistant
    ctx = MockCtx(tools=MockTools(), llm=MockLlm([]))

    # WHEN
    result = await mod.agent.run(
        {"input": {"text": "Analyse /tmp/inexistant.csv"}},
        ctx
    )

    # THEN — l'agent retourne une erreur structurée, pas une exception
    assert result["status"] == "failed"
    assert result["error"]["code"] == "file_not_found"


@pytest.mark.asyncio
async def test_empty_csv_returns_structured_error():
    # GIVEN — un fichier CSV vide
    tools = MockTools()
    tools.set_result("python_executor", {
        "stdout": "",
        "stderr": "pandas.errors.EmptyDataError: No columns to parse from file",
        "exit_code": 1,
    })

    llm = MockLlm([
        json.dumps({
            "thought": "Le fichier est vide",
            "action": "final_answer",
            "text": "domain_error:empty_file:Le fichier CSV est vide",
        }),
    ])

    ctx = MockCtx(tools=tools, llm=llm)

    # WHEN — le test utilise un vrai fichier vide pour déclencher la validation
    import tempfile, os
    with tempfile.NamedTemporaryFile(suffix=".csv", delete=False) as f:
        f.write(b"")
        empty_path = f.name

    try:
        result = await mod.agent.run(
            {"input": {"text": f"Analyse {empty_path}"}},
            ctx
        )
        assert result["status"] in ("failed", "completed")
    finally:
        os.unlink(empty_path)
```

---

## Fixtures de domaine dans conftest.py

`conftest.py` fournit des fixtures avec de vrais fichiers pour les tests qui nécessitent des données réalistes :

| Fixture | Contenu |
|---|---|
| `csv_utf8_file` | CSV UTF-8, séparateur virgule, 10 lignes |
| `csv_latin1_file` | CSV latin-1 (simule export Excel Windows), avec accents |
| `csv_semicolon_file` | CSV séparateur point-virgule (format européen) |
| `excel_file` | `.xlsx` valide avec 10 lignes (pour tester le rejet de format) |

```python
@pytest.fixture
def csv_latin1_file(tmp_path):
    path = tmp_path / "ventes_fr.csv"
    path.write_bytes("région;montant\nÎle-de-France;12500\nProvence;8300\n".encode("latin-1"))
    return path


@pytest.mark.asyncio
async def test_latin1_csv_is_decoded_correctly(csv_latin1_file):
    tools = MockTools()
    tools.set_result("python_executor", {
        "stdout": "lignes=2, régions=['Île-de-France', 'Provence']",
        "stderr": "",
        "exit_code": 0,
    })
    llm = MockLlm([
        json.dumps({
            "thought": "Je détecte l'encodage",
            "action": "tool_call",
            "tool": "python_executor",
            "args": {"code": "...", "timeout_secs": 30},
        }),
        json.dumps({
            "thought": "Terminé",
            "action": "final_answer",
            "text": "Fichier lu en latin-1 : 2 lignes, colonnes : région, montant.",
        }),
    ])
    ctx = MockCtx(tools=tools, llm=llm)

    result = await mod.agent.run(
        {"input": {"text": f"Analyse {csv_latin1_file}"}},
        ctx
    )
    assert result["status"] == "completed"
```

---

## Tests live avec le runtime

Une fois les tests unitaires passés, testez l'agent avec le runtime réel :

```bash
# 1. Démarrer l'agent
apollia-os agent start agents/csv-data-worker.py

# 2. Vérifier qu'il est actif
apollia-os agent list
# csv-data-worker  [Active]  skills: read-csv, analyze-csv, transform-csv

# 3. Test simple
apollia-os agent run csv-data-worker "Analyse /tmp/ventes.csv"

# 4. Test via A2A (depuis la CLI)
apollia-os a2a delegate analyze-csv '{"input": {"text": "Analyse /tmp/ventes.csv"}}'

# 5. Vérifier les logs
apollia-os audit stats --filter csv-data-worker

# 6. Arrêter
apollia-os agent stop csv-data-worker
```

---

## Lancer tous les tests avant la publication

```bash
# Tests de l'agent uniquement
pytest agents/tests/test_csv_data_worker.py -v

# Sortie attendue :
# PASSED test_manifest_is_valid
# PASSED test_system_prompt_has_guardrails
# PASSED test_happy_path_analyze_csv
# PASSED test_file_not_found_returns_structured_error
# PASSED test_empty_csv_returns_structured_error
# PASSED test_latin1_csv_is_decoded_correctly
# 6 passed in 0.8s
```

Un seul test en échec bloque l'installation via `apollia-os agent install`. C'est voulu.
