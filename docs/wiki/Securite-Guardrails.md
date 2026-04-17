# Sécurité — Guardrails — Apollia OS

> StepBudget, ResilienceLayer et circuit breakers : les garde-fous non contournables appliqués par le runtime.
> Public cible : développeur d'agent, opérateur

---

## Vue d'ensemble

Les guardrails d'Apollia OS protègent contre les risques les plus fréquemment observés dans les déploiements d'agents IA : les boucles infinies et les coûts LLM incontrôlés. Ils sont appliqués par le runtime Rust — un agent Python ne peut pas les désactiver depuis son code.

---

## StepBudget — tri-dimensionnel

Le StepBudget est appliqué par l'`ExecutionCoordinator` (Rust), pas par l'agent Python.

### Les trois dimensions

| Dimension | Défaut runtime | Override agent | Protection contre |
|---|---|---|---|
| `max_steps` | 30 | Oui, via manifest | Boucles infinies d'étapes de raisonnement |
| `max_tool_calls` | 60 | Oui, via manifest | Spam d'appels d'outils |
| `wall_clock_timeout_secs` | 600 (10 min) | Oui, via manifest | Tâches bloquées indéfiniment |

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

### Qu'est-ce qu'un "step" ?

Un **step** correspond à un cycle de raisonnement de l'agent. Concrètement :

| Opération | Coût en steps | Coût en tool_calls |
|---|---|---|
| Un appel LLM (raisonnement) | 1 step | 0 |
| Un appel outil (`ctx.tools.*`) | 0 | 1 tool_call |
| Un retry d'outil (RetryPolicy) | 0 | 1 tool_call supplémentaire |

En mode orchestré (ORIA), chaque step du plan d'exécution consomme 1 step + N tool_calls.

### Plafonds runtime (`max_steps_ceiling`)

Le runtime applique un plafond global configurable dans `apollia.toml` :

```toml
[runtime.budget]
max_steps_ceiling = 50
max_tool_calls_ceiling = 100
wall_clock_ceiling_secs = 600
```

Si ces valeurs ne sont pas définies, les défauts runtime (10 steps, 20 tool_calls, 300s) s'appliquent comme plafond.

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
| Seuil d'échec | 3 échecs consécutifs |
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

### Configuration par défaut

| Paramètre | Valeur par défaut |
|---|---|
| `max_attempts` | 3 |
| `base_delay_ms` | 500 ms |
| `max_delay_ms` | 10 000 ms (10s) |
| `jitter` | ±25% |

### Comportement

```
Tentative 1 : immédiate
Tentative 2 : 500ms  (±25% jitter)
Tentative 3 : 1000ms (±25% jitter)
...
Maximum : 10 000ms (cap)
```

Formule : `min(base_delay_ms × 2^(attempt-1), max_delay_ms)`, avec jitter optionnel de ±25%.

Le jitter évite les tempêtes de retry synchronisées quand plusieurs agents réessaient en même temps.

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

## Outils dangereux — `dangerous_tools_allowed`

Les outils avec `dangerous: true` dans leur `ToolDescriptor` (ex: `bash_executor` avec profil `SandboxProfile::Full`) nécessitent un opt-in explicite de l'agent via `dangerous_tools_allowed: True` dans le manifest. Sans ce flag, l'appel retourne `ToolAccessDenied`.

Ce mécanisme garantit qu'un agent ne peut pas accidentellement accéder à des outils à haut risque. Voir [Bonnes Pratiques — Outils dangereux](./Agents-Bonnes-Pratiques) pour les détails d'implémentation.

---

## Garde-fous A2A — protection des chaînes d'invocations inter-agents (Sprint 32)

Quand un Director Agent invoque un Worker via A2A (`ctx.delegate(skill_id, ...)`), le runtime applique trois garde-fous automatiques non contournables depuis Python.

### Les trois protections

| Garde-fou | Défaut | Protection contre |
|---|---|---|
| `max_depth` | 3 | Récursivité infinie (A invoque B qui invoque A...) |
| `chain_timeout_secs` | 300 (5 min) | Chaîne A2A monopolisant les ressources indéfiniment |
| Self-invocation | Bloqué | Agent qui s'invoque lui-même via A2A |

### Configuration dans `apollia.toml`

```toml
[a2a]
max_depth = 3                 # Profondeur maximale de la chaîne A2A
invocation_timeout_secs = 120 # Timeout par invocation individuelle
chain_timeout_secs = 300      # Budget cumulé pour toute la chaîne
```

### Comportement quand un garde-fou se déclenche

L'invocation retourne immédiatement une erreur structurée `A2AError` et un événement `RuntimeEvent::A2AGuardTriggered` est émis sur l'EventBus :

```rust
// apollia-runtime/src/a2a/invoker.rs
pub enum A2AError {
    // ...
    MaxDepthExceeded { current_depth: u32, max_depth: u32, caller: String, skill_id: String },
    SelfInvocation { agent_name: String, skill_id: String },
    ChainTimeoutExceeded { caller: String, skill_id: String },
}
```

```rust
// apollia-core/src/events.rs
A2AGuardTriggered {
    guard_type: String,  // "max_depth" | "self_invocation" | "chain_timeout"
    caller: String,
    skill_id: String,
    detail: String,
},
```

### Ordre d'application des garde-fous

```
1. max_depth → MaxDepthExceeded
2. chain_deadline expirée → ChainTimeoutExceeded
3. caller == target → SelfInvocation
4. Skill résolu → invocation normale
```

Le `chain_deadline` est initialisé lors de la première invocation A2A à `Instant::now() + chain_timeout_secs` et propagé dans toute la chaîne. Le timeout effectif par invocation est `min(invocation_timeout, chain_remaining)`.

### Interaction avec StepBudget

Le StepBudget du Director Agent continue de s'appliquer pendant l'invocation A2A. Si le budget du Director est épuisé pendant qu'un Worker exécute, la tâche du Director échoue avec `BUDGET_EXCEEDED` — le Worker est interrompu par le timeout A2A.

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) — Principe #7 Garde-fous non négociables
- [Briques ORIA Engine](./Briques-ORIA-Engine) — implémentation StepBudget et ResilienceLayer
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — comment anticiper le budget dans le code agent
- [A2A / ACP](./A2A-ACP-Alignement) — routing A2A, trust model, A2AToolsProvider
