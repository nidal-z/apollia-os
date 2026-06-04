---
title: Agents SDK - Python API Reference
description: Pure API reference for apollia-sdk classes, methods, and utilities. For tutorials, see the book.
weight: 50
---

# Agents SDK - Python API Reference

**Référence pure des signatures, paramètres, retours et exceptions du SDK Python Apollia.**

> ⚠️ **Ce document est une référence, pas un tutoriel.** Pour apprendre comment utiliser le SDK, voir [book ch03–ch04](../../book/src/ch03-intro-aip-et-manifest.md). Chaque section liste les méthodes publiques d'une classe ou module ; aucun exemple long. Les cas d'usage vont dans le book.

---

## Installation

```bash
$ pip install -e ./sdk
$ python3 -c "import apollia; print(apollia.__version__)"
0.3.0
```

Requiert Python 3.10+. Zéro dépendance runtime (ADR-023).

---

## Module structure

```
sdk/apollia/
├── agents/           BaseReActAgent, ConversationalAgent, OrchestratedAgent, WorkerAgent
├── types.py          AIPResult (dataclass)
├── bootstrap.py      ContextBootstrap (abstract class) ⚠️ non livré encore
├── utils/            parsing, formatting, HITL helpers
├── tools/            build_tools_block_from_ctx(), NATIVE_TOOL_SCHEMAS (legacy fallback)
├── testing/          mocks, assertions
├── stubs/            RuntimeContext, ToolProxy, LlmProxy, MemoryInterface (PEP 561)
└── py.typed          PEP 561 marker
```

---

## 1. Classes de base

### 1.1 BaseReActAgent

Implémente la boucle Reason-Act-Observe avec LLM et outils.

**Constantes de classe** (`class MyAgent(BaseReActAgent)`) :

| Constante | Type | Défaut | Description |
|---|---|---|---|
| `SYSTEM_PROMPT` | `str` | `"You are a helpful assistant."` | Prompt système pour le LLM |
| `MAX_STEPS` | `int` | `30` | Iterations max avant timeout |
| `TEMPERATURE` | `float` | `0.3` | Température LLM (0–2) |

**Méthodes abstraites** (à implémenter) :

| Méthode | Signature | Retour | Description |
|---|---|---|---|
| `manifest()` | ` -> dict[str, Any]` | Agent metadata dict | Renvoie nom, version, outils requis, mode execution, etc. |
| `run(task, ctx)` | `async (dict, RuntimeContext) -> dict[str, Any]` | `AIPResult` serialized | Point d'entrée - appelé une fois par tâche |

**Méthodes publiques** :

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| `react` | `async (task, ctx, user_message, *, extra_context="", pending_tool=None, history=None) -> str \| dict` | `task`: AIP task dict; `ctx`: RuntimeContext; `user_message`: str; `extra_context`: contexte additionnel (str); `pending_tool`: HITL resume (dict \| None); `history`: previous turns (list[dict] \| None) | `str` (final answer) OR `dict` (AIPResult.input_required/failed) | (aucune - dégradation gracieuse) | Cœur de la boucle ReAct. Si `ctx.llm is None` retourne `AIPResult.failed("NO_LLM",...)`. |
| `get_tool_schemas` | ` -> list[dict[str, Any]]` | - | Schémas d'outils natifs | - | Retourne les 13 outils natifs (bash_executor, file_io, python_executor, ask_user, notebook_read, notebook_edit, etc.) |

**Observabilité automatique (ADR-019, Lot 2).** `react()` instrumente le
loop pour pousser sur la trace event-sourced (visible dans
`ExecutionTrace`) :
- `ctx.emit_thought(thought, step_num)` après chaque parsing JSON
  d'action - rend la pensée du LLM visible.
- `ctx.emit_action_parse_error(step_num, raw, repair_attempted=True)`
  quand le JSON action est invalide.
- `ctx.emit_retry(step_num, "action_parse_error", attempt)` avant
  chaque tentative de réparation.

