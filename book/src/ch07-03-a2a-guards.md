# A2A Guards

Dans une architecture multi-agents, un Director Agent peut déléguer une tâche à un Worker via `ctx.delegate()`. Ce mécanisme est puissant — mais il ouvre un vecteur de risque nouveau : **la récursivité entre agents**.

Agent A invoque Agent B. Agent B invoque Agent C. Agent C invoque Agent A. La chaîne tourne indéfiniment, consommant des ressources jusqu'à épuisement. Ou pire : Agent A s'invoque lui-même, créant une boucle infinie d'auto-délégation.

Les **A2A Guards** sont les trois garde-fous non contournables qui protègent les chaînes d'invocations inter-agents.

---

## Les trois protections

| Garde-fou | Défaut | Protection contre |
|---|---|---|
| `max_depth` | 3 | Récursivité infinie (`A → B → C → A → ...`) |
| `chain_timeout_secs` | 300 (5 min) | Chaîne monopolisant les ressources indéfiniment |
| Self-invocation | Bloqué | Agent qui s'invoque lui-même via A2A |

Ces protections s'appliquent automatiquement à chaque `ctx.delegate()`. L'agent Python ne peut pas les désactiver.

---

## Configuration dans `apollia.toml`

```toml
[a2a]
max_depth = 3                  # Profondeur maximale de la chaîne
invocation_timeout_secs = 120  # Timeout par invocation individuelle
chain_timeout_secs = 300       # Budget cumulé pour toute la chaîne
```

Le `chain_timeout_secs` est le budget total de la chaîne entière — pas par invocation. Si une chaîne `A → B → C` prend 280 secondes sur les 300 allouées, l'invocation suivante de C a seulement 20 secondes disponibles.

---

## Ordre d'application des garde-fous

À chaque `ctx.delegate()`, le runtime vérifie dans cet ordre :

```
1. max_depth atteint ?      → MaxDepthExceeded
2. chain_deadline expirée ? → ChainTimeoutExceeded
3. caller == target ?       → SelfInvocation
4. Skill résolu             → invocation normale
```

La vérification est effectuée en Rust avant que l'agent Worker soit instancié — pas de coût d'exécution inutile en cas de refus.

---

## Erreurs retournées

Quand un garde-fou se déclenche, l'invocation retourne immédiatement une erreur structurée `A2AError` :

```rust
// apollia-runtime/src/a2a/invoker.rs
pub enum A2AError {
    MaxDepthExceeded {
        current_depth: u32,
        max_depth: u32,
        caller: String,
        skill_id: String,
    },
    SelfInvocation {
        agent_name: String,
        skill_id: String,
    },
    ChainTimeoutExceeded {
        caller: String,
        skill_id: String,
    },
}
```

Un événement `RuntimeEvent::A2AGuardTriggered` est simultanément émis sur l'EventBus :

```rust
// apollia-core/src/events.rs
A2AGuardTriggered {
    guard_type: String,  // "max_depth" | "self_invocation" | "chain_timeout"
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
        if code == "A2A_MAX_DEPTH_EXCEEDED":
            # Traiter localement plutôt que de déléguer
            return await self._process_locally(task["data"], ctx)
        elif code == "A2A_CHAIN_TIMEOUT":
            return AIPResult.failed("TIMEOUT",
                                    "Chaîne A2A expirée avant la fin du traitement")
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
  │               Worker exécute...      (chain_timeout s'écoule)
  └── Step 3 : traitement du résultat    (-1 step)
```

Si le budget du Director est épuisé pendant qu'un Worker exécute, la tâche du Director échoue avec `BUDGET_EXCEEDED` — le Worker est interrompu par le timeout A2A (`invocation_timeout_secs`).

**Règle pratique** : pour chaque `ctx.delegate()` dans votre agent, comptez au minimum 2 steps (avant et après la délégation) et ajustez `max_steps` dans votre manifest en conséquence.

---

## Visualiser la chaîne A2A

Le diagramme de séquence des A2A Guards est disponible dans `docs/diagrams/seq-a2a-guards.puml`. Il illustre le flux complet d'une invocation `A → B → C` avec déclenchement de `MaxDepthExceeded` à la profondeur 4.

```bash
# Observer les événements A2A en temps réel
apollia-os audit stats --filter a2a
#  GARDE-FOU            DÉCLENCHEMENTS   DERNIER
#  max_depth            2                14:32:01
#  self_invocation      0                —
#  chain_timeout        1                09:17:44
```
