# Sécurité — Guardrails — Apollia OS

> StepBudget, ResilienceLayer et circuit breakers : les garde-fous non contournables appliqués par le runtime.
> Public cible : développeur d'agent, opérateur

---

## Vue d'ensemble

Les guardrails d'Apollia OS protègent contre les deux causes de mort les plus communes des agents en production : les boucles infinies et les coûts LLM incontrôlés. Ils sont appliqués par le runtime Rust — un agent Python ne peut pas les désactiver depuis son code.

---

## StepBudget — tri-dimensionnel

Le StepBudget est appliqué par l'`ExecutionCoordinator` (Rust), pas par l'agent Python.

### Les trois dimensions

| Dimension | Défaut runtime | Override agent | Protection contre |
|---|---|---|---|
| `max_steps` | 10 | Oui, via manifest | Boucles infinies d'étapes de raisonnement |
| `max_tool_calls` | 20 | Oui, via manifest | Spam d'appels d'outils |
| `wall_clock_timeout_secs` | 300 (5 min) | Oui, via manifest | Tâches bloquées indéfiniment |

### Comportement quand le budget est épuisé

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

### from_capped — le runtime plafonne toujours

L'agent peut demander plus que les défauts, mais le runtime plafonne :

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

Si le runtime est configuré avec un plafond de 50 steps, un agent qui déclare `max_steps: 100` obtient en réalité 50.

### Lire le budget depuis l'agent

```python
async def run(self, task, ctx):
    # Adapter le comportement proactivement
    if ctx.step_budget.steps_remaining < 3:
        return self._summarize_and_stop(task)

    # Après chaque étape significative
    ctx.log.info("step_completed",
                 steps_used=10 - ctx.step_budget.steps_remaining,
                 tool_calls=ctx.tools.tool_call_count())
```

---

## ResilienceLayer — circuit breakers par outil

Empêche qu'un outil défaillant ne bloque toutes les tâches de l'agent.

### États du circuit

```
CLOSED (normal)
    │ failure_threshold atteint
    ▼
OPEN (circuit coupé — outil bloqué)
    │ cooldown_period écoulé
    ▼
HALF_OPEN (test — une tentative autorisée)
    │ succès → CLOSED
    │ échec → OPEN (reset cooldown)
```

### Configuration par défaut

| Paramètre | Valeur |
|---|---|
| Seuil d'échec | 5 échecs consécutifs |
| Période de cooldown | 30 secondes |

### Comportement observé

Quand un circuit est ouvert, les appels à cet outil retournent immédiatement `CircuitOpen` sans tenter l'exécution. La tâche peut choisir de continuer avec d'autres outils ou de retourner une erreur.

```bash
# Voir l'état des circuits depuis les logs
RUST_LOG=apollia_oria=debug apollia-os start --foreground 2>&1 | grep "circuit"

# L'événement ToolCircuitRestored est émis quand le circuit se referme
apollia-os audit stats
# bash_executor  142 calls  94.4% success
# [circuit opened for 30s at 14:32:01, restored at 14:32:31]
```

---

## RetryPolicy — backoff exponentiel avec jitter

Pour les erreurs transitoires, l'ORIA Engine réessaie automatiquement avec backoff.

### Comportement

```
Tentative 1 : immédiate
Tentative 2 : base_delay = 100ms
Tentative 3 : base_delay × 2 = 200ms  (±25% jitter)
Tentative 4 : base_delay × 4 = 400ms  (±25% jitter)
...
Maximum : max_delay (défaut: 5000ms)
```

Le jitter (±25%) évite les tempêtes de retry synchronisées quand plusieurs agents réessaient en même temps.

### Erreurs qui déclenchent un retry

Seules les erreurs classifiées `Transient` déclenchent un retry :

| Classe d'erreur | Retry | Exemples |
|---|---|---|
| `Transient` | Oui | Timeout réseau, service temporairement indisponible |
| `Permanent` | Non | Outil non trouvé, permissions refusées |
| `BudgetExceeded` | Non | StepBudget épuisé |
| `SandboxViolation` | Non | Tentative de sortie du sandbox |

---

## Interaction StepBudget × ResilienceLayer

Le StepBudget s'applique en parallèle du ResilienceLayer :

1. `working` : chaque step, chaque tool call décrémente les compteurs
2. Si un circuit est ouvert, l'appel échoue immédiatement (compte comme 1 tool call)
3. Un retry (RetryPolicy) consomme des tool calls supplémentaires
4. Si le budget est épuisé pendant un retry, la tâche échoue avec `BUDGET_EXCEEDED`

**Conseil :** pour les agents qui font beaucoup d'appels d'outils, déclarer un `step_budget.max_tool_calls` réaliste dans le manifest :

```python
def manifest(self):
    return {
        "step_budget": {
            "max_steps": 20,
            "max_tool_calls": 50,  # inclure les retries éventuels
            "wall_clock_timeout_secs": 300
        }
    }
```

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) — Principe #7 Garde-fous non négociables
- [Briques ORIA Engine](./Briques-ORIA-Engine) — implémentation StepBudget et ResilienceLayer
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — comment anticiper le budget dans le code agent