Toutes ces émissions passent par un helper `_emit_safe(ctx, method,
*args)` qui ignore silencieusement l'absence de méthode (`MockContext`
en test) ou toute exception - la télémétrie ne casse jamais le loop.

---

### 1.2 ConversationalAgent

Agent dialogue uniquement, sans outils. Hérite de `ABC`.

**Constantes de classe** :

| Constante | Type | Défaut | Description |
|---|---|---|---|
| `SYSTEM_PROMPT` | `str` | `""` | Prompt système pour le LLM |
| `MAX_TURNS` | `int` | `20` | Turns max par conversation |
| `TEMPERATURE` | `float` | `0.7` | Température LLM |

**Méthodes abstraites** :

| Méthode | Signature | Retour | Description |
|---|---|---|---|
| `manifest()` | ` -> dict[str, Any]` | Manifest dict (requiert `tools_required: []`) | Métadonnées agent |

**Méthodes publiques** :

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| `converse` | `async (ctx, user_message, history=None) -> tuple[str, list[dict]]` | `ctx`: RuntimeContext; `user_message`: str; `history`: previous turns (list[dict] \| None) | `(response_text, updated_history)` | `RuntimeError` si `ctx.llm is None` | Persiste dans `ctx.memory` (importance=0.3) si disponible |
| `run()` | `async (task, ctx) -> AIPResult` | `task`: AIP task; `ctx`: RuntimeContext | `AIPResult.completed` | `RuntimeError` si `ctx.llm is None` | Extrait `task["input"]["parts"][0]["text"]` et appelle `converse` |
| `on_response` | `(response: str) -> str` | `response`: LLM text (overridable) | Texte post-traité | - | Post-processing optionnel (défaut : pas de modification) |

---

### 1.3 OrchestratedAgent

Agent piloté par ORIA (mode orchestré). Hérite de `ABC`.

**Méthodes abstraites** :

| Méthode | Signature | Retour | Description |
|---|---|---|---|
| `manifest()` | ` -> dict[str, Any]` | Manifest (requiert `execution_mode: "orchestrated"` + `system_prompt`) | Métadonnées |

**Méthodes publiques** :

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| `run()` | `async (task, ctx) -> AIPResult` | `task`: AIP task; `ctx`: RuntimeContext | - | **`RuntimeError`** (toujours) | ORIA gère l'exécution - `run()` ne doit pas être appelée |
| `on_plan_complete` | `(self, step_results: dict[str, Any], ctx) -> dict[str, Any]` | `step_results`: `{step_id: result_dict}`; `ctx`: RuntimeContext (overridable) | `{"text": "...",...}` | - | Post-traitement après plan ORIA (défaut : concatène les textes) |
| `format_step_results` | `(results: dict[str, Any]) -> str` | `results`: step results dict | Texte formaté multi-ligne | - | Helper statique pour formatter les résultats |

---

### 1.4 WorkerAgent

Agent spécialisé dans un domaine métier. **Hérite de `BaseReActAgent`** - même boucle ReAct, mêmes constantes.

