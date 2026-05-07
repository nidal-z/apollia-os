# Agents — RuntimeContext Guide

> Référence exhaustive (signatures uniquement) des services injectés dans `ctx` lors de `run(task, ctx)`.
> Public cible : développeur Python intermédiaire en consultation.

---

## Vue synthétique des services

| Service | Disponibilité | Interface | Statut |
|---|---|---|---|
| **ctx.tools** | Si `tools_required` ou `tools_optional` | [`ToolProxy`](#ctxtools--toolproxy) | ✅ Livré |
| **ctx.llm** | Si backend LLM configuré | [`LlmProxy`](#ctxllm--llmproxy) | ✅ Livré |
| **ctx.memory** | Si `memory_namespace` dans manifest | [`MemoryInterface`](#ctxmemory--memoryinterface) | ✅ Livré |
| **ctx.step_budget** | Toujours disponible | [`StepBudgetView`](#ctxstep_budget--stepbudgetview) | ✅ Livré |
| **ctx.log** | Toujours disponible | [`AgentLogger`](#ctxlog--agentlogger) | ✅ Livré |
| **ctx.workspace** | Contexte workspace collecté | [`WorkspaceContextPy`](#ctxworkspace--workspacecontextpy) | ✅ Livré |
| **ctx.user_context** | Mode chat uniquement | `dict[str, list[tuple]]` ou `None` | ✅ Livré |
| **ctx.send** | Si `supports_a2a: True` | Async, messagerie inter-agents | ✅ Livré |
| **ctx.receive** | Si `supports_a2a: True` | Async, réception messages | ✅ Livré |
| **ctx.delegate** | Si `supports_a2a: True` (Director) | Async, délégation A2A | ✅ Livré |
| **ctx.emit_token** | Mode chat uniquement | Sync, streaming tokens | ✅ Livré |
| **ctx.a2a_invoke** | Si `supports_a2a: True` | Async, invocation A2A de haut niveau | ✅ Livré |
| **ctx.a2a_discover** | Si `supports_a2a: True` | Async, découverte skill | ✅ Livré |

---

## ctx.tools – ToolProxy

Proxy de sécurité pour l'invocation d'outils. Permissifs, audit, comptabilité step budget.

### Méthodes

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| **call** | `async call(tool_name: str, input: dict) -> dict` | `tool_name` : str ; `input` : dict JSON sérialisable | dict (résultat JSON du Rust) | `RuntimeError: tool not found` ; `RuntimeError: tool not allowed` ; `RuntimeError: tool execution failed` | Tous les appels comptabilisés, audit trail SQLite fire-and-forget |
| **list_tools** | `list_tools -> list[str]` | — | liste des noms d'outils accessibles | — | Consulter avant de décider d'un appel optionnel |
| **tool_call_count** | `tool_call_count -> int` | — | nombre d'appels effectués | — | Aide à adapter le comportement proche de la limite budget |

### Outils natifs disponibles

#### bash_executor

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `command` | str | — | ✅ | Commande bash à exécuter |
| `timeout_seconds` | int | 30 | ❌ | Timeout en secondes |
| `working_dir` | str | `.` | ❌ | Répertoire de travail |
| **Retour** | dict | — | — | `{"stdout": str, "stderr": str, "exit_code": int}` |

#### file_read

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `path` | str | — | ✅ | Chemin du fichier à lire |
| `offset` | int | 1 | ❌ | Ligne de départ (1-based) |
| `limit` | int | — | ❌ | Nombre max de lignes à retourner |
| **Retour** | dict | — | — | `{"content": str, "total_lines": int, "truncated": bool}` |

#### file_write

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `path` | str | — | ✅ | Chemin du fichier (crée ou remplace) |
| `content` | str | — | ✅ | Contenu à écrire |
| **Retour** | dict | — | — | `{"bytes_written": int, "path": str}` |

#### file_edit

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `path` | str | — | ✅ | Chemin du fichier à modifier |
| `old_str` | str | — | ✅ | Chaîne exacte à remplacer (doit être unique) |
| `new_str` | str | — | ✅ | Nouvelle chaîne |
| **Retour** | dict | — | — | `{"replaced": bool, "path": str}` — échoue si absent/non-unique |

#### file_list

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `path` | str | `.` | ❌ | Répertoire à lister |
| `depth` | int | 1 | ❌ | Profondeur de récursion |
| **Retour** | dict | — | — | `{"entries": [{"name": str, "is_dir": bool, "size": int},...]}` |

#### file_glob

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `pattern` | str | — | ✅ | Pattern glob (ex. `**/*.py`) |
| `path` | str | `.` | ❌ | Répertoire de départ |
| **Retour** | dict | — | — | `{"matches": [str], "count": int}` |

#### file_grep

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `pattern` | str | — | ✅ | Expression régulière |
| `path` | str | `.` | ❌ | Répertoire de recherche |
| `glob` | str | — | ❌ | Filtre sur les fichiers (pattern glob) |
| `context_lines` | int | 0 | ❌ | Lignes de contexte avant/après |
| **Retour** | dict | — | — | `{"matches": [{"file": str, "line": int, "content": str},...], "count": int}` |

#### http_fetch

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `url` | str | — | ✅ | URL cible (domaine doit être dans `network_allowlist`) |
| `method` | str | `GET` | ❌ | Méthode HTTP (GET, POST, etc.) |
| `headers` | dict | `{}` | ❌ | En-têtes HTTP |
| `timeout_secs` | int | 15 | ❌ | Timeout en secondes |
| **Retour** | dict | — | — | `{"status": int, "body": str, "headers": dict}` |

#### memory_search

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `query` | str | — | ✅ | Texte à rechercher (FTS5 + BM25) |
| `namespace` | str | namespace propre | ❌ | Namespace cible |
| `limit` | int | 10 | ❌ | Max 50 résultats |
| `source` | str | `"episodic"` | ❌ | `"episodic"` \| `"semantic"` |
| **Retour** | dict | — | — | `{"results": [{"content": str, "score": float, "source": str},...], "count": int}` |

#### python_executor

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `code` | str | — | ✅ | Code Python à exécuter |
| `timeout_seconds` | int | 60 | ❌ | Timeout en secondes |
| **Retour** | dict | — | — | `{"stdout": str, "stderr": str, "exit_code": int}` |

---

## ctx.llm – LlmProxy

Proxy vers les backends LLM configurés. Wrapper autour de `LlmRouter`.

### Propriété

| Propriété | Type | Notes |
|---|---|---|
| **default_backend** | str | Getter : nom du backend par défaut (ex. `"anthropic"`, `"local"`) |

### Méthodes

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| **chat** | `async chat(system: str, user: str, backend: str=None) -> LlmResponse` | `system` : str (system prompt) ; `user` : str (user message) ; `backend` : str optionnel | [`LlmResponse`](#llmresponse) | `RuntimeError` si LLM None | Cas d'usage 80% : appel simple |
| **complete** | `async complete(messages: list[dict], backend: str=None) -> LlmResponse` | `messages` : `list[{"role": str, "content": str}]` ; `backend` : str optionnel | [`LlmResponse`](#llmresponse) | `PyValueError` si message invalide | Multi-tour explicite : system/user/assistant |
| **stream** | `async stream(messages: list[dict], backend: str=None) -> list[str]` | `messages` : `list[dict]` ; `backend` : str optionnel | `list[str]` (chunks collectés) | `PyRuntimeError` si backend unavailable | Fallback : si pas de stream natif, une seule réponse |
| **stream_complete** | `async stream_complete(messages: list[dict], backend: str=None) -> PyTokenStream` | `messages` : `list[dict]` ; `backend` : str optionnel | Async iterator de chunks | `PyRuntimeError` si backend unavailable | Token par token en temps réel (vs `stream()` qui collecte) |
| **run_tools** | `async run_tools(messages: list[dict], tools: list[dict], max_iterations: int=5) -> dict` | `messages` : `list[dict]` ; `tools` : `list[dict]` JSON Schema ; `max_iterations` : int | dict avec `{"content": str,...}` | `PyRuntimeError` si max_iterations atteint ou budget épuisé | Boucle ReAct auto : Thought → Action → Observe |

### LlmResponse

Retourné par `chat()`, `complete()`, `stream()`.

| Propriété | Type | Notes |
|---|---|---|
| **content** | str | Texte généré par le modèle |
| **latency_ms** | int | Latence totale en millisecondes |
| **usage** | TokenUsage (objet) | — |
| **usage.prompt_tokens** | int | Tokens entrée |
| **usage.completion_tokens** | int | Tokens sortie |
| **usage.cost_usd** | float \| None | Coût estimé (`None` pour backends locaux) |

---

## ctx.memory – MemoryInterface

Accès à la mémoire persistante (épisodique, sémantique, procédurale). Namespace isolé par agent.

### Méthodes

| Méthode | Signature | Paramètres | Retour | Exceptions | Notes |
|---|---|---|---|---|---|
| **record** | `async record(content: str, importance: float=0.5, task_id: str=None, metadata: dict=None) -> str` | `content` : str ; `importance` : float [0.0-1.0] ; `task_id` : str optionnel ; `metadata` : dict optionnel | memory_id (str) | `RuntimeError` si no namespace | Enregistrement mémoire épisodique (horodaté) |
| **remember** | `async remember(key: str, value: str, *, source: str \| None = None, confidence: float \| None = None) -> None` | `key` : str ; `value` : str ; `source` : str optionnel ; `confidence` : float [0.0-1.0] optionnel | `None` | `RuntimeError` si no namespace | Enregistrement mémoire sémantique clé/valeur dans le namespace propre de l'agent |
| **remember_user** | `async remember_user(key: str, value: str, source: str \| None = None, confidence: float \| None = None) -> None` | `key` : str ; `value` : str ; `source` / `confidence` optionnels | `None` | `RuntimeError` si `user_memory_write ≠ true` dans le manifest ; `RuntimeError` si aucun `user_manager` configuré | Écrit dans le namespace global `__user__`. Réservé aux agents système (ex. `onboarding-agent`). Toute valeur écrite devient lisible par tous les agents via `recall()`. |
| **recall** | `async recall(key: str) -> str \| None` | `key` : str (clé de fait sémantique) | `str` (valeur stockée) ou `None` si absent | — | Cherche d'abord dans le namespace propre, puis dans `__user__` en fallback inconditionnellement (si `user_manager` configuré). |
| **recall_entry** | `async recall_entry(key: str) -> dict \| None` | `key` : str | dict complet `{"key": str, "value": str, "confidence": float, "source": str, "updated_at": str, "expires_at": str \| None}` ou `None` | — | Rappel complet avec toutes métadonnées |
| **recall_all** | `async recall_all(limit: int=100) -> list[dict]` | `limit` : int (défaut 100) | `list[dict]` (même format que `recall_entry()`) | — | Toutes les entrées sémantiques du namespace |
| **search** | `async search(query: str, limit: int=10) -> list[dict]` | `query` : str (texte libre) ; `limit` : int | `list[dict]` avec `{"content": str, "score": float, "type": str, "created_at": str}` | — | Recherche FTS5 cross-backend (épisodique + sémantique + procédurale) |
| **forget** | `async forget(memory_id: str) -> None` | `memory_id` : str (id retourné par `record()` ou `remember`) | `None` | `RuntimeError` si id invalid | Suppression d'un enregistrement |

---

## ctx.step_budget – StepBudgetView

Lecture seule. Permet à l'agent de s'adapter proactivement avant épuisement.

### Propriétés

| Propriété | Type | Notes |
|---|---|---|
| **steps_remaining** | int | Nombre d'étapes restantes |
| **tool_calls_remaining** | int | Appels d'outils restants |
| **elapsed_seconds** | float | Secondes écoulées depuis le démarrage |

---

## ctx.log — méthode `(level, message)`

Logs émis vers deux canaux en parallèle :

1. **`tracing::*`** du runtime (stderr, format structuré opérateur).
2. **`runtime_events.db`** comme `RuntimeEvent::AgentLog` (ADR-088,
   ADR-088 Lot 1) — visible dans la trace `ExecutionTrace` de l'UI et
   requêtable via `GET /api/v1/tasks/:id/trace`.

### Signature

```python
ctx.log(level: str, message: str) -> None
```

| Paramètre | Type | Notes |
|---|---|---|
| `level` | `str` | `"debug"` \| `"info"` \| `"warn"` \| `"error"` ; lève `ValueError` pour tout autre niveau |
| `message` | `str` | Message libre. Pré-formater côté agent (`f"…"`) — pas de templating runtime. |

Exemple :
```python
ctx.log("info", f"step started: tool=file_read")
ctx.log("warn", f"budget low: {ctx.step_budget.steps_remaining} steps left")
```

### Méthodes d'observabilité ReAct

Trois méthodes complémentaires pour pousser des événements typés dans la
trace event-sourced (ADR-088 Lot 2). Utilisées par le SDK `react.py`
pour exposer thoughts / retries / parse errors. Toutes *fire-and-forget*
(no-op silencieux si task_id ou bus absents).

| Méthode | Signature | Émet |
|---|---|---|
| **emit_thought** | `emit_thought(text: str, step_num: int)` | `RuntimeEvent::Thought` |
| **emit_retry** | `emit_retry(step_num: int, cause: str, attempt: int)` | `RuntimeEvent::Retry` |
| **emit_action_parse_error** | `emit_action_parse_error(step_num: int, raw_content: str, repair_attempted: bool)` | `RuntimeEvent::ActionParseError` |

`cause` accepte `"action_parse_error"`, `"tool_error"`, `"llm_error"`,
`"other"`. Les agents Python n'appellent rarement ces méthodes
directement — `BaseReActAgent` les déclenche au bon endroit dans la
boucle ReAct.

---

## ctx.workspace – WorkspaceContextPy

Contexte projet collecté au démarrage. Agrège les sections des providers actifs.

### Propriétés

| Propriété | Type | Notes |
|---|---|---|
| **rules** | str \| None | Alias pour `get("Règles du projet")` |
| **apollia_md** | str \| None | Alias pour `rules` (compatibilité) |
| **sections** | list[dict] | Toutes les sections `[{"title": str, "content": str},...]` |

### Méthodes

| Méthode | Signature | Paramètres | Retour | Notes |
|---|---|---|---|---|
| **get** | `get(title: str) -> str \| None` | `title` : str (titre de section) | Contenu ou `None` | Lookup par titre exact |

---

## ctx.user_context

Propriété dict ou None. Profil utilisateur injecté en mode chat.

| Propriété | Type | Notes |
|---|---|---|
| **user_context** | dict[str, list[tuple[str, str]]] \| None | Catégories : `"preferences"`, `"habits"`, `"context"` (chacune : liste de tuples `(clé, valeur)`) |

Exemple accès :
```python
uc = ctx.user_context
if uc:
    for key, val in uc.get("preferences", []):
        if key == "langue":
            # utiliser val
```

---

## ctx.send – Messagerie inter-agents

Envoie un message JSON asynchrone à un autre agent via mailbox.

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `agent_name` | str | — | ✅ | Nom de l'agent destinataire |
| `payload` | dict | — | ✅ | Données JSON arbitraires |
| **Retour** | awaitable (None) | — | — | — |
| **Erreurs** | — | — | — | `RuntimeError: A2A requires supports_a2a: true` ; `RuntimeError: mailbox not available` ; `RuntimeError: queue full` |

Limitation : max 100 messages en file par agent.

---

## ctx.receive – Réception inter-agents

Attend le prochain message dans la mailbox avec timeout.

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `timeout_seconds` | float | 5.0 | ❌ | Timeout en secondes |
| **Retour** | awaitable (dict \| None) | — | — | `{"from": str, "payload": dict, "sent_at": str}` ou `None` si timeout |
| **Erreurs** | — | — | — | `RuntimeError: A2A requires supports_a2a: true` ; `RuntimeError: mailbox not available` |

---

## ctx.delegate – Délégation A2A

Délègue une tâche à un Worker Agent via skill ID. Bas niveau, type-erasé.

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `skill_id` | str | — | ✅ | ID de compétence du Worker |
| `payload` | dict | — | ✅ | Données d'entrée JSON |
| `timeout_secs` | int \| None | 120 | ❌ | Timeout en secondes |
| **Retour** | awaitable (dict) | — | — | `{"task_id": str, "agent_name": str, "output": list[dict]}` |
| **Erreurs** | — | — | — | `RuntimeError: A2A requires supports_a2a: true` ; `RuntimeError: delegation not available` ; `RuntimeError: A2A cycle: agent X already in delegation chain` ; `RuntimeError: A2A max hops exceeded: limit is 5` ; Timeout |

---

## ctx.emit_token – Streaming chatbot

Émet un token vers le frontend SSE en mode chat. No-op en mode task.

| Paramètre | Type | Obligatoire | Notes |
|---|---|---|---|
| `token` | str | ✅ | Fragment texte à streaming |
| **Retour** | None | — | Fire-and-forget (erreurs silencieuses si bus plein) |

---

## ctx.a2a_invoke – Invocation A2A haut niveau

Invoque un Worker Agent via `A2AInvoker` (haut niveau, orchestration complète).

| Paramètre | Type | Défaut | Obligatoire | Notes |
|---|---|---|---|---|
| `skill_id` | str | — | ✅ | ID de compétence |
| `input` | dict | — | ✅ | Données d'entrée JSON |
| `timeout_secs` | int | — | ❌ | Timeout en secondes |
| **Retour** | awaitable (dict) | — | — | `{"result": dict, "agent_name": str, "skill_id": str, "duration_ms": int}` ou `AIPResult.failed` |

---

## ctx.a2a_discover – Découverte skill

Découvre l'agent qui expose un skill et retourne sa carte.

| Paramètre | Type | Obligatoire | Notes |
|---|---|---|---|
| `skill_id` | str | ✅ | ID de skill à découvrir |
| **Retour** | awaitable (dict \| None) | — | Carte de découverte ou `None` si non trouvé |

---

## ctx.user_memory_writable – Propriété

Indique si l'agent peut écrire dans la mémoire utilisateur globale via `ctx.memory.remember_user()`.

| Propriété | Type | Notes |
|---|---|---|
| **user_memory_writable** | bool | `True` uniquement pour les agents dont le manifest déclare `user_memory_write = true` (ex. `onboarding-agent`). La **lecture** de `__user__` via `recall()` est inconditionnelle — disponible à tout agent dès qu'un `user_manager` est configuré. |

---

## Corrections vs version précédente (780 → 320 lignes)

| Item | Ancien | Nouveau | Raison |
|---|---|---|---|
| **Structure générale** | Narrative (tutoriels) | Table canonique | Respect charte L1.4 : wiki = référence pure |
| **Section "Vue d'ensemble"** | 21 lignes + diagramme | Tableau synthétique 1 page | Condensé ; lien vers book pour patterns |
| **Chaque service** | 40-100 lignes narratives | Table : sig/params/retour/erreurs | Grille de référence consultable |
| **Exemples Python** | 10+ par service | 0 (sauf 1 pour syntax) | Exemples = book ch03/ch06, pas wiki |
| **Outils natifs** | Prose + exemples (200 lignes) | 10 tables (1 par outil) | Paramètres structurés = queryable |
| **ctx.delegate** | Suspect per Audit Axe 3 | ✅ Confirmé, signature actuelle | Vérification effectuée dans context.rs:1128-1184 |
| **ctx.llm.stream_complete** | Absent | ✅ Ajouté | Async iterator vs collect |
| **ctx.emit_token** | Absent | ✅ Ajouté | Mode chat streaming |
| **ctx.user_memory_writable** | Absent (ancien `user_memory_read_only`) | ✅ MAJ | Propriété booléenne — contrôle les écritures `__user__`, lecture toujours libre |
| **Métadonnées memory** | Narratif | Tables : `recall_entry()`, `recall_all()` | (injection tracker) |

---

## Pour apprendre (liens externes)

> Voir [book ch03](../../book/src/ch03-02-runtime-context.md) pour patterns d'usage avec exemples complets.
> Voir [book ch04](../../book/src/ch04-01-outils.md) pour tutoriel outils natifs.
> Voir [book ch05](../../book/src/ch05-01-memory.md) pour mémoire : concepts + patterns.
> Voir [book ch06](../../book/src/ch06-02-ctx-llm.md) pour LLM : choix backend, streaming, ReAct.

---

## Voir aussi

- [Briques-AIP-Specification](./Briques-AIP-Specification.md) — contrat `AIPTask`, `AIPResult`
- [Briques-Tool-Registry](./Briques-Tool-Registry.md) — catalogue complet outils + schémas JSON
- [Briques-Memory-Engine](./Briques-Memory-Engine.md) — backends mémoire, FTS5, namespaces
- [Briques-LLM-Backend](./Briques-LLM-Backend.md) — backends LLM, routing, feature flags
- [Outils-Reference](./Outils-Reference.md) — outils disponibles (autre source)
- [Agents-SDK-Guide](./Agents-SDK-Guide.md) — classes SDK Python, mocks de test
- [Agents-Bonnes-Pratiques](./Agents-Bonnes-Pratiques.md) — gestion StepBudget, coûts LLM

