# Worker Agent Pattern — Guide pour les builders Apollia OS

> Ce guide documente le pattern Worker Agent : quand l'utiliser, comment l'implémenter,
> comment le tester, et comment choisir entre Worker Agent et MCP tool.

---

## Définition

Un **Worker Agent** est un agent spécialisé dont l'expertise de domaine est **compilée dans le code Python** — pas injectée en contexte LLM à l'exécution.

L'expertise prend la forme d'un `SYSTEM_PROMPT` constant contenant :
- Les guardrails non-contournables (ce que l'agent ne doit **jamais** faire)
- Les imports corrects de la librairie cible
- Les patterns d'usage obligatoires (code Python de référence)
- La gestion des erreurs domaine (exceptions connues, messages clairs)
- Le format de réponse attendu

Un Worker Agent hérite de `WorkerAgent` (qui étend `BaseReActAgent`) et expose les mêmes deux méthodes contractuelles : `manifest()` + `run()`.

### Pourquoi "compiler" l'expertise ?

Sur les modèles frontier (Claude, GPT-4o), le LLM improvise correctement même sans instructions spécifiques. Sur les modèles légers 7–14B utilisés en local :

| Problème | Agent générique | Worker Agent |
|---|---|---|
| Hallucination API | `openpyxl.open_workbook()` (inexistant) | Import correct dans le prompt |
| Guardrails oubliés | "ne jamais bash" ignoré après 4 étapes | Règle compilée, toujours présente |
| Séquençage incorrect | `wb.save()` oublié | Pattern obligatoire dans le prompt |
| Contexte saturé | 4K tokens → instructions tronquées | SYSTEM_PROMPT statique, jamais perdu |

---

## Quand créer un Worker Agent (règle des 2 sur 3)

Créer un Worker Agent si **au moins 2 des 3 conditions** suivantes sont réunies :

1. **Séquence non-triviale** : La tâche impose un ordre d'opérations critique que le LLM oublie régulièrement (ex. inspecter les feuilles avant d'accéder à une feuille, appeler `wb.save()` après modification).

