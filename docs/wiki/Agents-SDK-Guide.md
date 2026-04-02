# Agents — Python SDK Guide — Apollia OS

> Guide complet du SDK Python `apollia-sdk` pour le développement d'agents.
> Cible : développeur Python souhaitant construire des agents structurés et testables.

---

## Vue d'ensemble

Le SDK Apollia fournit des classes de base, des type stubs, des utilitaires de parsing/formatting, une infrastructure de test et un outil de scaffolding pour développer des agents professionnels sur Apollia OS.

**Caractéristiques :**
- Pur Python, zéro dépendance runtime (ADR-037)
- Type stubs PEP 561 pour `RuntimeContext`, `ToolProxy`, `LlmProxy`, `MemoryInterface`
- 4 classes de base : `BaseReActAgent`, `ConversationalAgent`, `OrchestratedAgent`, `WorkerAgent`
- Mocks + assertions pour tests unitaires sans runtime
- CLI scaffolding : `apollia-os agent new <name> --type react` (ou `python -m apollia new <name>`)
- Compatible Python 3.10+

---

## Installation

```bash
# Depuis le répertoire racine d'Apollia OS
$ pip install -e ./sdk

# Vérifier
$ python3 -c "import apollia; print(apollia.__version__)"
0.1.0
```

Le SDK est un package Python standard dans `sdk/`. Il n'a aucune dépendance Rust — seul Python 3.10+ est requis.

---

## Structure du package

```
sdk/apollia/
├── __init__.py            ← AIPResult, __version__
├── types.py               ← AIPResult dataclass
├── agents/
│   ├── react.py           ← BaseReActAgent
│   ├── conversational.py  ← ConversationalAgent
│   ├── orchestrated.py    ← OrchestratedAgent
│   └── worker.py          ← WorkerAgent
├── utils/
│   ├── parsing.py         ← extract_json, truncate, validate_action
│   ├── formatting.py      ← format_as_markdown, aip_result_text
│   └── hitl.py            ← resume_pending_tool
├── tools/
│   └── schemas.py         ← NATIVE_TOOL_SCHEMAS, build_tools_block
├── testing/
│   ├── mocks.py           ← MockContext, MockToolProxy, MockLlmProxy, MockMemory
│   └── assertions.py      ← assert_result_completed, assert_tool_called
├── stubs/
│   ├── context.pyi        ← RuntimeContext
│   ├── tools.pyi          ← ToolProxy
│   ├── llm.pyi            ← LlmProxy
│   └── memory.pyi         ← MemoryInterface
├── cli/
│   ├── __main__.py        ← Entry point: python -m apollia
│   └── scaffold.py        ← scaffold_agent, templates
└── py.typed               ← PEP 561 marker
```

---

## 1. Classes de base

### 1.1 `BaseReActAgent`

Boucle Reason-Act-Observe avec LLM et outils.

```python
from apollia.agents import BaseReActAgent, AIPResult

class MonAgent(BaseReActAgent):
    SYSTEM_PROMPT = "Tu es un assistant spécialisé en analyse de code."
    MAX_STEPS = 15
    TEMPERATURE = 0.3

    def manifest(self):
        return {
            "name": "mon-agent",
            "version": "1.0.0",
            "description": "Agent d'analyse de code",
            "tools_required": ["bash_executor", "file_io"],
            "memory_namespace": "mon-agent",
            "execution_mode": "direct",
        }

    async def run(self, task, ctx):
        user_msg = task["input"]["parts"][0]["text"]
        result = await self.react(task, ctx, user_msg)

        if isinstance(result, dict):
            return result  # AIPResult (HITL ou erreur)
        return AIPResult.completed(result).to_dict()

agent = MonAgent()
```

**Méthodes clés :**

| Méthode | Description |
|---|---|
| `react(task, ctx, user_message, *, extra_context="", pending_tool=None)` | Exécute la boucle ReAct complète. **Retourne** `str` (texte final) ou `dict` (AIPResult complet, ex: HITL `input_required`) |
| `get_tool_schemas()` | Retourne les schémas des outils natifs |
| `manifest()` | (abstraite) Retourne le manifest AIP |
| `run(task, ctx)` | (abstraite) Point d'entrée principal |

**Dégradation gracieuse :**
- `ctx.llm is None` → retourne `AIPResult.failed("NO_LLM", ...)`
- `ctx.tools is None` → la boucle fonctionne, les appels outils retournent une erreur en observation
- `ctx.memory is None` → HITL reprend depuis une boucle vierge

