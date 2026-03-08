# Briques — AIP Specification — Apollia OS

> Spécification complète de l'Agent Interface Protocol : contrat duck typing, types de données, RuntimeContext et exemples fonctionnels.
> Public cible : développeur d'agent Python, contributeur Rust

---

## Vue d'ensemble

L'Agent Interface Protocol (AIP) est le contrat minimal entre un agent Python et le runtime Apollia OS. Sa philosophie tient en une phrase : **un agent est n'importe quel objet Python avec `manifest()` et `async run()`**.

Pas de classe de base obligatoire. Pas de framework à apprendre. Un agent LangGraph, CrewAI, AutoGen ou entièrement custom peut tourner dans Apollia OS avec moins de 10 lignes d'adaptation.

L'AIP définit quatre composants : le `AgentManifest` (identité et capacités), le `ProcessState` (lifecycle du processus), le `AIPTask`/`AIPResult` (contrat de communication), et le `RuntimeContext` (services injectés).

---

## Composant 1 — AgentManifest

La carte d'identité de l'agent. Retournée par `manifest()` sous forme de dict Python ou d'objet sérialisable en JSON. Le runtime la convertit en `AgentManifest` Rust via serde_json à l'état `INITIALIZING`.

### Tous les champs

```python
def manifest(self):
    return {
        # Obligatoires
        "name": "mon-agent",           # str — identifiant unique dans le runtime
        "version": "1.0.0",            # str — semver
        "description": "...",          # str — description humaine

        # Outils (validation fail-fast à INITIALIZING)
        "tools_required": ["file_io"], # list[str] — absent = agent ne démarre pas
        "tools_optional": ["mcp:fs"],  # list[str] — absent = état DEGRADED, pas fatal

        # Mémoire
        "memory_namespace": "mon-ns",  # str | None — None = pas de mémoire persistante
        "shared_memory_namespaces": [], # list[str] — namespaces partagés en lecture

        # Concurrence et budget
        "max_concurrent_tasks": 1,     # int — défaut: 1
        "step_budget": {               # dict | None — None = défauts runtime
            "max_steps": 20,           # int — défaut runtime: 10
            "max_tool_calls": 40,      # int — défaut runtime: 20
            "wall_clock_timeout_secs": 300  # int — défaut runtime: 300
        },

        # Réseau
        "network_allowlist": None,     # list[str] | None — None = pas de réseau

        # Sécurité
        "dangerous_tools_allowed": False,  # bool — défaut: False

        # Protocoles
        "supports_streaming": False,   # bool — SSE si True
        "supports_a2a": False,         # bool — AgentCard A2A si True

        # Métadonnées
        "tags": ["finance", "crm"],    # list[str]
        "skills": [],                  # list[AgentSkill dict]
    }
```

### Champs obligatoires vs optionnels

| Champ | Obligatoire | Défaut | Effet si absent |
|---|---|---|---|
| `name` | oui | — | Erreur démarrage |
| `version` | oui | — | Erreur démarrage |
| `description` | oui | — | Erreur démarrage |
| `tools_required` | oui | `[]` | Erreur démarrage |
| `tools_optional` | non | `[]` | Ignoré |
| `memory_namespace` | non | `None` | `ctx.memory` est `None` |
| `max_concurrent_tasks` | non | `1` | 1 tâche à la fois |
| `step_budget` | non | `None` | Défauts runtime (10 steps, 20 calls, 300s) |
| `dangerous_tools_allowed` | non | `False` | Outils dangereux bloqués |
| `supports_a2a` | non | `False` | Pas de AgentCard A2A |

### Structure AgentSkill

Utilisée si `supports_a2a: True` pour construire automatiquement la AgentCard :

```python
{
    "id": "generate-quote",
    "name": "Génération de devis",
    "description": "Génère un devis PDF à partir d'un brief client",
    "input_modes": ["text", "data"],
    "output_modes": ["file", "text"]
}
```