2. **Guardrail critique** : Il existe une façon dangereuse ou silencieusement incorrecte de faire la tâche que le LLM emprunte naturellement (ex. utiliser `bash` pour modifier un `.xlsx` corrupt l'archive ZIP).

3. **Pattern d'erreur domaine récurrent** : Le domaine produit des exceptions spécifiques avec des causes non-évidentes pour le LLM (ex. `UnicodeDecodeError` sur CSV français, `BadZipFile` sur Excel corrompu).

---

## Worker Agent vs MCP tool : tableau de décision

| Situation | Mécanisme recommandé |
|---|---|
| Opération atomique, API simple, 1 appel | MCP tool |
| Tâche multi-étapes avec ordre imposé | Worker Agent |
| Guardrail critique à respecter impérativement | Worker Agent |
| Librairie Python tierce nécessaire | Worker Agent (via `packages`) |
| Intégration d'un service externe (Slack, GitHub…) | MCP tool |
| Format de fichier complexe (Excel, PDF, CSV encodé) | Worker Agent |
| Simple lecture/écriture fichier texte | MCP tool (`file_read` / `file_write`) |
| Calcul pur sans I/O | MCP tool ou `python_executor` direct |
| Expertise domaine réutilisable entre agents | Worker Agent |

---

## Template de manifest

```python
def manifest() -> dict:
    return {
        "name": "mon-worker",
        "version": "0.1.0",
        "description": (
            "Description courte de l'agent et de son domaine. "
            "Préciser les formats gérés et les LLMs supportés."
        ),
        "execution_mode": "direct",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": ["file_write", "file_list"],
        "tools_requiring_approval": [],          # outils déclenchant HITL
        "packages": ["ma-lib>=1.0.0"],           # pip install automatique
        "memory_namespace": "mon-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "skill-principal",
                "name": "Nom lisible",
                "description": "Ce que fait ce skill, en une phrase.",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
        ],
        "tags": ["domaine", "format", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }
```

---

## Comment déclarer des packages pip

Le champ `packages` dans le manifest liste les dépendances pip à installer dans le venv Python de l'agent. Le runtime Apollia appelle `python_executor.setup_venv()` automatiquement lors de la phase `INITIALIZING`.

```python
"packages": ["openpyxl>=3.1.0"],          # version minimale recommandée
"packages": ["pandas>=2.0.0"],             # spécifier la version majeure minimum
"packages": ["requests>=2.31.0", "lxml"],  # plusieurs packages possibles
```

**Règles :**
- Utiliser la syntaxe pip standard : `nom>=version`, `nom==version`, `nom`
- Préférer `>=` à `==` pour ne pas bloquer les mises à jour de sécurité
- Ne déclarer que les packages **non-inclus** dans la stdlib Python
- Si l'installation échoue, l'agent démarre en état `DEGRADED` (non bloquant)

---

## Comment écrire un SYSTEM_PROMPT efficace

Un SYSTEM_PROMPT de Worker Agent a une structure fixe en 5 sections.

### 1. Identité et domaine

```
Tu es mon-worker, un agent expert de [domaine] via [librairie].
```

### 2. Guardrails (règles non-négociables)

Les guardrails sont la section la plus importante. Ils doivent :
- Commencer par `JAMAIS` ou `TOUJOURS` (verbe fort)
- Inclure la **RAISON** immédiatement après la règle
- Couvrir le cas d'erreur silencieuse le plus dangereux du domaine

```python
SYSTEM_PROMPT = """Tu es mon-worker, ...

## RÈGLES ABSOLUES (non-négociables)

1. N'utilise JAMAIS [outil dangereux] pour [opération].
   RAISON : [Conséquence silencieuse et irreversible].

2. Utilise TOUJOURS [pattern correct].
   RAISON : [Pourquoi le pattern alternatif échoue].
"""
```

### 3. Patterns obligatoires (blocs de code)

Inclure les snippets Python exacts pour les 2–3 opérations les plus courantes :

```python
"""
## PATTERNS OBLIGATOIRES

### Lire un fichier
```python
from ma_lib import open_file
with open_file(path) as f:
    data = f.read()
```

### Modifier et sauvegarder
```python
obj = load(path)       # charger
obj.field = value      # modifier
obj.save(path)         # OBLIGATOIRE — sans ça les changements sont perdus
```
"""
```

### 4. Gestion des erreurs domaine

Mapper les exceptions connues vers des messages utilisateur clairs :

```python
"""
## GESTION DES ERREURS DOMAINE

- `FileNotFoundError` → message : "Fichier introuvable : {path}"
- `BadFormatError` → informer que le fichier est corrompu ou mal formé
- `KeyError` sur champ → lister les champs disponibles
"""
```

### 5. Format de réponse

Préciser ce que le LLM doit inclure dans sa réponse finale :

```python
"""
## FORMAT DE RÉPONSE

- Indiquer toujours [information contextuelle obligatoire]
- Pour [type de résultat] : présenter en [format]
- Pour les erreurs : message clair avec le path et la raison précise
"""
```

---

## Comment tester un Worker Agent

### MockCtx

`MockCtx` (depuis `agents/tests/conftest.py`) est le contexte de test léger :

```python
import importlib.util
from pathlib import Path
from conftest import MockCtx, MockTools, MockLlm
import json

# Charger l'agent (nom de fichier avec tiret → importlib)
_AGENT_PATH = Path("agents/mon-worker.py")
spec = importlib.util.spec_from_file_location("mon_worker", _AGENT_PATH)
assert spec and spec.loader
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
```

### Tester les guardrails statiquement

Les guardrails sont testables sans exécuter l'agent — il suffit d'inspecter le texte du `SYSTEM_PROMPT` :

```python
def test_guardrail_bash_forbidden():
    prompt = mod.SYSTEM_PROMPT
    assert "JAMAIS bash" in prompt or "JAMAIS bash_executor" in prompt
```

### Tester le happy path

```python
import pytest

@pytest.mark.asyncio
async def test_happy_path():
    tools = MockTools()
    tools.set_result("python_executor", {
        "stdout": "Résultat attendu",
        "stderr": "",
        "exit_code": 0,
    })
    llm = MockLlm([
        json.dumps({
            "thought": "Je vais utiliser python_executor",
            "action": "tool_call",
            "tool": "python_executor",
            "args": {"code": "...", "timeout_secs": 30},
        }),
        json.dumps({
            "thought": "Terminé",
            "action": "final_answer",
            "text": "Résultat : ...",
        }),
    ])
    ctx = MockCtx(tools=tools, llm=llm)

    agent = mod.MyWorkerAgent()
    result = await agent.run({"input": {"text": "Ma tâche"}}, ctx)

    assert result["status"] == "completed"
    assert tools.called_with("python_executor")
```

### Tester les cas d'erreur domaine

```python
@pytest.mark.asyncio
async def test_corrupted_file():
    tools = MockTools()
    tools.set_result("python_executor", {
        "stdout": "",
        "stderr": "SomeError: fichier invalide",
        "exit_code": 1,
    })
    llm = MockLlm([
        json.dumps({"action": "tool_call", "tool": "python_executor", "args": {}}),
        json.dumps({"action": "final_answer", "text": "Erreur : fichier invalide"}),
    ])
    ctx = MockCtx(tools=tools, llm=llm)

    agent = mod.MyWorkerAgent()
    result = await agent.run({"input": {"text": "Lis fichier_corrompu.ext"}}, ctx)

    # L'agent doit retourner un résultat valide — jamais lever d'exception
    assert result["status"] in ("failed", "completed")
```

### Fixtures fichiers (tests avec vrais fichiers)

Les fixtures `excel_file`, `csv_utf8_file`, `csv_latin1_file` (depuis `conftest.py`)
génèrent des fichiers réels dans `tmp_path` via pytest :

```python
@pytest.mark.asyncio
async def test_with_real_file(excel_file):
    # excel_file est un Path vers un .xlsx valide avec la feuille "Ventes"
    tools = MockTools()
    tools.set_result("python_executor", {"stdout": "ok", "stderr": "", "exit_code": 0})
    llm = MockLlm([json.dumps({"action": "final_answer", "text": "Lu."})])
    ctx = MockCtx(tools=tools, llm=llm)

    agent = mod.ExcelWorkerAgent()
    result = await agent.run(
        {"input": {"text": f"Lis {excel_file}"}},
        ctx,
    )
    assert result["status"] == "completed"
```

---

## Worker Agents disponibles

| Agent | Domaine | Packages requis | Skills |
|---|---|---|---|
| `excel-worker` | Fichiers Excel `.xlsx` / `.xlsm` | `openpyxl>=3.1.0` | read-excel, edit-excel, analyze-excel |
| `csv-data-worker` | Fichiers CSV (multi-encodage, multi-séparateur) | `pandas>=2.0.0` | read-csv, analyze-csv, transform-csv |

### `excel-worker`

Spécialité : manipulation de classeurs Excel via openpyxl. Guardrail central : n'utilise jamais `bash_executor` pour lire ou modifier un `.xlsx` (un `.xlsx` est une archive ZIP — bash corromprait silencieusement l'archive).