**Helpers fournis** (méthodes d'instance) :

| Méthode | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `run_python` | `async (ctx, code, timeout_secs=30) -> dict[str, Any]` | `code`: str Python; `timeout_secs`: int | `{"stdout", "stderr", "exit_code", "duration_ms"}` | Exécute Python via `python_executor` |
| `check_python_result` | `(result: dict, operation: str) -> str \| dict` | `result`: output de `run_python`; `operation`: str de log | Stdout (`str`) ou `AIPResult.failed` dict | Vérifie `exit_code == 0` |
| `read_file` | `async (ctx, path: str) -> str` | `path`: chemin fichier | Contenu fichier | Via `file_read` tool |
| `write_file` | `async (ctx, path: str, content: str) -> None` | `path`: chemin; `content`: str | - | Via `file_write` tool ; crée répertoires |
| `list_files` | `async (ctx, path: str, recursive: bool=False) -> list[str]` | `path`: répertoire; `recursive`: bool | Chemins relatifs (list[str]) | Via `file_list` tool |
| `delegate_skill` | `async (ctx, skill_id: str, payload: dict, timeout_secs=120) -> dict[str, Any]` | `skill_id`: str; `payload`: dict; `timeout_secs`: int | Résultat A2A (dict) | Via `ctx.delegate` ; lève `RuntimeError` si skill absent |
| `domain_error` | `(code: str, message: str, details=None) -> dict[str, Any]` | `code`: stable snake_case (ex: `file_not_found`); `message`: str; `details`: dict \| None | `AIPResult.failed` dict | Codes: `file_not_found`, `corrupted_file`, `parse_error`, `sheet_not_found`, `column_not_found`, `encoding_error`, `python_execution_failed`, `permission_denied` |

**Constantes recommandées** :

| Constante | Valeur recommandée | Raison |
|---|---|---|
| `MAX_STEPS` | `8` | Plus court que BaseReActAgent (15) - scope délimité |
| `TEMPERATURE` | `0.1` | Déterministe - le Worker exécute, ne raisonne pas |

---

## 2. Types

### AIPResult (dataclass)

Résultat retourné par `run()` pour le runtime.

**Champs** :

| Champ | Type | Optional | Description |
|---|---|---|---|
| `status` | `str` | ✓ | `"completed"` \| `"failed"` \| `"input_required"` |
| `text` | `str \| None` | ✓ | Texte de réponse (completed) |
| `error_code` | `str \| None` | ✓ | Code erreur (failed) |
| `error_message` | `str \| None` | ✓ | Message erreur (failed) |
| `input_prompt` | `str \| None` | ✓ | Demande HITL (input_required) |
| `input_context` | `dict[str, Any] \| None` | ✓ | Contexte HITL (input_required) |
| `data` | `dict[str, Any]` | - | Données additionnelles (défaut: `{}`) |

**Méthodes factory** :

| Méthode | Signature | Retour | Notes |
|---|---|---|---|
| `completed` | `(text: str, data=None) -> AIPResult` | Status=`"completed"` | Succès avec texte optionnel + données |
| `failed` | `(code: str, message: str) -> AIPResult` | Status=`"failed"` | Erreur typée |
| `input_required` | `(prompt: str, context=None) -> AIPResult` | Status=`"input_required"` | Suspension HITL avec contexte optionnel |
| `to_dict` | ` -> dict[str, Any]` | Dict sérialisé | Pour runtime (omit fields=None) |

---

## 3. Parsing (`apollia.utils.parsing`)

| Fonction | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `extract_json` | `(content: str) -> dict[str, Any]` | `content`: str potentiellement avec JSON | Dict extrait ou `{}` | 4 stratégies : full JSON, fence, outermost braces, heuristic repair |
| `extract_code_block` | `(content: str, language: str = "") -> str` | `content`: str; `language`: Python, bash, etc. | Code extrait ou `""` | Extrait depuis fences `` ``` `` |
| `extract_xml_tag` | `(content: str, tag: str) -> str` | `content`: str XML; `tag`: nom tag | Contenu tag ou `""` | Extrait `<tag>...</tag>` |
| `truncate` | `(text: str, max_chars: int, marker: str = "…") -> str` | `text`: str; `max_chars`: int; `marker`: str suffix | Texte tronqué UTF-8 safe | Jamais de levée |
| `safe_json_loads` | `(content: str, default: Any = None) -> Any` | `content`: str JSON; `default`: valeur fallback | JSON désérialisé ou `default` | Jamais d'exception |
| `validate_action` | `(data: dict) -> dict` | `data`: extracted JSON (ReAct action) | Action dict validée | Lève `ActionParseError` si structure invalide |

---

## 4. Formatting (`apollia.utils.formatting`)

| Fonction | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `format_as_text` | `(data: Any) -> str` | `data`: Any type | Texte lisible | dict → `key: value` par ligne; list → une ligne/element |
| `format_as_markdown` | `(data: Any) -> str` | `data`: Any type | Markdown | dict → table 2-colonnes; list[dict] → table multi-colonnes |
| `format_as_json` | `(data: Any, indent: int = 2) -> str` | `data`: Any; `indent`: int spaces | JSON indented | Non-serializable types → `str` ; jamais d'exception |
| `aip_result_text` | `(result_dict: dict) -> str` | `result_dict`: AIPResult dict | Texte extrait | Extrait le texte principal de `result["output"]` ou `result["error"]` |

---

## 5. HITL (`apollia.utils`)

| Fonction | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `resume_pending_tool` | `(task: dict) -> dict \| None` | `task`: AIP task dict | `{"tool": str, "args": dict}` or `None` | À utiliser avec `react(..., pending_tool=...)` sur HITL resume |

---

## 6. Tool Schemas (`apollia.tools`)

Le **runtime tool registry** (Rust) est la source unique de vérité pour les
descripteurs d'outils : nom, description, JSON Schema des paramètres, schéma
de sortie, tags, profil sandbox, niveau de risque. Ces descripteurs sont
exposés à Python via la coroutine `ctx.tools.describe(name)` (binding PyO3).

`BaseReActAgent.react()` construit son bloc d'outils dans le system prompt
en interrogeant le registry pour chaque outil autorisé - pas de duplication
côté SDK, pas de drift possible entre ce que le LLM voit et ce que le
runtime enforce au dispatch.

### API préférée (runtime-driven)

| Fonction | Signature | Retour | Notes |
|---|---|---|---|
| `build_tools_block_from_ctx` | `async (ctx, tool_names: list[str]) -> str` | Bloc `Available tools:` formaté | Appelle `ctx.tools.describe()` pour chaque nom. Dégrade silencieusement vers le fallback offline si `ctx.tools` est `None` ou si `describe()` échoue. |
| `render_descriptor` | `(name: str, descriptor: dict \| None) -> str` | Bloc d'un outil | Formate un descripteur (dict retourné par `ctx.tools.describe()`) en bloc prompt-friendly avec parameters et example. Si `descriptor=None`, retombe sur le renderer legacy. |

### API legacy (fallback offline)

À utiliser uniquement en tests, dry-runs ou contextes sans runtime. Best-effort,
non autoritaire : la validation des paramètres se fait toujours contre le
descripteur Rust au dispatch.

| Objet / Fonction | Description |
|---|---|
| `NATIVE_TOOL_SCHEMAS: dict[str, dict]` | Miroir statique des descripteurs Rust (15 outils natifs : `bash_executor`, `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`, `ask_user`, `memory_search`, `http_fetch`, `python_executor`, `notebook_read`, `notebook_edit`, `web_search`, `web_read`). |
| `describe_tool(name) -> str` | Renderer synchrone basé sur `NATIVE_TOOL_SCHEMAS`. |
| `build_tools_block(tool_names) -> str` | Builder synchrone basé sur `NATIVE_TOOL_SCHEMAS`. |

---

## 7. Testing (`apollia.testing`)

### Mocks

| Classe | Constructeur | Description |
|---|---|---|
| `MockContext` | `create(tools={}, llm_responses=[], memory=False)` | Contexte mock complet ; retourne objet avec `.tools`, `.llm`, `.memory` |
| `MockToolProxy` | (crée via `MockContext`) | Proxy outils mock ; méthodes : `call()`, `list_tools`, `tool_call_count`, `assert_called(name)`, `assert_called_with(name, args)` |
| `MockLlmProxy` | (crée via `MockContext`) | Proxy LLM mock ; méthodes : `complete()`, `chat()`, `call_count` |
| `MockMemory` | (crée via `MockContext`) | Mémoire mock ; méthodes : `record()`, `recall()`, `remember`, `search()`, `forget` |

### Assertions

| Fonction | Signature | Notes |
|---|---|---|
| `assert_result_completed` | `(result: dict, contains: str = "") -> None` | Lève AssertionError si status ≠ `"completed"` ou contenu absent |
| `assert_result_failed` | `(result: dict, code: str = "") -> None` | Lève si status ≠ `"failed"` ou code absent |
| `assert_result_input_required` | `(result: dict) -> None` | Lève si status ≠ `"input_required"` |
| `assert_tool_called` | `(ctx: MockContext, name: str, times: int = 1) -> None` | Lève si tool non appelé `times` fois |
| `assert_llm_called` | `(ctx: MockContext, times: int = 1) -> None` | Lève si LLM non appelé `times` fois |

---

## 8. ContextBootstrap (`apollia.bootstrap`)

> ⚠️ **Module non livré** - `sdk/apollia/bootstrap.py` n'existe pas dans la version actuelle du SDK. Cette section documente l'interface prévue ; le module sera ajouté dans un sprint futur.

Classe abstraite pour que les agents explorent et persistent un contexte cross-session.

**Méthodes abstraites** (à implémenter) :

| Méthode | Signature | Paramètres | Retour | Description |
|---|---|---|---|---|
| `is_stale()` | `async (ctx) -> bool` | `ctx`: RuntimeContext | `bool` | Snapshot existant est-il périmé ? |
| `run_bootstrap()` | `async (ctx) -> dict` | `ctx`: RuntimeContext | Snapshot dict | Explore domaine, construit snapshot, persiste |

**Méthodes héritées** (rarement surchargées) :

| Méthode | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `needs_bootstrap()` | `async (ctx) -> bool` | `ctx`: RuntimeContext | `bool` | Vérifie status + staleness |
| `load_snapshot()` | `async (ctx) -> dict \| None` | `ctx`: RuntimeContext | Snapshot dict or None | Charge depuis mémoire sémantique |
| `load_meta()` | `async (ctx) -> dict \| None` | `ctx`: RuntimeContext | Métadonnées dict or None | Charge `{version, created_at, staleness_marker}` |
| `persist()` | `async (ctx, snapshot, *, staleness_marker,...)` | `ctx`: RuntimeContext; `snapshot`: dict; `staleness_marker`: str | - | Écrit snapshot + meta + status en mémoire |

**Clés mémoire convention** :

- `bootstrap.snapshot` - snapshot JSON complet
- `bootstrap.meta` - `{version, created_at, staleness_marker}`
- `bootstrap.status` - `"complete"` \| `"partial"` \| `"missing"`

---

## 9. Agent Manifest (`apollia.stubs.manifest`)

`AgentManifestDict` - TypedDict pour typage static manifest.

**Champs clés** :

| Champ | Type | Required | Description |
|---|---|---|---|
| `name` | `str` | ✓ | Identifiant unique kebab-case |
| `version` | `str` | ✓ | Semver (ex: `"1.0.0"`) |
| `description` | `str` | ✓ | Texte une ligne |
| `execution_mode` | `str` | ✓ | `"direct"` (ReAct) ou `"orchestrated"` (ORIA) |
| `tools_required` | `list[str]` | - | Outils natifs utilisés (ex: `["bash_executor", "file_io"]`) |
| `tools_requiring_approval` | `list[str]` | - | Outils qui triggent HITL (subset de `tools_required`) |
| `agent_type` | `str` | - | `"worker"` \| `"assistant"` \| `"system"` |
| `examples` | `list[str]` | - | Cas d'usage exemple (UI + doc) |
| `limitations` | `list[str]` | - | Contraintes connues |
| `setup_notes` | `str` | - | Notes configuration |
| `packages` | `list[str]` | - | Dépendances pip (ex: `["openpyxl>=3.1.0"]`) |
| `agent_class` | `str \| None` | - | **Ne pas déclarer.** Renseigné automatiquement par le runtime depuis `agent.__class__.__name__`. Utilisé par l'UI pour afficher un badge de type. |
| `supports_a2a` | `bool` | - | Accessible via A2A routing (défaut: false) |
| `skills` | `list[dict]` | - | Skills publiés pour delegation A2A |
| `user_memory_write` | `bool` | - | Autorise `ctx.memory.remember_user()` - écriture dans `__user__`. Réservé aux agents système (ex. `onboarding-agent`). Défaut: `false`. |

---

## 10. RuntimeContext (`apollia.stubs.context`)

Injecté par le runtime Rust ; type stub PEP 561.

**Propriétés** :

| Propriété | Type | Nullable | Description |
|---|---|---|---|
| `llm` | `LlmProxy` | ✓ | Proxy pour LLM backend (Claude, etc.) |
| `tools` | `ToolProxy` | ✓ | Proxy pour outils natifs (bash, file_io, etc.) |
| `memory` | `MemoryInterface` | ✓ | Mémoire sémantique persistante |
| `step_budget` | `StepBudgetView` | ✓ | Budget d'exécution restant (lecture seule) |
| `workspace` | `WorkspaceContext` | ✓ | Contexte workspace collecté au démarrage |
| `user_context` | `dict[str, list[tuple[str,str]]]` | ✓ | Contexte utilisateur injecté en mode chat |
| `delegate()` | callable | - | Fonction A2A pour déléguer à autres agents |

**Méthodes** :

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| `log()` | `(level: str, message: str) -> None` | `level`: `"debug"\|"info"\|"warn"\|"error"`; `message`: str | - | `ValueError` si niveau invalide | Émet via `tracing::` du runtime (traces structurées) |
| `emit_token()` | `(token: str) -> None` | `token`: str | - | - | Streaming SSE en mode chat ; no-op en mode task |
| `delegate()` | `async (skill_id: str, payload: dict, timeout_secs: int = 120) -> dict[str, Any]` | `skill_id`: str; `payload`: dict; `timeout_secs`: int | Résultat A2A (dict) | `RuntimeError` si skill absent | - |
| `send()` | `async (agent_name: str, message: dict) -> None` | `agent_name`: str; `message`: dict JSON | - | `RuntimeError` si `supports_a2a` false | Messagerie inter-agents |
| `receive()` | `async (timeout_seconds: float \| None = None) -> dict \| None` | `timeout_seconds`: délai max | Message dict ou `None` | `RuntimeError` si `supports_a2a` false | - |

**`StepBudgetView`** - retourné par `ctx.step_budget` :

| Propriété | Type | Description |
|---|---|---|
| `steps_remaining` | `int` | Steps restants avant limite (0 si épuisé) |
| `tool_calls_remaining` | `int` | Appels outils restants (`-1` = non tracké dans cette vue) |
| `elapsed_seconds` | `float` | Secondes écoulées (`0.0` = non tracké dans cette vue) |

---

## 11. LlmProxy (`apollia.stubs.llm`)

Stub pour backend LLM.

**Propriétés** :

| Propriété | Type | Description |
|---|---|---|
| `default_backend` | `str` | Backend actif (ex: `"claude-opus-4"`) |

**Méthodes** :

| Méthode | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `complete()` | `async (messages: list[dict] \| str, **kwargs) -> dict[str, object]` | `messages`: list ou str; `kwargs`: backend options | Response dict avec champ `"content"` (ou `"text"`) | Jamais de levée ; dégradation gracieuse si backend absent |
| `chat()` | `async (system: str, user: str, backend: str \| None = None) -> dict[str, object]` | `system`: str; `user`: str; `backend`: optional override | Response dict | Wrapper `complete()` |
| `stream_complete()` | `async (messages: list[dict]) -> AsyncIterator[str]` | `messages`: list | Async iterator de chunks str | Optional (certains backends ne supportent pas) |

---

## 12. ToolProxy (`apollia.stubs.tools`)

Stub pour exécution outils.

**Méthodes** :

| Méthode | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| `call()` | `async (tool_name: str, input: dict[str, object]) -> dict[str, object]` | `tool_name`: str; `input`: args dict | Résultat tool (dict) | Jamais de levée ; dégradation gracieuse si tool absent |
| `list_tools` | ` -> list[str]` | - | Noms outils disponibles | Immuable par session |
| `tool_call_count` | ` -> int` | - | Nombre d'appels cumulés | Test helper (mock seulement) |
| `describe()` | `async (name: str) -> dict[str, object] \| None` | `name`: str outil | Spec dict ou None | Retourne schéma outil |

---

## 13. MemoryInterface (`apollia.stubs.memory`)

Stub pour mémoire persistante.

**Méthodes** :

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| `record()` | `async (content: str, importance: float \| None = 0.5, task_id: str \| None = None, metadata: dict \| None = None, expires_in: int \| None = None) -> None` | `content`: texte; `importance`: 0–1; `task_id`: tâche courante; `metadata`: dict arbitraire; `expires_in`: TTL en secondes | - | `RuntimeError` si espace namespace épuisé | Persiste en mémoire épisodique |
| `remember()` | `async (key: str, value: str, source: str \| None = None, confidence: float \| None = None) -> None` | `key`: str; `value`: str; `source`: optional; `confidence`: 0–1 | - | - | Mémoire sémantique clé/valeur dans le namespace propre de l'agent |
| `remember_user()` | `async (key: str, value: str, source: str \| None = None, confidence: float \| None = None) -> None` | `key`: str; `value`: str | - | `RuntimeError` si `user_memory_write ≠ true` | Écriture dans `__user__` (namespace global). Réservé aux agents avec `user_memory_write = true` dans le manifest. |
| `recall()` | `async (key: str) -> str \| None` | `key`: str | Valeur ou `None` | - | Cherche dans le namespace propre puis dans `__user__` (fallback inconditionnel si `user_manager` configuré). |
| `recall_entry()` | `async (key: str, injection_reason: str \| None = None) -> dict \| None` | `key`: str | `{key, value, confidence, source, updated_at, expires_at}` ou `None` | - | Retourne entrée avec metadata complète |
| `recall_all()` | `async (limit: int \| None = 100, injection_reason: str \| None = None) -> list[dict]` | `limit`: max résultats | `list[dict]` - même structure que `recall_entry()` | - | Liste toutes les entrées du namespace |
| `recall_procedure()` | `async (trigger: str) -> list[dict]` | `trigger`: déclencheur exact | `[{id, trigger, steps, success_count, last_used_at, created_at}]` ou `[]` | - | Mémoire procédurale - workflows appris |
| `search()` | `async (query: str, limit: int \| None = None) -> list[dict[str, object]]` | `query`: str texte; `limit`: int max results | `[{content, score, source, timestamp}]` | - | Recherche full-text (FTS5) |
| `forget()` | `async (key: str) -> None` | `key`: str | - | - | Supprime entrée sémantique |

---

## 14. CLI Scaffolding

```bash
# Via CLI Apollia OS (recommandé)
$ apollia-os agent new mon-agent --type react
$ apollia-os agent new mon-assistant --type conversational
$ apollia-os agent new mon-orateur --type orchestrated
$ apollia-os agent new mon-worker --type worker

# Ou via SDK Python
$ python -m apollia new mon-agent
$ python -m apollia new mon-agent --output-dir ./agents/

# Via Desktop app
# → Menu "Create from Template" (valide nom, détecte SDK, crée fichier + test)
```

Génère : `<snake_name>_agent.py` + `test_<snake_name>_agent.py`

---

## Voir aussi

- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide.md) - table complète des services injectés
- [Briques AIP Specification](./Briques-AIP-Specification.md) - contrat AIP complet
- [Worker Agent Pattern](./Worker-Agent-Pattern.md) - spécialisation agents
- [Agents ContextBootstrap Guide](./Agents-ContextBootstrap-Guide.md) - bootstrapping cross-session
- [book ch03–ch04](../../book/src/ch03-intro-aip-et-manifest.md) - apprendre le SDK par l'exemple
- [ADR-023](../adr/ADR-023-sdk-agentkit-design.md) - décision packaging SDK
- [ADR-010](../adr/ADR-010-memory-context-architecture.md) - ContextBootstrap convention

---

*Dernière mise à jour : 2026-04-24*

