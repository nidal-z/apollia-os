# Agents — RuntimeContext Guide — Apollia OS

> Référence complète de tous les services disponibles via `ctx` dans la méthode `run()`.
> Public cible : développeur d'agent Python intermédiaire

---

## Vue d'ensemble en 1 minute

Le `RuntimeContext` (`ctx`) est votre agent's interface avec le runtime. Quatre services, quatre responsabilités :

| Service | Rôle | Quand l'utiliser |
|---|---|---|
| `ctx.tools` | Appeler des outils (bash, fichiers, réseau, MCP) | Toute interaction avec le monde extérieur |
| `ctx.memory` | Stocker/retrouver des souvenirs persistants | Enrichir le contexte entre les tâches |
| `ctx.llm` | Appeler un LLM (raisonnement, résumé, extraction) | Tout ce qui nécessite de l'intelligence |
| `ctx.step_budget` | Lire le budget restant (lecture seule) | Adapter le comportement avant épuisement |

`ctx.tools` est toujours disponible. `ctx.memory` nécessite `memory_namespace` dans le manifest. `ctx.llm` nécessite un backend LLM configuré. `ctx.step_budget` est toujours disponible (lecture seule, le runtime gère le décompte).

## Détail

Le `RuntimeContext` (accessible via le paramètre `ctx` dans `run()`) est l'interface entre votre agent et tous les services du runtime. Il est injecté par Apollia OS à chaque appel de tâche — vous n'avez jamais à l'instancier.

```python
async def run(self, task, ctx):
    # ctx donne accès à :
    # ctx.tools         — ToolProxy : invocation des outils
    # ctx.memory        — MemoryInterface | None : mémoire persistante
    # ctx.llm           — LlmProxy | None : appels LLM (None si aucun backend configuré)
    # ctx.log           — AgentLogger : logs structurés
    # ctx.step_budget   — StepBudgetView : budget restant (lecture seule)
```

---

## ctx.tools — ToolProxy

Disponible si au moins un outil est déclaré dans `tools_required` ou `tools_optional`.

### Appeler un outil

```python
result = await ctx.tools.call("nom_outil", {"param": "valeur"})
# result : dict Python (JSON désérialisé depuis le résultat Rust)
```

Les appels sont automatiquement :
- Vérifiés contre les permissions de l'agent (seuls `tools_required` + `tools_optional` sont accessibles)
- Enregistrés dans l'audit trail SQLite (fire-and-forget)
- Comptabilisés dans le `StepBudget`

### Outils natifs disponibles

#### bash_executor

Exécute une commande bash dans un namespace Linux isolé (ou en mode dev sur macOS).

```python
result = await ctx.tools.call("bash_executor", {
    "command": "ls -la /tmp",
    "timeout_seconds": 30,     # optionnel, défaut: 30
    "working_dir": "/tmp",     # optionnel
})
# result : {"stdout": "...", "stderr": "...", "exit_code": 0}
```

#### file_read

Lit un fichier avec protection path traversal. Supporte la lecture partielle par plage de lignes.

```python
result = await ctx.tools.call("file_read", {
    "path": "data/config.json",
    "offset": 1,    # optionnel, 1-based line number
    "limit": 50,    # optionnel, max lignes à retourner
})
# result : {"content": "    1\t{\n    2\t  \"key\": ...", "total_lines": 42, "truncated": false}
```

#### file_write

Écrit un fichier (crée ou remplace). Protection path traversal.

```python
result = await ctx.tools.call("file_write", {
    "path": "output/rapport.txt",
    "content": "Contenu du rapport...",
})
# result : {"bytes_written": 1234, "path": "output/rapport.txt"}
```

#### file_edit

Remplace une chaîne exacte dans un fichier. Échoue si `old_str` est absent ou non-unique.

```python
result = await ctx.tools.call("file_edit", {
    "path": "src/agent.py",
    "old_str": "version = \"1.0.0\"",
    "new_str": "version = \"1.1.0\"",
})
# result : {"replaced": true, "path": "src/agent.py"}
# Échoue si old_str absent ou non-unique dans le fichier.
```

#### file_list

Liste les entrées d'un répertoire avec profondeur configurable.

```python
result = await ctx.tools.call("file_list", {
    "path": ".",
    "depth": 2,     # optionnel
})
# result : {"entries": [{"name": "...", "is_dir": false, "size": 1234}, ...]}
```

#### file_glob

Recherche de fichiers par pattern glob.

