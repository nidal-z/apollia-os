# Agents — RuntimeContext Guide — Apollia OS

> Référence complète de tous les services disponibles via `ctx` dans la méthode `run()`.
> Public cible : développeur d'agent Python intermédiaire

---

## Vue d'ensemble

Le `RuntimeContext` (accessible via le paramètre `ctx` dans `run()`) est l'interface entre votre agent et tous les services du runtime. Il est injecté par Apollia OS à chaque appel de tâche — vous n'avez jamais à l'instancier.

```python
async def run(self, task, ctx):
    # ctx donne accès à :
    # ctx.tools         — ToolProxy : invocation des outils
    # ctx.memory        — MemoryInterface | None : mémoire persistante
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

#### file_io

Lecture/écriture de fichiers avec protection path traversal.

```python
# Lister des fichiers
result = await ctx.tools.call("file_io", {
    "action": "list",
    "path": ".",
    "pattern": "*.py",   # optionnel, glob
})
# result : {"files": ["hello_agent.py", "devis_agent.py"]}

# Lire un fichier
result = await ctx.tools.call("file_io", {
    "action": "read",
    "path": "./data/config.json",
})
# result : {"content": "...", "size": 1234}

# Écrire un fichier
result = await ctx.tools.call("file_io", {
    "action": "write",
    "path": "./output/rapport.txt",
    "content": "Contenu du rapport...",
})
# result : {"written": 42}
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
# ["bash_executor", "file_io"]
```

### Compter les appels

```python
count = ctx.tools.tool_call_count()
# Utile pour adapter le comportement proche de la limite StepBudget
```

---

## ctx.memory — MemoryInterface

**Disponible uniquement si `memory_namespace` est défini dans le manifest.** `None` sinon.

Le Memory Engine distingue trois types de mémoire : épisodique (événements), sémantique (faits), procédurale (procédures). `ctx.memory` expose une interface unifiée.

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

## ctx.log — AgentLogger

Logs structurés envoyés via le système de logging du runtime (`tracing`).

```python
ctx.log.info("step_started", step=1, tool="file_io")
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

- [Briques AIP Specification](./Briques-AIP-Specification) — contrat complet AIPTask, AIPResult, AgentManifest
- [Briques Tool Registry](./Briques-Tool-Registry) — catalogue des outils, schémas complets
- [Briques Memory Engine](./Briques-Memory-Engine) — backends mémoire, FTS5, namespaces
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — gestion du StepBudget, coûts LLM