### 1.2 `ConversationalAgent`

Agent dialogue uniquement, sans outils. Idéal pour des assistants conversationnels.

```python
from apollia.agents import ConversationalAgent

class AssistantAgent(ConversationalAgent):
    SYSTEM_PROMPT = "Tu es un assistant amical et utile."
    MAX_TURNS = 20
    TEMPERATURE = 0.7

    def manifest(self):
        return {
            "name": "assistant",
            "version": "1.0.0",
            "description": "Assistant conversationnel",
            "tools_required": [],
        }

    def on_response(self, response):
        """Post-traitement optionnel de la réponse LLM."""
        return response

agent = AssistantAgent()
```

**Méthodes clés :**

| Méthode | Description |
|---|---|
| `converse(ctx, user_message, history=None)` | Envoie un message et retourne `(response, updated_history)` |
| `run(task, ctx)` | Appelle `converse()` et retourne `AIPResult.completed()` |
| `on_response(response)` | (overridable) Post-traitement de la réponse |

**Exigence :** `ctx.llm` doit être disponible. Lève `RuntimeError` sinon.

### 1.3 `OrchestratedAgent`

Agent piloté par ORIA en mode Orchestré. ORIA génère le plan, exécute les outils, et appelle `on_plan_complete()` en fin de plan.

```python
from apollia.agents import OrchestratedAgent

class AnalyseAgent(OrchestratedAgent):
    def manifest(self):
        return {
            "name": "analyse-contrat",
            "version": "1.0.0",
            "description": "Analyse des contrats via ORIA",
            "tools_required": ["file_io"],
            "execution_mode": "orchestrated",
            "system_prompt": "Tu analyses des contrats juridiques.",
        }

    def on_plan_complete(self, step_results):
        """Post-traitement après exécution du plan ORIA."""
        summary = self.format_step_results(step_results)
        return {"text": f"Analyse terminée :\n{summary}"}

agent = AnalyseAgent()
```

**Méthodes clés :**

| Méthode | Description |
|---|---|
| `on_plan_complete(step_results)` | (overridable) Post-traitement des résultats du plan |
| `format_step_results(results)` | (statique) Formate les résultats en texte lisible |
| `run(task, ctx)` | Lève `RuntimeError` — ORIA gère l'exécution |

### 1.4 `WorkerAgent` *(Sprint 32)*

Agent spécialisé dans un domaine métier. Hérite de `BaseReActAgent` — même boucle ReAct, même contrat AIP. La différence est dans les **helpers** fournis et la convention de `SYSTEM_PROMPT` structuré.

#### Héritage

```
BaseReActAgent
    └── WorkerAgent         ← ajoute helpers tools + domain errors
```

`WorkerAgent` ne redéfinit pas la boucle ReAct — il l'enrichit avec des méthodes utilitaires qui éliminent le boilerplate des `ctx.tools.call()` répétitifs.

#### Exemple minimal

```python
from apollia.agents import AIPResult, WorkerAgent

class ExcelAgent(WorkerAgent):
    SYSTEM_PROMPT = """
    Tu es ExcelAgent, un expert Python/openpyxl.

    ## RÈGLES ABSOLUES
    1. Toujours utiliser openpyxl pour lire/écrire des fichiers Excel.
    2. Retourner domain_error("file_not_found", ...) si le fichier est absent.

    ## FORMAT DE RÉPONSE
    - Indiquer le nombre de lignes/colonnes lues.
    """
    MAX_STEPS = 8       # plus court qu'un agent générique
    TEMPERATURE = 0.1   # plus déterministe pour du code Python

    def manifest(self):
        return {
            "name": "excel-agent",
            "version": "1.0.0",
            "description": "Analyse et génère des fichiers Excel",
            "tools_required": ["python_executor", "file_read"],
            "packages": ["openpyxl>=3.1.0"],  # pip installé au démarrage
            "supports_a2a": True,              # accessible via A2A routing
            "skills": [
                {
                    "id": "read-excel",
                    "name": "Lecture Excel",
                    "description": "Lit et analyse un fichier .xlsx",
                    "input_modes": ["text", "data"],
                    "output_modes": ["data", "text"],
                }
            ],
        }

    async def run(self, task, ctx):
        user_msg = task["input"]["parts"][0]["text"]
        result = await self.react(task, ctx, user_msg)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result).to_dict()

agent = ExcelAgent()
```

