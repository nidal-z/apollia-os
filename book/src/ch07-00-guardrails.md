# Les garde-fous

Dans le chapitre précédent, `ctx.llm.run_tools` a rendu votre agent autonome : le modèle décide lui-même quels outils appeler, combien de fois, et dans quel ordre. C'est précisément ce qui le rend puissant — et c'est précisément ce qui le rend dangereux sans limites.

Un agent autonome sans contraintes peut boucler indéfiniment sur une tâche mal formée, accumuler des centaines d'appels LLM en quelques minutes, ou, dans une architecture multi-agents, déclencher une cascade d'invocations récursives qui monopolise toutes les ressources du runtime.

Les **garde-fous** d'Apollia OS sont la réponse à ces risques. Ils sont appliqués par le runtime Rust — un agent Python ne peut pas les désactiver depuis son code, même s'il le tente.

---

## Trois couches de protection

Apollia OS protège vos agents avec trois mécanismes indépendants, complémentaires, actifs par défaut.

### StepBudget — le budget d'exécution

Le StepBudget est appliqué par l'`ExecutionCoordinator`. Il limite l'agent sur trois dimensions simultanées : nombre de cycles de raisonnement (`max_steps`), nombre d'appels d'outils (`max_tool_calls`), et durée totale d'exécution (`wall_clock_timeout_secs`).

Quand le budget est épuisé, la tâche passe immédiatement en `failed` — pas de graceful degradation, pas de tentative de finir. La protection est absolue.

### ResilienceLayer — les circuit breakers

Si un outil échoue trois fois de suite, le circuit s'ouvre : pendant 30 secondes, tout appel à cet outil retourne immédiatement `CircuitOpen` sans même tenter l'exécution. Cela protège le reste du runtime d'un outil défaillant ou d'un service tiers indisponible.

Le circuit se referme automatiquement après la période de cooldown.

### A2A Guards — les garde-fous inter-agents

Quand un Director Agent invoque un Worker via `ctx.delegate`, deux garde-fous supplémentaires s'appliquent : nombre maximal de hops dans la chaîne (`max_hops`) et détection de cycles (y compris l'auto-invocation). Ces protections empêchent les récursions infinies entre agents.

---

## Principe #7 en action

Ces garde-fous sont la mise en œuvre directe du principe non-négociable #7 d'Apollia OS :

> **Garde-fous non-négociables** : StepBudget appliqué par le runtime, non contournable.

L'agent déclare un budget dans son manifest — le runtime l'applique, en le plafonnant si nécessaire. L'agent peut observer son budget restant et adapter son comportement proactivement. Mais il ne peut pas lever la limite.

```python
async def run(self, task, ctx):
    # Observer le budget proactivement
    if ctx.step_budget.steps_remaining < 3:
        return self._summarize_and_stop(task)

    # Continuer normalement
    result = await ctx.llm.run_tools(...)
    return AIPResult.completed(result)
```

---

## Ce que vous allez apprendre

- **Section 1 — StepBudget** : les trois dimensions, la configuration dans le manifest, les plafonds runtime, comment lire le budget depuis l'agent
- **Section 2 — ResilienceLayer** : la machine à états du circuit breaker, la RetryPolicy avec backoff exponentiel, l'observabilité via les logs et l'audit
- **Section 3 — A2A Guards** : les deux protections pour les chaînes inter-agents (max_hops + détection de cycles), la configuration dans `apollia.toml`, l'interaction avec le StepBudget