```python
result = await ctx.tools.call("file_glob", {
    "pattern": "**/*.py",
    "path": ".",    # optionnel, répertoire de départ
})
# result : {"matches": ["src/agent.py", "tests/test_agent.py"], "count": 2}
```

#### file_grep

Recherche par expression régulière dans les fichiers, avec filtre glob et lignes de contexte.

```python
result = await ctx.tools.call("file_grep", {
    "pattern": "def run\\(",     # regex
    "path": ".",
    "glob": "*.py",              # optionnel, filtre fichiers
    "context_lines": 2,          # optionnel, lignes de contexte
})
# result : {"matches": [{"file": "src/agent.py", "line": 12, "content": "..."}], "count": 3}
```

#### http_fetch

Effectue une requête HTTP. Requiert que le domaine cible soit dans `network_allowlist` du manifest.

```python
result = await ctx.tools.call("http_fetch", {
    "url": "https://api.exemple.com/data",
    "method": "GET",                         # optionnel, défaut GET
    "headers": {"Authorization": "Bearer x"}, # optionnel
    "timeout_secs": 15,                      # optionnel
})
# result : {"status": 200, "body": "...", "headers": {...}}
# Requiert que api.exemple.com soit dans network_allowlist du manifest.
```

#### memory_search

Recherche dans la mémoire persistante de l'agent ou d'un namespace explicite.

```python
result = await ctx.tools.call("memory_search", {
    "query": "devis client Dupont",
    "namespace": "crm-agent",  # optionnel, défaut = namespace propre
    "limit": 10,               # optionnel, max 50
    "source": "episodic",      # optionnel : "episodic" | "semantic"
})
# result : {"results": [{"content": "...", "score": 0.92, "source": "episodic"}], "count": 3}
```

#### python_executor

Exécute du code Python dans un venv isolé par agent.

```python
result = await ctx.tools.call("python_executor", {
    "code": "import json\nprint(json.dumps({'result': 42}))",
    "timeout_seconds": 60,  # optionnel
})
# result : {"stdout": '{"result": 42}\n', "stderr": "", "exit_code": 0}
```

### Lister les outils accessibles

```python
available = ctx.tools.list_tools()
# ["bash_executor", "python_executor", "file_read", "file_write", "file_edit", "file_list", "file_glob", "file_grep", "http_fetch", "memory_search"]
```

### Compter les appels

```python
count = ctx.tools.tool_call_count()
# Utile pour adapter le comportement proche de la limite StepBudget
```

---

## ctx.memory — MemoryInterface

**Disponible uniquement si `memory_namespace` est défini dans le manifest.** `None` sinon.

Le Memory Engine distingue trois types de mémoire. `ctx.memory` expose une interface unifiée pour les trois :

| Type | Ce qu'il stocke | Exemple | Score clé |
|---|---|---|---|
| **Épisodique** | Événements horodatés | "Client Acme a demandé 10 licences à 5000€" | `importance` (0.0-1.0) : à quel point cet événement est significatif |
| **Sémantique** | Faits durables (clé→valeur) | "budget_annuel_acme" → "50 000€" | `confidence` (0.0-1.0) : degré de certitude du fait |
| **Procédurale** | Procédures réutilisables | "Pour créer un devis : 1. Vérifier le client..." | `confidence` (0.0-1.0) |