---

## Composant 2 — AIPTask

Ce que le runtime envoie à l'agent via `run(task, ctx)`. En Python, `task` est un dict JSON.

```python
async def run(self, task, ctx):
    # task est un dict avec ces clés :
    task_id    = task["task_id"]           # str — UUID généré par le runtime
    context_id = task["context_id"]        # str — groupe de tâches liées
    parts      = task["input"]["parts"]    # list[dict] — AIPPart
    history    = task.get("history", [])   # list[dict] — messages précédents
    timeout    = task.get("timeout_seconds")  # int | None
```

### Structure AIPPart

Les parties sont polymorphes via le champ `type` :

```python
# TextPart
{"type": "text", "text": "Générer un devis pour 10 licences Figma"}

# DataPart
{"type": "data", "data": {"client": "Acme", "budget": 5000}}

# FilePart
{"type": "file", "name": "brief.pdf", "mime_type": "application/pdf",
 "data": "<base64>", "uri": None}
```

### Accéder au contenu

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]

    # Texte brut
    text_parts = [p["text"] for p in parts if p["type"] == "text"]
    user_input = text_parts[0] if text_parts else ""

    # Données structurées
    data_parts = [p["data"] for p in parts if p["type"] == "data"]
    structured = data_parts[0] if data_parts else {}
```

---

## Composant 3 — AIPResult

Ce que l'agent retourne. Peut être un dict Python ou un objet JSON-sérialisable.

```python
# Format dict minimal
return {
    "task_id": task["task_id"],      # str — obligatoire
    "status": "completed",           # str — voir TaskStatus
    "output": [                      # list[AIPPart] — résultat
        {"type": "text", "text": "Résultat..."}
    ],
}

# Avec erreur
return {
    "task_id": task["task_id"],
    "status": "failed",
    "error": {
        "code": "INVALID_INPUT",
        "message": "Le champ 'client' est requis"
    }
}

# Human-in-the-loop
return {
    "task_id": task["task_id"],
    "status": "input_required",
    "input_request": {
        "type": "text",
        "prompt": "Quel budget maximum pour ce devis ?"
    }
}
```

### Valeurs TaskStatus

| Valeur | Signification |
|---|---|
| `"completed"` | Tâche terminée avec succès |
| `"failed"` | Erreur non récupérable |
| `"input_required"` | Attente d'une entrée humaine |
| `"canceled"` | Annulée par le runtime ou l'opérateur |

---

## Composant 4 — RuntimeContext

Le deuxième argument de `run()`. Injecté par le runtime. Donne accès à tous les services.

```python
async def run(self, task, ctx):
    # ctx.tools — ToolProxy (toujours disponible)
    result = await ctx.tools.call("file_io", {"action": "list", "path": "."})

    # ctx.memory — MemoryInterface | None (None si pas de memory_namespace)
    if ctx.memory:
        await ctx.memory.record("Tâche reçue", importance=0.5,
                                task_id=task["task_id"])

    # ctx.log — logs structurés via le runtime
    ctx.log.info("processing_task", task_id=task["task_id"])

    # ctx.step_budget — lecture seule (StepBudgetView)
    remaining = ctx.step_budget.steps_remaining
    if remaining < 3:
        ctx.log.warn("budget_low", steps_remaining=remaining)
```

### ctx.tools — ToolProxy

```python
# Appeler un outil
result = await ctx.tools.call("bash_executor", {"command": "ls -la /tmp"})
# result est un dict Python issu du JSON retourné par l'outil

# Lister les outils disponibles pour cet agent
available = ctx.tools.list_tools()  # list[str]

