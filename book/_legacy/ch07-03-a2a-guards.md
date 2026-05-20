# A2A Guards

Dans une architecture multi-agents, un Director Agent peut déléguer une tâche à un Worker via `ctx.delegate`. Ce mécanisme est puissant — mais il ouvre un vecteur de risque nouveau : **la récursivité entre agents**.

Agent A invoque Agent B. Agent B invoque Agent C. Agent C invoque Agent A. La chaîne tourne indéfiniment, consommant des ressources jusqu'à épuisement. Ou pire : Agent A s'invoque lui-même, créant une boucle infinie d'auto-délégation.

Les **A2A Guards** sont les deux garde-fous non contournables qui protègent les chaînes d'invocations inter-agents.

---

## Les deux protections

| Garde-fou | Défaut | Protection contre |
|---|---|---|
| `max_hops` | 5 | Récursivité infinie (`A → B → C → A →...`) |
| Self-invocation | Bloqué | Agent qui s'invoque lui-même via A2A |

Ces protections s'appliquent automatiquement à chaque `ctx.delegate`. L'agent Python ne peut pas les désactiver.

---

## Configuration dans `apollia.toml`

```toml
[a2a]
max_hops = 5                   # Nombre maximal de hops dans la chaîne
invocation_timeout_secs = 120  # Timeout par invocation individuelle
```

Le timeout wall-clock sur l'exécution complète d'un agent est géré par le `StepBudget` (`wall_clock_timeout_secs`) côté Python — il n'existe pas de `chain_deadline` ni de `chain_timeout_secs` au niveau A2A.

---

## Algorithme de validation

À chaque `ctx.delegate`, le runtime appelle `validate_chain` dans cet ordre :

```
1. len(delegation_chain) >= max_hops ? → MaxHopsExceeded
2. agent_id cible dans delegation_chain ?  → CycleDetected
3. caller == target ?                      → CycleDetected (auto-invocation)
4. Skill résolu                            → invocation normale
```

La vérification est effectuée en Rust avant que l'agent Worker soit instancié — pas de coût d'exécution inutile en cas de refus.

---

## Erreurs retournées

Quand un garde-fou se déclenche, l'invocation retourne immédiatement une erreur structurée `A2aError` :

```rust
// apollia-runtime/src/a2a/mod.rs
pub enum A2aError {
    MaxHopsExceeded {
        limit: usize,
    },
    CycleDetected {
        agent_id: AgentId,
    },
    // ...
}
```

Un événement `RuntimeEvent::A2AGuardTriggered` est simultanément émis sur l'EventBus :

```rust
// apollia-core/src/events.rs
A2AGuardTriggered {
    guard_type: String,  // "max_hops" | "cycle_detected" | "self_invocation"
    caller: String,
    skill_id: String,
    detail: String,
}
```

---

## Gérer les erreurs A2A depuis Python

```python
async def run(self, task, ctx):
    result = await ctx.delegate("data-processor", {
        "input": task["data"]
    })

    if result.error:
        code = result.error.code
        if code == "A2A_MAX_HOPS_EXCEEDED":
            # Traiter localement plutôt que de déléguer
            return await self._process_locally(task["data"], ctx)
        elif code == "A2A_CYCLE_DETECTED":
            return AIPResult.failed("CYCLE",
                                    "Cycle détecté dans la chaîne de délégation A2A")
        else:
            return AIPResult.failed(code, result.error.message)

    return AIPResult.completed(result.output)
```

---

## Interaction avec le StepBudget

Le StepBudget du Director Agent continue de s'appliquer pendant l'invocation A2A. Les deux contraintes coexistent :

```
Director (budget: 10 steps, 20 tool_calls)
  │
  ├── Step 1 : raisonnement LLM          (-1 step)
  ├── Step 2 : ctx.delegate("worker")    (-1 step, démarre la chaîne A2A)
  │               Worker exécute...
  └── Step 3 : traitement du résultat    (-1 step)
```

Si le budget du Director est épuisé pendant qu'un Worker exécute, la tâche du Director échoue avec `STEP_BUDGET_EXCEEDED` — le Worker est interrompu par le timeout d'invocation (`invocation_timeout_secs`).

**Règle pratique** : pour chaque `ctx.delegate` dans votre agent, comptez au minimum 2 steps (avant et après la délégation) et ajustez `max_steps` dans votre manifest en conséquence.

---

## Visualiser la chaîne A2A

Le diagramme de séquence des A2A Guards est disponible dans `docs/diagrams/seq-a2a-guards.puml`. Il illustre le flux complet d'une invocation `A → B → C` avec validation de la `delegation_chain` et déclenchement de `MaxHopsExceeded` au-delà de `max_hops=5` (ADR-D7).

```bash
# Observer les événements A2A en temps réel
apollia-os audit stats --filter a2a
#  GARDE-FOU            DÉCLENCHEMENTS   DERNIER
#  max_hops             2                14:32:01
#  self_invocation      0                —
```
