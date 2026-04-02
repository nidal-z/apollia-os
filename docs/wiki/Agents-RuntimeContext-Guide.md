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
| `ctx.send()` / `ctx.receive()` | Messagerie inter-agents (AgentMailbox) | Coordination entre agents en parallèle |
| `ctx.delegate()` | Déléguer une tâche à un Worker Agent via A2A | Appeler une compétence spécialisée d'un autre agent |
| `ctx.user_context` | Lire le profil utilisateur (mode chat uniquement) | Personnaliser la réponse selon les préférences utilisateur |

`ctx.tools` et `ctx.step_budget` sont toujours disponibles. `ctx.memory` nécessite `memory_namespace` dans le manifest. `ctx.llm` nécessite un backend LLM configuré. `ctx.send()`, `ctx.receive()` et `ctx.delegate()` nécessitent `supports_a2a: True` dans le manifest. `ctx.user_context` est disponible uniquement en mode chat.

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
    # ctx.send()        — messagerie inter-agents (supports_a2a requis)
    # ctx.receive()     — réception messages inter-agents (supports_a2a requis)
    # ctx.delegate()    — délégation A2A vers Worker Agent (supports_a2a requis)
    # ctx.user_context  — profil utilisateur (mode chat uniquement, None sinon)
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

## ctx.send() / ctx.receive() — Messagerie inter-agents *(Sprint 20)*

**Nécessite `supports_a2a: True` dans le manifest.**

Permet à un agent d'envoyer et de recevoir des messages asynchrones avec d'autres agents via l'`AgentMailbox` — un acteur Tokio séparé du `TaskRouter`.

### ctx.send()

Envoie un message JSON à un autre agent identifié par son nom.

```python
# Signature : async send(agent_name: str, payload: dict) -> None
await ctx.send(
    "worker-agent",                          # str — nom de l'agent destinataire
    {"type": "data", "content": "résultat"}, # dict — payload JSON arbitraire
)
```

| Paramètre | Type | Description |
|---|---|---|
| `agent_name` | `str` | Nom de l'agent destinataire (doit être démarré) |
| `payload` | `dict` | Données JSON à envoyer (serde_json::Value) |

**Erreurs :**
- `RuntimeError: "A2A messaging requires supports_a2a: true in manifest"` — manifest incorrect
- `RuntimeError: "A2A mailbox not available in this runtime context"` — mailbox non disponible
- `RuntimeError` (MailboxError::QueueFull) — file pleine (max 100 messages par agent)

### ctx.receive()

Attend le prochain message dans la mailbox de l'agent courant avec un timeout.

```python
# Signature : async receive(timeout_seconds: float = 5.0) -> dict | None
msg = await ctx.receive(timeout=10.0)  # timeout en secondes, défaut: 5.0
if msg is not None:
    sender   = msg["from"]     # str — nom de l'agent émetteur
    payload  = msg["payload"]  # dict — données reçues
    sent_at  = msg["sent_at"]  # str — horodatage ISO 8601
```

**Retour :** `dict` avec les clés `from`, `payload`, `sent_at` — ou `None` si timeout expiré.

**Erreurs :**
- `RuntimeError: "A2A messaging requires supports_a2a: true in manifest"` — manifest incorrect
- `RuntimeError: "A2A mailbox not available in this runtime context"` — mailbox non disponible

### Contraintes

- Max **100 messages** en file par agent — au-delà : `RuntimeError` (MailboxError::QueueFull)
- Les messages sont des `serde_json::Value` (JSON arbitraire côté Rust)
- L'agent destinataire **doit être démarré** — pas de persistance hors-mémoire
- L'`AgentMailbox` est un acteur Tokio **séparé** du `TaskRouter`

### Exemple — coordination pipeline

```python
# agent-coordinator.py
class CoordinatorAgent:
    def manifest(self):
        return {
            "name": "coordinator",
            "version": "1.0.0",
            "description": "Coordonne deux agents en parallèle",
            "tools_required": [],
            "supports_a2a": True,  # obligatoire pour send/receive
        }

    async def run(self, task, ctx):
        input_data = task["input"]["parts"][0]["text"]

        # Envoyer à deux workers en parallèle
        await ctx.send("worker-a", {"job": "analyser", "data": input_data})
        await ctx.send("worker-b", {"job": "résumer",  "data": input_data})

        # Récupérer les réponses (dans l'ordre d'arrivée)
        results = []
        for _ in range(2):
            msg = await ctx.receive(timeout=30.0)
            if msg:
                results.append(msg["payload"])

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "data", "data": {"results": results}}],
        }
```

---

## ctx.delegate() — Délégation A2A vers Worker Agent *(Sprint 32)*

**Nécessite `supports_a2a: True` dans le manifest.**

Délègue une tâche à un Worker Agent identifié par son `skill_id`. Méthode A2A de bas niveau — expose directement la `A2aDelegateFn` injectée par le runtime dans les Director Agents en Mode Orchestré.