# Compter les appels (pour diagnostiquer)
count = ctx.tools.tool_call_count()  # int
```

### ctx.memory — MemoryInterface

Disponible uniquement si `memory_namespace` est défini dans le manifest. `None` sinon.

```python
if ctx.memory:
    # Stocker un épisode (mémoire épisodique)
    await ctx.memory.record(
        "Client Acme a demandé 10 licences Figma",
        importance=0.7,
        task_id=task["task_id"],
        metadata={"client": "Acme", "product": "Figma"}
    )

    # Stocker un fait (mémoire sémantique)
    await ctx.memory.remember(
        "Le budget max d'Acme est 5000€",
        confidence=0.9,
        source=task["task_id"]
    )

    # Récupérer un fait précis
    facts = await ctx.memory.recall("budget max Acme")

    # Recherche full-text (FTS5 + BM25)
    results = await ctx.memory.search("licences Figma", limit=5)
    # results : list[dict] avec clé "content" et "score"

    # Supprimer un enregistrement
    await ctx.memory.forget(memory_id)
```

### ctx.step_budget — StepBudgetView

```python
# Lecture seule — l'agent ne peut pas modifier le budget
remaining_steps = ctx.step_budget.steps_remaining      # int
remaining_calls = ctx.step_budget.tool_calls_remaining  # int
elapsed_secs    = ctx.step_budget.elapsed_seconds       # float
```

---

## Agent minimal complet

```python
# minimal_agent.py
class MinimalAgent:
    def manifest(self):
        return {
            "name": "minimal-agent",
            "version": "1.0.0",
            "description": "Agent sans outils ni mémoire",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        text = parts[0]["text"] if parts else ""
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Reçu : {text}"}],
        }

agent = MinimalAgent()
```

Déployer :

```bash
$ apollia-os agent start ./minimal_agent.py
✓ minimal-agent [ACTIVE]

$ apollia-os run minimal-agent "test"
Reçu : test
```

---

## Agent avec mémoire et outils

```python
# full_agent.py
class FullAgent:
    def manifest(self):
        return {
            "name": "full-agent",
            "version": "1.0.0",
            "description": "Agent avec mémoire et outils fichiers",
            "tools_required": ["file_io"],
            "memory_namespace": "full-agent-memory",
            "max_concurrent_tasks": 2,
        }

    async def run(self, task, ctx):
        user_input = task["input"]["parts"][0]["text"]

        # Lire l'historique mémoriel pertinent
        past = []
        if ctx.memory:
            results = await ctx.memory.search(user_input, limit=3)
            past = [r["content"] for r in results]

        # Appeler un outil
        files = await ctx.tools.call("file_io", {
            "action": "list",
            "path": ".",
            "pattern": "*.py"
        })

        # Mémoriser cette interaction
        if ctx.memory:
            await ctx.memory.record(
                f"Requête : {user_input}",
                importance=0.6,
                task_id=task["task_id"]
            )

        response = f"Fichiers Python : {files.get('files', [])}"
        if past:
            response += f"\n(Contexte mémoriel : {past[0]})"

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": response}],
        }

agent = FullAgent()
```

---

## Validation duck typing

Le runtime valide via inspection Python à `INITIALIZING` :

1. L'objet `agent` doit exister au niveau module (`agent = MyAgent()`)
2. `hasattr(agent, 'manifest')` doit être `True`
3. `manifest()` doit retourner un dict JSON-sérialisable avec `name`, `version`, `description`, `tools_required`
4. `hasattr(agent, 'run')` doit être `True`
5. `run` doit être une coroutine async (`asyncio.iscoroutinefunction`)

Si une validation échoue, l'agent s'arrête en `STOPPED` avec un message d'erreur précis.

---

## Voir aussi

- [Agents Quickstart](./Agents-Quickstart) — démarrer en 5 minutes
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — référence complète des services
- [Architecture Vue d'ensemble](./Architecture-Vue-Ensemble) — AIP dans le contexte global
- [ADR-003](../adr/ADR-003-duck-typing-aip) — pourquoi duck typing plutôt que classe de base
- [ADR-014](../adr/ADR-014-bridge-spawn-blocking-asyncio-run) — bridge async Rust → Python