Démarrage : `apollia-os agent start agents/excel-worker.py`

### `csv-data-worker`

Spécialité : analyse et transformation de CSVs via pandas. Gère automatiquement la détection d'encodage (UTF-8, latin-1, utf-8-sig) et de séparateur (`,` ou `;`). Guardrail central : toujours inspecter `df.dtypes` avant tout calcul numérique.

Démarrage : `apollia-os agent start agents/csv-data-worker.py`

---

## Exemples complets

### Structure minimale d'un Worker Agent

```python
"""mon-worker — [description courte]."""
from __future__ import annotations
from typing import Any
from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es mon-worker, expert de [domaine].

## RÈGLES ABSOLUES

1. N'utilise JAMAIS [outil dangereux].
   RAISON : [conséquence].

## PATTERNS OBLIGATOIRES

```python
# pattern correct ici
```

## GESTION DES ERREURS DOMAINE

- `SomeError` → message clair à l'utilisateur
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for mon-worker."""
    return {
        "name": "mon-worker",
        "version": "0.1.0",
        "description": "...",
        "execution_mode": "direct",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "packages": ["ma-lib>=1.0.0"],
        "memory_namespace": "mon-worker",
        "supports_a2a": True,
        "skills": [{"id": "main-skill", "name": "...", "description": "...",
                    "input_modes": ["text"], "output_modes": ["text"]}],
        "tags": ["domaine", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class MonWorkerAgent(WorkerAgent):
    """Worker Agent spécialisé pour [domaine]."""

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for mon-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the task using the ReAct loop."""
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = MonWorkerAgent()
```