```python
# Signature : async delegate(skill_id: str, payload: dict, timeout_secs: int = 120) -> dict
result = await ctx.delegate(
    skill_id="generate-quote",                  # str — ID de la compétence
    payload={"client": "Acme", "amount": 5000}, # dict — données d'entrée JSON
    timeout_secs=120,                           # int | None — défaut: 120s
)
```

| Paramètre | Type | Obligatoire | Défaut | Description |
|---|---|---|---|---|
| `skill_id` | `str` | oui | — | Identifiant de la compétence du Worker Agent cible |
| `payload` | `dict` | oui | — | Données d'entrée JSON sérialisables |
| `timeout_secs` | `int \| None` | non | `120` | Timeout total de la délégation en secondes |

**Retour :** `dict` avec les clés :

| Clé | Type | Description |
|---|---|---|
| `task_id` | `str` | UUID de la tâche déléguée |
| `agent_name` | `str` | Nom de l'agent qui a exécuté la compétence |
| `output` | `list[dict]` | Résultat : liste d'`AIPPart` (même format que AIPResult) |

**Erreurs :**
- `RuntimeError: "A2A delegation requires supports_a2a: true in manifest"` — manifest incorrect
- `RuntimeError: "A2A delegation not available in this runtime context"` — contexte non-orchestré (injectée uniquement pour les Director Agents)

### Exemple — Director Agent qui délègue

```python
# director_agent.py
class DirectorAgent:
    def manifest(self):
        return {
            "name": "director",
            "version": "1.0.0",
            "description": "Orchestre plusieurs workers spécialisés",
            "tools_required": [],
            "supports_a2a": True,
            "execution_mode": "orchestrated",
        }

    async def run(self, task, ctx):
        brief = task["input"]["parts"][0]["text"]

        # Déléguer la génération de devis à un Worker Agent
        result = await ctx.delegate(
            skill_id="generate-quote",
            payload={"brief": brief},
            timeout_secs=60,
        )

        # Extraire le résultat
        output_parts = result["output"]  # list[AIPPart]
        agent_used   = result["agent_name"]
        ctx.log.info("delegation_done", agent=agent_used, parts=len(output_parts))

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": output_parts,
        }
```

---

## ctx.user_context — Contexte utilisateur global *(Sprint 28)*

**Disponible uniquement en mode chat.** `None` en mode task.

Propriété (pas une méthode) qui expose les entrées de mémoire utilisateur (`__user__`) chargées depuis le namespace global via `recall_all()` au démarrage de la session chat. L'agent décide quoi en faire — jamais d'injection automatique dans les prompts (Principe #6).

```python
# ctx.user_context : dict[str, list[tuple[str, str]]] | None
uc = ctx.user_context
if uc is not None:
    # Catégories disponibles
    prefs  = uc.get("preferences", [])  # list[tuple[str, str]] — préférences explicites
    habits = uc.get("habits", [])       # list[tuple[str, str]] — habitudes détectées
    ctxts  = uc.get("context", [])      # list[tuple[str, str]] — contexte situationnel
```

### Structure des catégories

| Catégorie | Description | Exemple |
|---|---|---|
| `preferences` | Préférences explicites de l'utilisateur | `("langue", "français")` |
| `habits` | Habitudes détectées par le système de mémoire | `("format_réponse", "bullet points")` |
| `context` | Contexte situationnel courant | `("projet_courant", "apollia-os")` |

### Exemple — personnalisation de réponse

```python
async def run(self, task, ctx):
    user_input = task["input"]["parts"][0]["text"]

    # Construire un system prompt personnalisé selon le profil
    system = "Tu es un assistant bienveillant."
    uc = ctx.user_context
    if uc:
        # Appliquer les préférences de langue
        for key, val in uc.get("preferences", []):
            if key == "langue":
                system += f" Réponds toujours en {val}."
        # Appliquer les habitudes de format
        for key, val in uc.get("habits", []):
            if key == "format_réponse":
                system += f" Format : {val}."

    if ctx.llm:
        response = await ctx.llm.chat(system=system, user=user_input)
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": response.content}],
        }
```

**Règles :**
- `None` si l'agent n'est pas invoqué depuis une `ChatSession`
- `None` si le namespace `__user__` est vide (aucune entrée mémorisée)
- Lecture seule — l'agent ne peut pas modifier `user_context` directement (utiliser `ctx.memory` pour écrire dans `__user__`)
- L'agent est responsable de décider si et comment utiliser ce contexte (Principe #6)

---

## Voir aussi

- [Agents SDK Guide](./Agents-SDK-Guide) — SDK Python avec classes de base, mocks de test et scaffolding
- [Briques AIP Specification](./Briques-AIP-Specification) — contrat complet AIPTask, AIPResult, AgentManifest
- [Briques Tool Registry](./Briques-Tool-Registry) — catalogue des outils, schémas complets
- [Briques Memory Engine](./Briques-Memory-Engine) — backends mémoire, FTS5, namespaces
- [Briques LLM Backend](./Briques-LLM-Backend) — backends LLM, feature flags, configuration
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — gestion du StepBudget, coûts LLM