#### Constantes de classe

| Constante | Défaut recommandé | Description |
|---|---|---|
| `SYSTEM_PROMPT` | *voir ci-dessous* | Prompt expert compilé — guardrails, patterns, gestion erreurs domaine |
| `MAX_STEPS` | `8` | Plus court que `BaseReActAgent` (15) — scope délimité |
| `TEMPERATURE` | `0.1` | Déterministe — le Worker exécute, ne raisonne pas |

**Convention `SYSTEM_PROMPT` pour un Worker Agent :**
```python
SYSTEM_PROMPT = """
Tu es {Nom}, un agent expert de {domaine}.

## RÈGLES ABSOLUES (non-négociables)
1. [Guardrail 1] — RAISON : ...
2. [Guardrail 2] — RAISON : ...

## IMPORTS ET PATTERNS CORRECTS
```python
import openpyxl                  # toujours utiliser openpyxl
wb = openpyxl.load_workbook(path)
```

## GESTION DES ERREURS DOMAINE
- FileNotFoundError → domain_error("file_not_found", ...)
- KeyError           → domain_error("sheet_not_found", ...)

## FORMAT DE RÉPONSE
- Toujours indiquer ce qui a été fait et le résultat.
"""
```

#### Helpers fournis

`WorkerAgent` expose des méthodes utilitaires sur `self` :

**Exécution Python :**

```python
# Exécuter du code Python (python_executor)
result = await self.run_python(ctx, code="import json; print(json.dumps({'x': 1}))")
# result : {"stdout": '{"x": 1}\n', "stderr": "", "exit_code": 0, "duration_ms": 42}

# Valider le résultat et extraire stdout (ou retourner AIPResult.failed())
output = self.check_python_result(result, operation="excel_parse")
if isinstance(output, dict):
    return output  # AIPResult.failed avec "python_execution_failed"
# output : str — contenu de stdout
```

**Opérations fichier :**

```python
# Lire un fichier → str
content = await self.read_file(ctx, path="data/rapport.xlsx")

# Écrire un fichier (crée les répertoires si nécessaire)
await self.write_file(ctx, path="output/resultat.json", content=json.dumps(data))

# Lister un répertoire → list[str]
files = await self.list_files(ctx, path="data/", recursive=True)
```

**Délégation A2A :**

```python
# Déléguer à un autre Worker Agent par skill ID
result = await self.delegate_skill(ctx, skill_id="generate-pdf", payload={"data": data})
# Raccourci pour : await ctx.delegate(skill_id, payload, timeout_secs)
```

**Erreurs domaine :**

```python
# Retourner une erreur typée (codes standardisés)
return self.domain_error("file_not_found", "Le fichier rapport.xlsx est introuvable",
                          details={"path": path})
```

Codes d'erreur standardisés : `file_not_found`, `corrupted_file`, `parse_error`, `sheet_not_found`, `column_not_found`, `encoding_error`, `python_execution_failed`, `permission_denied`.

#### Différences avec les autres classes de base

| | `BaseReActAgent` | `WorkerAgent` | `OrchestratedAgent` |
|---|---|---|---|
| Boucle ReAct | oui | oui (héritée) | non (ORIA gère) |
| Helpers tools | non | oui | non |
| Helpers erreurs domaine | non | oui | non |
| Usage typique | Agent générique | Expert domaine métier | Agent piloté par ORIA |
| `supports_a2a` | optionnel | recommandé (`True`) | optionnel |
| `MAX_STEPS` recommandé | 15 | 8 | — |
| `TEMPERATURE` recommandée | 0.3 | 0.1 | — |

#### Scaffolding Worker Agent

```bash
$ apollia-os agent new excel-agent --type worker
# génère : excel_agent_agent.py + test_excel_agent_agent.py
```

> **Voir aussi :** [Worker Agent Pattern](./Worker-Agent-Pattern) — guide complet concept, anatomie, bonnes pratiques, publishing.

---

## 2. Types

### `AIPResult`

Dataclass pour les résultats d'exécution avec méthodes factory :

```python
from apollia import AIPResult

# Succès
result = AIPResult.completed("Devis généré : 5100€ TTC", data={"amount": 5100})

# Échec
result = AIPResult.failed("VALIDATION_ERROR", "Le montant doit être positif")

# Demande HITL
result = AIPResult.input_required(
    "Confirmer l'envoi du devis ?",
    context={"email": "dupont@sa.fr", "amount": 5100}
)

# Sérialisation
dict_result = result.to_dict()
```

