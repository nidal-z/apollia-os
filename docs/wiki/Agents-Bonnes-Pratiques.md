# Agents — Bonnes Pratiques — Apollia OS

> Patterns recommandés pour écrire des agents robustes, économiques et diagnostiquables en production.
> Public cible : développeur d'agent Python avancé

---

## StepBudget — anticiper avant d'être arrêté

Le runtime applique un StepBudget non contournable (Principe #7). Un agent qui ignore le budget restant se fait couper en pleine exécution avec une tâche `failed`.

### Vérifier avant chaque itération

```python
async def run(self, task, ctx):
    for step in range(100):  # boucle potentiellement longue
        # Vérifier AVANT d'appeler un LLM ou un outil
        if ctx.step_budget.steps_remaining < 2:
            # Retourner un résultat partiel plutôt qu'une erreur
            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": f"Résultat partiel après {step} étapes"}],
            }

        result = await self._call_llm(...)
        # ...
```

### Adapter le comportement selon le budget

```python
async def run(self, task, ctx):
    budget_ratio = ctx.step_budget.steps_remaining / 20  # 20 = max_steps

    if budget_ratio > 0.5:
        # Budget confortable — stratégie normale
        depth = "deep"
    elif budget_ratio > 0.2:
        # Budget faible — réduire la profondeur
        depth = "shallow"
    else:
        # Budget critique — résumé immédiat
        return self._emergency_summary(task)

    return await self._process(task, ctx, depth=depth)
```

### Déclarer un budget adapté dans le manifest

```python
def manifest(self):
    return {
        "name": "research-agent",
        "step_budget": {
            "max_steps": 30,            # plus que le défaut (10)
            "max_tool_calls": 60,       # plus que le défaut (20)
            "wall_clock_timeout_secs": 600  # 10 min plutôt que 5
        }
    }
```

Le runtime plafonne toujours via `min(agent_budget, runtime_defaults)`. Vous ne pouvez pas demander plus que ce que le runtime autorise.

---

## Mémoire — lire avant d'appeler un LLM

L'appel LLM est cher. La recherche mémoire FTS5 est gratuite. Toujours chercher en mémoire avant de générer.

```python
async def run(self, task, ctx):
    user_input = task["input"]["parts"][0]["text"]

    # 1. Chercher en mémoire d'abord
    memory_context = []
    if ctx.memory:
        results = await ctx.memory.search(user_input, limit=5)
        memory_context = [r["content"] for r in results if r["score"] > 0.3]

    # 2. Appeler le LLM avec le contexte récupéré
    prompt = self._build_prompt(user_input, memory_context)
    response = await self._call_llm(prompt)

    # 3. Mémoriser le résultat (importance proportionnelle à la qualité)
    if ctx.memory and response:
        importance = 0.8 if len(response) > 200 else 0.4
        await ctx.memory.record(
            f"Q: {user_input[:80]} → R: {response[:80]}",
            importance=importance,
            task_id=task["task_id"]
        )

    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [{"type": "text", "text": response}],
    }
```

---

## Outils — gérer les échecs explicitement

Ne pas laisser propager les exceptions d'outils sans les traiter.

```python
async def run(self, task, ctx):
    # ❌ Mauvais — une exception outil fait planter la tâche sans contexte
    result = await ctx.tools.call("bash_executor", {"command": "./process.sh"})

    # ✅ Correct — traitement explicite de l'erreur
    try:
        result = await ctx.tools.call("bash_executor", {"command": "./process.sh"})
        stdout = result.get("stdout", "")
        exit_code = result.get("exit_code", -1)

        if exit_code != 0:
            ctx.log.warn("script_failed", exit_code=exit_code, stderr=result.get("stderr", ""))
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "SCRIPT_FAILED",
                    "message": f"Exit code {exit_code}: {result.get('stderr', '')[:200]}"
                }
            }
    except Exception as e:
        ctx.log.error("tool_error", tool="bash_executor", error=str(e))
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {"code": "TOOL_ERROR", "message": str(e)}
        }
```

---

## Concurrence — déclarer `max_concurrent_tasks`

Par défaut `max_concurrent_tasks: 1`. Un seul client ? C'est correct. En production avec plusieurs requêtes simultanées possibles :

```python
def manifest(self):
    return {
        "name": "api-agent",
        "max_concurrent_tasks": 3,  # 3 tâches simultanées max
    }
```

**Attention :** si votre agent maintient un état interne entre les appels, `max_concurrent_tasks: 1` est obligatoire. Si `run()` est stateless (ne modifie aucun attribut de `self`), vous pouvez augmenter cette valeur.

---

## Logging — utiliser ctx.log, pas print

```python
# ❌ Éviter — perdu dans stdout, pas de contexte agent/tâche
print(f"Processing step {step}")

# ✅ Correct — structuré, corrélé avec task_id automatiquement
ctx.log.info("processing_step", step=step, remaining=ctx.step_budget.steps_remaining)
```

Le runtime ajoute automatiquement `agent_id` et `task_id` à chaque log. Ils apparaissent dans les logs du runtime filtrables par `RUST_LOG=apollia_runtime=debug`.

---

## on_start et on_stop — initialisation coûteuse

Si votre agent charge un modèle, établit une connexion ou initialise un état, faites-le dans `on_start()` :

```python
class HeavyAgent:
    def __init__(self):
        self._model = None  # pas initialisé ici

    async def on_start(self, ctx):
        # Appelé une fois, quand l'agent passe à ACTIVE
        ctx.log.info("loading_model")
        self._model = await load_model("./models/llm.gguf")
        ctx.log.info("model_loaded")

    async def run(self, task, ctx):
        # self._model est déjà chargé
        result = await self._model.generate(...)
        ...

    async def on_stop(self):
        # Libération propre des ressources
        if self._model:
            await self._model.close()
            self._model = None
```

---

## Erreurs connues à éviter

**Boucle infinie sans vérification du budget :**

```python
# ❌ — le runtime coupe avec TaskFailed BudgetExceeded
while not self._is_done():
    result = await self._call_llm(...)
    self._process(result)

# ✅ — l'agent conclut proprement
while not self._is_done():
    if ctx.step_budget.steps_remaining < 1:
        return self._partial_result(task)
    result = await self._call_llm(...)
    self._process(result)
```

**Exception non catchée dans run() :**

Une exception Python non catchée dans `run()` fait passer la tâche en `failed` avec un message générique. Toujours catcher les exceptions attendues et retourner un `AIPResult` explicite.

**Appel d'outil non déclaré dans tools_required/tools_optional :**

```python
# manifest déclare tools_required: ["file_io"]
# ❌ — ToolNotAllowed exception
result = await ctx.tools.call("bash_executor", {...})
```

---

## Voir aussi

- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — référence ctx.tools, ctx.memory, ctx.step_budget
- [Briques ORIA Engine](./Briques-ORIA-Engine) — StepBudget et ResilienceLayer détaillés
- [Briques AIP Specification](./Briques-AIP-Specification) — tous les champs manifest