> `importance` mesure la pertinence d'un événement pour le contexte futur. `confidence` mesure la fiabilité d'une information (1.0 = déclaré par l'utilisateur, 0.5 = inféré par un LLM).

### Stocker un épisode

La mémoire épisodique enregistre des événements avec un score d'importance et un timestamp.

```python
if ctx.memory:
    await ctx.memory.record(
        "Client Acme a demandé 10 licences Figma à 5000€ max",
        importance=0.8,           # float 0.0-1.0
        task_id=task["task_id"],  # lie l'épisode à la tâche
        metadata={                # dict optionnel — enrichissement
            "client": "Acme",
            "product": "Figma",
            "budget": 5000
        }
    )
```

### Stocker un fait

La mémoire sémantique enregistre des faits structurés avec un score de confiance.

```python
if ctx.memory:
    await ctx.memory.remember(
        "Le budget max d'Acme est 5000€",
        confidence=0.9,           # float 0.0-1.0
        source=task["task_id"]    # traçabilité
    )
```

### Rappeler des faits

```python
if ctx.memory:
    facts = await ctx.memory.recall("budget Acme")
    # facts : list[dict] avec "content", "confidence", "created_at"
    for fact in facts:
        print(fact["content"])  # "Le budget max d'Acme est 5000€"
```

### Recherche full-text

Recherche FTS5 + BM25 cross-backend (épisodique + sémantique + procédurale).

```python
if ctx.memory:
    results = await ctx.memory.search(
        "licences Figma",
        limit=5    # optionnel, défaut: 10
    )
    # results : list[dict] avec "content", "score", "type", "created_at"
    for r in results:
        print(f"[{r['score']:.2f}] {r['content']}")
```

### Supprimer un enregistrement

```python
if ctx.memory:
    await ctx.memory.forget(memory_id)
    # memory_id : str — id retourné par record() ou remember()
```

### Pattern de mémoire contextuelle

```python
async def run(self, task, ctx):
    user_input = task["input"]["parts"][0]["text"]

    # 1. Chercher le contexte pertinent AVANT de traiter
    context_from_memory = []
    if ctx.memory:
        results = await ctx.memory.search(user_input, limit=3)
        context_from_memory = [r["content"] for r in results]

    # 2. Traiter avec le contexte
    response = await self._generate_response(user_input, context_from_memory)

    # 3. Mémoriser le résultat APRÈS traitement
    if ctx.memory:
        await ctx.memory.record(
            f"Q: {user_input} → R: {response[:100]}",
            importance=0.6,
            task_id=task["task_id"]
        )

    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [{"type": "text", "text": response}],
    }
```

---

## ctx.llm — LlmProxy

**Disponible uniquement si au moins un backend LLM est configuré dans `apollia.toml`.** `None` sinon.

Quand `ctx.llm` est `None`, le runtime émet automatiquement `RuntimeEvent::AgentDegraded` sur l'EventBus (visible dans `apollia-os status` et les logs). L'agent peut continuer à tourner en mode dégradé — aucun crash, aucune exception Python.

```python
if ctx.llm is None:
    # AgentDegraded déjà émis par le runtime — l'agent décide quoi faire
    return {"task_id": task["task_id"], "status": "failed",
            "error": "LLM backend requis mais non disponible"}
```

### Propriété `default_backend`

```python
# Connaître le backend par défaut utilisé (utile pour les logs)
print(ctx.llm.default_backend)   # "local" | "anthropic" | "gpt-4o-mini" ...
```

### Chat simple (80% des cas)

Un system prompt + un message utilisateur → une réponse.

```python
response = await ctx.llm.chat(
    system="Tu es un assistant commercial expert en devis.",
    user=task["input"]["parts"][0]["text"],
)
# response.content : str — texte généré
# response.usage.prompt_tokens : int
# response.usage.completion_tokens : int
# response.usage.cost_usd : float | None  (None pour les backends locaux)
# response.latency_ms : int
print(response.content)
```

### Conversation multi-tour

Pour les flux avec historique ou les rôles system/user/assistant explicites.

```python
response = await ctx.llm.complete([
    {"role": "system",    "content": "Sois concis. Réponds en 3 points max."},
    {"role": "user",      "content": "Quels sont les avantages du cloud ?"},
    {"role": "assistant", "content": "1. Scalabilité 2. Coût variable 3. ..."},
    {"role": "user",      "content": "Et les inconvénients ?"},
])
print(response.content)
```

### Streaming

Retourne une liste de chunks texte. Utile pour les réponses longues.

```python
chunks = await ctx.llm.stream([
    {"role": "user", "content": "Génère un rapport détaillé sur..."},
])
full_response = "".join(chunks)
```

`stream()` retourne toujours une `list[str]`. Si le backend ne supporte pas le streaming nativement, un seul chunk contenant la réponse complète est retourné (fallback silencieux — le code de l'agent ne change pas).

### Boucle ReAct automatique — `run_tools()`

Délègue la boucle Thought → Action → Observe au LLM. Idéal pour les agents qui laissent le modèle décider des outils à utiliser.

> **Types importants :** `messages` et `tools` sont des **`list[dict]`** Python — pas des objets Rust. La sérialisation est gérée automatiquement par le bridge PyO3.

```python
result = await ctx.llm.run_tools(
    messages=[                   # list[dict] avec clés "role" et "content"
        {"role": "system", "content": "Tu es un assistant qui lit des fichiers."},
        {"role": "user",   "content": "Lis le fichier config.json et résume-le."},
    ],
    tools=[                      # list[dict] — schéma JSON Schema
        {
            "name":        "file_read",
            "description": "Lit un fichier local avec protection path traversal.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path":   {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit":  {"type": "integer"},
                },
                "required": ["path"],
            },
        }
    ],
    max_iterations=5,   # garde-fou : max 5 aller-retours LLM ↔ outils
)
# result.content : str — réponse finale après toutes les boucles
# result.usage.prompt_tokens : int (cumul de toutes les itérations)
print(result.content)
```

La boucle `run_tools()` :
1. Appelle le LLM avec les outils disponibles
2. Si `finish_reason == tool_calls` → exécute les outils via `ctx.tools` (erreurs absorbées comme texte, jamais fatales)
3. Injecte les résultats comme messages `role: tool`
4. Répète jusqu'à `finish_reason == stop` ou `max_iterations` atteint → `PyRuntimeError`
5. Si `StepBudget` épuisé → `PyRuntimeError` immédiat

### Choisir un backend spécifique

Si plusieurs backends sont configurés, il est possible d'en choisir un explicitement.

```python
# Utiliser le backend anthropic pour une tâche spécifique
response = await ctx.llm.chat(
    system="...",
    user="...",
    backend="anthropic",   # override du backend par défaut
)
```

### Pattern complet — agent LLM avec mémoire

```python
async def run(self, task, ctx):
    user_input = task["input"]["parts"][0]["text"]

    # 1. Contexte mémoriel
    memory_context = ""
    if ctx.memory:
        results = await ctx.memory.search(user_input, limit=3)
        if results:
            memory_context = "\n".join(r["content"] for r in results)

    # 2. Appel LLM avec contexte
    if ctx.llm is None:
        return {"task_id": task["task_id"], "status": "failed",
                "error": "LLM requis"}

    system_prompt = "Tu es un assistant commercial."
    if memory_context:
        system_prompt += f"\n\nContexte mémorisé :\n{memory_context}"

    response = await ctx.llm.chat(system=system_prompt, user=user_input)

    # 3. Mémoriser la réponse
    if ctx.memory:
        await ctx.memory.record(
            f"Q: {user_input[:80]} → R: {response.content[:80]}",
            importance=0.7,
            task_id=task["task_id"],
        )

    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [{"type": "text", "text": response.content}],
    }
```

---

## ctx.log — AgentLogger

Logs structurés envoyés via le système de logging du runtime (`tracing`).

```python
ctx.log.info("step_started", step=1, tool="file_read")
ctx.log.warn("budget_low", steps_remaining=2)
ctx.log.error("tool_failed", tool="bash_executor", reason="timeout")
ctx.log.debug("internal_state", state={"key": "val"})
```

Ces logs apparaissent dans les logs du runtime avec le contexte agent/tâche automatiquement ajouté. Ils ne sont pas stockés en mémoire persistante — c'est l'audit trail qui joue ce rôle pour les appels d'outils.

---

## ctx.step_budget — StepBudgetView

Lecture seule. Permet à l'agent d'adapter son comportement proactivement avant que le runtime n'intervienne.

```python
async def run(self, task, ctx):
    while True:
        # Vérifier le budget avant chaque itération
        if ctx.step_budget.steps_remaining < 2:
            # Conclure proprement plutôt que d'être interrompu
            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": "Résultat partiel (budget faible)"}],
            }

        # ... traiter une étape
```

```python
# Propriétés disponibles
steps_remaining      = ctx.step_budget.steps_remaining       # int
tool_calls_remaining = ctx.step_budget.tool_calls_remaining   # int
elapsed_seconds      = ctx.step_budget.elapsed_seconds        # float
```

**Note :** l'agent ne peut pas modifier le budget. Le runtime le plafonne toujours via `from_capped(agent_budget, runtime_defaults)`.

---

## Voir aussi

- [Agents SDK Guide](./Agents-SDK-Guide) — SDK Python avec classes de base, mocks de test et scaffolding
- [Briques AIP Specification](./Briques-AIP-Specification) — contrat complet AIPTask, AIPResult, AgentManifest
- [Briques Tool Registry](./Briques-Tool-Registry) — catalogue des outils, schémas complets
- [Briques Memory Engine](./Briques-Memory-Engine) — backends mémoire, FTS5, namespaces
- [Briques LLM Backend](./Briques-LLM-Backend) — backends LLM, feature flags, configuration
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — gestion du StepBudget, coûts LLM