---

## 3. Utilitaires

### 3.1 Parsing (`apollia.utils`)

```python
from apollia.utils import extract_json, extract_code_block, extract_xml_tag, truncate, safe_json_loads

# Extraire du JSON depuis une réponse LLM
data = extract_json('Voici le résultat: ```json\n{"key": "value"}\n```')
# → {"key": "value"}

# Extraire un bloc de code
code = extract_code_block("```python\nprint('hello')\n```", language="python")
# → "print('hello')"

# Extraire un tag XML
content = extract_xml_tag("<analysis>Important finding</analysis>", "analysis")
# → "Important finding"

# Tronquer un texte (UTF-8 safe)
short = truncate("Texte très long...", max_chars=20, marker="…")

# JSON safe (jamais d'exception)
data = safe_json_loads('{"valid": true}', default={})
```

### 3.2 Formatting (`apollia.utils`)

```python
from apollia.utils import format_as_text, format_as_markdown, format_as_json, aip_result_text

# Dict → texte lisible
text = format_as_text({"name": "Dupont", "amount": 5100})
# → "name: Dupont\namount: 5100"

# Dict → tableau Markdown
md = format_as_markdown({"name": "Dupont", "amount": 5100})
# → | Key | Value |\n|---|---|\n| name | Dupont |\n| amount | 5100 |

# JSON sérialisé (jamais d'exception)
json_str = format_as_json(data, indent=2)

# Extraire le texte d'un AIPResult dict
text = aip_result_text(result_dict)
```

### 3.3 HITL (`apollia.utils`)

```python
from apollia.utils import resume_pending_tool

async def run(self, task, ctx):
    pending = resume_pending_tool(task)
    result = await self.react(task, ctx, user_msg, pending_tool=pending)
    # ...
```

### 3.4 Tool Schemas (`apollia.tools`)

```python
from apollia.tools import NATIVE_TOOL_SCHEMAS, describe_tool, build_tools_block

# Schémas des outils natifs
schemas = NATIVE_TOOL_SCHEMAS  # dict: bash_executor, file_io, python_executor

# Description compacte d'un outil
desc = describe_tool("bash_executor")

# Bloc outils pour system prompt
block = build_tools_block(["bash_executor", "file_io"])
```

---

## 4. Testing

### 4.1 Mocks

Le SDK fournit des mocks complets pour tester les agents sans runtime :

```python
from apollia.testing import MockContext, MockToolProxy, MockLlmProxy, MockMemory

# Créer un contexte mock complet
ctx = MockContext.create(
    tools={
        "bash_executor": {"stdout": "hello world", "stderr": "", "exit_code": 0},
        "file_io": {"content": "file content", "success": True},
    },
    llm_responses=[
        {"content": '{"thought": "Analyse", "action": "final_answer", "text": "Résultat"}'},
    ],
    memory=True,
)

# Exécuter l'agent
agent = MonAgent()
result = await agent.run(task, ctx)

# Inspecter les appels
assert ctx.tools.tool_call_count() == 2
ctx.tools.assert_called("bash_executor")
ctx.tools.assert_called_with("file_io", {"action": "read", "path": "/tmp/data.txt"})
assert ctx.llm.call_count == 1
```

### 4.2 Assertions

```python
from apollia.testing import (
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_tool_called,
    assert_llm_called,
)

# Vérifier le résultat
assert_result_completed(result, contains="Résultat")
assert_result_failed(result, code="VALIDATION_ERROR")
assert_result_input_required(result)

# Vérifier les appels contextuels
assert_tool_called(ctx, "bash_executor", times=2)
assert_llm_called(ctx, times=1)
```

### 4.3 Exemple de test complet

```python
import pytest
from apollia.testing import MockContext, assert_result_completed, assert_tool_called

@pytest.mark.asyncio
async def test_mon_agent_analyse():
    ctx = MockContext.create(
        tools={"bash_executor": {"stdout": "Python 3.12", "exit_code": 0}},
        llm_responses=[
            {"content": '{"thought":"check version","action":"tool_call","tool":"bash_executor","args":{"command":"python3 --version"}}'},
            {"content": '{"thought":"done","action":"final_answer","text":"Python 3.12 détecté"}'},
        ],
        memory=True,
    )

    task = {
        "task_id": "t-test-001",
        "input": {"parts": [{"type": "text", "text": "Quelle version de Python ?"}]},
    }

    agent = MonAgent()
    result = await agent.run(task, ctx)

    assert_result_completed(result, contains="Python 3.12")
    assert_tool_called(ctx, "bash_executor", times=1)
```