---

## Routing A2A — appeler un Worker depuis un Director

Un Director Agent peut déléguer une tâche à un Worker Agent par `skill_id` sans en connaître le
nom ni l'état, via `ctx.delegate()`.

### Prérequis manifest du Worker

```python
"supports_a2a": True,
"skills": [
    {
        "id": "analyze-csv",
        "name": "Analyse CSV",
        "description": "Analyse un fichier CSV et retourne des statistiques.",
        "input_modes": ["text"],
        "output_modes": ["text"],
    },
],
```

Le champ `supports_a2a: true` est obligatoire. Sans lui, l'agent est invisible au router A2A.

### Appel depuis le Director

```python
async def run(self, task, ctx):
    result = await ctx.delegate(
        "analyze-csv",
        {"input": {"text": "Analyse /data/ventes.csv"}},
        timeout_secs=120,
    )
    # result est un dict : {"status": "completed", "output": [...], ...}
    return AIPResult.completed(f"Analyse terminée : {result}")
```

Ou via le helper `WorkerAgent.delegate_skill()` :

```python
result = await self.delegate_skill(ctx, "analyze-csv", payload)
```

### Erreurs possibles

| Situation | Erreur levée |
|---|---|
| Skill non trouvé | `RuntimeError: skill 'X' not found — available: [...]` |
| Skill déclaré par 2+ agents actifs | `RuntimeError: ambiguous skill 'X' — declared by: [A, B]` |
| Timeout dépassé | `RuntimeError: delegation timed out after N seconds` |
| `supports_a2a: false` dans manifest | `RuntimeError: A2A delegation requires supports_a2a: true` |

### CLI : lister les agents A2A

```bash
apollia-os agent list --supports-a2a
# A2A-capable agents (2):
#   csv-data-worker  [Active]
#     - analyze-csv: Analyse un fichier CSV
#     - transform-csv: Transforme et filtre les données d'un CSV
#   excel-worker  [Active]
#     - read-excel: Lit un classeur Excel
```

---

### Checklist avant de soumettre un Worker Agent

- [ ] `SYSTEM_PROMPT` contient au moins 1 guardrail avec `RAISON`
- [ ] `manifest()["packages"]` liste toutes les dépendances pip
- [ ] `manifest()["tools_required"]` contient `python_executor` si des packages sont utilisés
- [ ] `MAX_STEPS` est défini (valeur conseillée : 6–10 selon la complexité)
- [ ] `TEMPERATURE` est bas (0.0–0.2) pour des résultats déterministes
- [ ] `agent = MonWorkerAgent()` est déclaré au niveau du module
- [ ] Tests : manifest valide, guardrail vérifié, happy path, cas d'erreur
- [ ] Si `supports_a2a: True` : au moins 1 skill déclaré dans `manifest()["skills"]`
- [ ] Si `supports_a2a: True` : `skill_id` unique dans l'ensemble des agents déployés
