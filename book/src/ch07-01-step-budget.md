# StepBudget

Le StepBudget est le premier garde-fou d'Apollia OS. Il est appliqué par l'`ExecutionCoordinator` (Rust) — pas par l'agent Python. L'agent ne peut pas l'ignorer, le contourner, ou le modifier en cours d'exécution.

---

## Les trois dimensions

Un agent est limité simultanément sur trois axes indépendants :

| Dimension | Défaut runtime | Override agent | Protection contre |
|---|---|---|---|
| `max_steps` | 30 | Oui, via manifest | Boucles infinies de raisonnement |
| `max_tool_calls` | 60 | Oui, via manifest | Spam d'appels d'outils |
| `wall_clock_timeout_secs` | 600 (10 min) | Oui, via manifest | Tâches bloquées indéfiniment |

Les trois limites s'appliquent en parallèle. La première atteinte déclenche l'arrêt.

---

## Qu'est-ce qu'un "step" ?

Un **step** correspond à un cycle de raisonnement complet de l'agent — un appel LLM qui produit une décision. Un appel d'outil ne consomme pas de step, mais consomme un `tool_call` :

| Opération | Coût steps | Coût tool_calls |
|---|---|---|
| Un appel LLM (`ctx.llm.chat()` ou itération `run_tools()`) | 1 | 0 |
| Un appel outil (`ctx.tools.*`) | 0 | 1 |
| Un retry d'outil (RetryPolicy) | 0 | 1 supplémentaire |

En mode orchestré (ORIA), chaque étape du plan d'exécution consomme 1 step + N tool_calls selon les actions entreprises.

---

## Déclarer un budget dans le manifest

L'agent déclare ses besoins dans `manifest()`. Le runtime plafonnera ces valeurs si elles dépassent les limites opérateur.

```python
def manifest(self):
    return {
        "name": "research-agent",
        "version": "0.1.0",
        "step_budget": {
            "max_steps": 25,
            "max_tool_calls": 60,      # inclure les retries éventuels
            "wall_clock_timeout_secs": 180
        }
    }
```

Si `step_budget` est absent du manifest, les valeurs par défaut du runtime s'appliquent (30 steps, 60 tool_calls, 600s).

---

## Les plafonds runtime

L'opérateur peut définir des plafonds globaux dans `apollia.toml` qui s'imposent à tous les agents :

```toml
[runtime.budget]
max_steps_ceiling = 50
max_tool_calls_ceiling = 100
wall_clock_ceiling_secs = 600
```

Le runtime applique toujours `min(valeur_agent, plafond_runtime)` :

```rust
// apollia-oria/src/budget.rs
pub fn from_capped(agent: StepBudgetConfig, runtime: StepBudgetConfig) -> StepBudget {
    StepBudget {
        max_steps: agent.max_steps.min(runtime.max_steps_ceiling),
        max_tool_calls: agent.max_tool_calls.min(runtime.max_tool_calls_ceiling),
        wall_clock_timeout: agent.wall_clock_timeout.min(runtime.wall_clock_ceiling),
    }
}
```

Un agent qui déclare `max_steps: 100` avec un plafond runtime à 50 obtient effectivement 50 steps — sans erreur, sans avertissement. C'est voulu : l'agent doit fonctionner correctement quel que soit le budget réel accordé.

---

## Comportement quand le budget est épuisé

La tâche passe immédiatement en `failed` avec `error.code = "BUDGET_EXCEEDED"` :

```json
{
  "task_id": "t-abc123",
  "status": "failed",
  "error": {
    "code": "BUDGET_EXCEEDED",
    "message": "Step budget exhausted: 10/10 steps used"
  }
}
```

Il n'y a pas de mécanisme de graceful shutdown automatique. C'est à l'agent d'anticiper.

---

## Lire le budget depuis l'agent

`ctx.step_budget` expose l'état courant du budget. Utilisez-le pour adapter le comportement de votre agent proactivement, avant d'atteindre la limite :

```python
async def run(self, task, ctx):
    # Vérification défensive au démarrage
    if ctx.step_budget.steps_remaining < 3:
        return AIPResult.failed("BUDGET_TOO_LOW",
                                "Budget insuffisant pour démarrer cette tâche")

    # Boucle principale
    for item in items:
        result = await self._process(item, ctx)

        # Vérification proactive à chaque étape
        remaining = ctx.step_budget.steps_remaining
        used_calls = ctx.tools.tool_call_count()

        ctx.log.info("step_completed",
                     steps_remaining=remaining,
                     tool_calls_used=used_calls)

        if remaining <= 2:
            # Terminer proprement avec les résultats partiels
            return AIPResult.completed({
                "partial": True,
                "processed": processed_count,
                "reason": "budget_exhausted"
            })

    return AIPResult.completed({"partial": False, "processed": len(items)})
```

La règle d'or : **ne jamais assumer que le budget sera suffisant**. Déclarez un budget réaliste dans le manifest, et gérez le cas `steps_remaining < N` dans votre logique.

---

## Outils dangereux et budget

Les outils marqués `dangerous: true` (comme `bash_executor` avec profil `SandboxProfile::Full`) nécessitent un opt-in explicite dans le manifest :

```python
def manifest(self):
    return {
        "name": "sysadmin-agent",
        "dangerous_tools_allowed": True,
        "step_budget": {
            "max_tool_calls": 30  # compte les appels bash
        }
    }
```

Sans `dangerous_tools_allowed: True`, l'appel retourne `ToolAccessDenied` — et compte quand même comme 1 tool_call.