---

## 5. Scaffolding

### 5.1 Via le SDK Python

```bash
# Via le CLI runtime (recommandé — détecte le SDK automatiquement)
$ apollia-os agent new mon-agent
$ apollia-os agent new assistant --type conversational
$ apollia-os agent new analyseur --type orchestrated
$ apollia-os agent new excel-agent --type worker

# Ou directement via le SDK Python
$ python -m apollia new mon-agent
$ python -m apollia new mon-agent --output-dir ./agents/
```

Fichiers générés : `<module_name>_agent.py` + `test_<module_name>_agent.py`

### 5.2 Via la CLI Apollia OS

```bash
# Intégration directe dans la CLI Rust
$ apollia-os agent new mon-agent --type react
  ✔ SDK disponible (apollia 0.1.0)
  ✔ Nom disponible
  → Création de l'agent dans ~/.apollia/agents/mon-agent/
  ✔ Agent créé :
    - mon_agent_agent.py
    - test_mon_agent_agent.py
```

### 5.3 Via l'application Desktop

Le dialog "Create from Template" dans la vue Agents permet de créer un agent visuellement :

1. Cliquer sur le bouton "Create from Template"
2. Sélectionner un template (ReAct, Conversational, Orchestrated)
3. Saisir le nom de l'agent (kebab-case, validation temps réel)
4. Cliquer sur "Create"

Le dialog vérifie automatiquement la disponibilité du SDK et le conflit de noms.

---

## 6. Type Stubs

Le SDK inclut des type stubs PEP 561 pour les classes injectées par le runtime PyO3 :

```python
# sdk/apollia/stubs/context.pyi
class RuntimeContext:
    @property
    def tools(self) -> ToolProxy | None: ...
    @property
    def llm(self) -> LlmProxy | None: ...
    @property
    def memory(self) -> MemoryInterface | None: ...

# sdk/apollia/stubs/tools.pyi
class ToolProxy:
    async def call(self, tool_name: str, input: dict[str, object]) -> dict[str, object]: ...
    def list_tools(self) -> list[str]: ...
    def tool_call_count(self) -> int: ...
    async def describe(self, name: str) -> dict[str, object] | None: ...

# sdk/apollia/stubs/llm.pyi
class LlmProxy:
    async def complete(self, messages: list[dict[str, object]] | str, **kwargs) -> dict[str, object]: ...
    async def chat(self, system: str, user: str, backend: str | None = None) -> dict[str, object]: ...
    @property
    def default_backend(self) -> str: ...

# sdk/apollia/stubs/memory.pyi
class MemoryInterface:
    async def record(self, content: str, importance: float | None = None, ...) -> None: ...
    async def remember(self, key: str, value: str, source: str | None = None) -> None: ...
    async def recall(self, key: str) -> str | None: ...
    async def search(self, query: str, limit: int | None = None) -> list[dict[str, object]]: ...
    async def forget(self, key: str) -> None: ...
```

Avec le marker `py.typed`, les IDE (VSCode, PyCharm) et `mypy` résolvent automatiquement les types.

---

## 7. Migration depuis `apollia_base.py`

Si vous utilisez l'ancien `apollia_base.py`, la migration est simple :

```python
# Avant (sans SDK)
class MonAgent:
    def manifest(self): ...
    async def run(self, task, ctx): ...
agent = MonAgent()

# Après (avec SDK)
from apollia.agents import BaseReActAgent, AIPResult

class MonAgent(BaseReActAgent):
    def manifest(self): ...
    async def run(self, task, ctx):
        result = await self.react(task, ctx, user_message)
        return AIPResult.completed(result).to_dict()
agent = MonAgent()
```

L'ancien fichier `apollia_base.py` reste un wrapper de compatibilité et continue de fonctionner.

---

## Voir aussi

- [Agents Quickstart](./Agents-Quickstart) — premier agent en 5 minutes
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — référence complète `ctx.*`
- [Briques AIP Specification](./Briques-AIP-Specification) — contrat AIP complet
- [Worker Agent Pattern](./Worker-Agent-Pattern) — guide complet pour créer un Worker Agent de A à Z
- [ADR-037](../adr/ADR-037-python-sdk-packaging) — décision packaging SDK
