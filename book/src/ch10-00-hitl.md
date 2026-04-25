# Human-in-the-Loop

L'automatisation a ses limites. Un agent qui génère un devis de 50 000 € et l'envoie automatiquement par email — sans que personne n'ait vu le montant — c'est un risque que la plupart des opérateurs ne souhaitent pas prendre. Un agent qui efface des fichiers en production sur commande de l'utilisateur mérite une confirmation avant d'agir.

Le **HITL (Human-in-the-Loop)** est le mécanisme d'Apollia OS qui permet à un agent de **suspendre son exécution** et d'attendre une décision humaine avant de continuer. La tâche se fige — sans consommer de steps, sans timeout — jusqu'à ce qu'un opérateur approuve ou rejette l'action.

---

## Deux mécanismes complémentaires

Apollia OS propose deux approches HITL selon le mode d'exécution de l'agent :

### Mode Direct — `AIPResult.input_required`

L'agent Python décide lui-même du moment de la suspension. Dans `run()`, quand il atteint un point de contrôle critique, il retourne `AIPResult.input_required(prompt, context)` au lieu d'un résultat final. ORIA suspendut la tâche et attend une décision via l'API.

```python
# L'agent calcule, puis demande validation
amount = await self._calculate_total(task, ctx)
return AIPResult.input_required(
    prompt=f"Confirmer le devis de {amount} € pour le client ?",
    context={"amount": amount}
)
```

### Mode Orchestré — `tools_requiring_approval`

L'agent déclare dans son manifest quels outils nécessitent une approbation avant exécution. L'ActorLoop d'ORIA intercepte automatiquement les steps qui utilisent ces outils et suspend avant de les exécuter — sans que l'agent Python ait à écrire de logique de suspension.

```python
def manifest(self):
    return AgentManifest(
        name="envoi-devis",
        tools_required=["file_io", "smtp"],
        tools_requiring_approval=["smtp"],  # suspension avant chaque envoi email
    )
```

---

## Quand utiliser chaque mécanisme

| Situation | Mécanisme |
|---|---|
| Validation d'une valeur calculée avant action | Mode Direct — `input_required` |
| Confirmation avant d'utiliser un outil à effet de bord | Mode Orchestré — `tools_requiring_approval` |
| Workflow avec plusieurs points de contrôle explicites | Mode Direct — plusieurs `input_required` |
| Outil toujours risqué quel que soit le contexte | Mode Orchestré — déclaration dans le manifest |
| Logique de reprise complexe (état à restaurer) | Mode Direct — `is_resumed` + `context` |

Les deux mécanismes sont orthogonaux : un agent en mode orchestré peut déclarer `tools_requiring_approval` ET retourner `input_required` dans `on_plan_complete` si besoin.

---

## Garanties du runtime

Pendant la suspension HITL :

- **Le StepBudget ne progresse pas** — l'attente de la décision humaine ne consomme ni steps ni tool_calls
- **L'état est persisté en SQLite** — un redémarrage du runtime pendant la suspension ne perd pas la tâche suspendue
- **Le TimeoutWatcher annule automatiquement** les tâches suspendues après 24h (configurable) — aucune tâche ne reste orpheline indéfiniment

---

## Ce que vous allez apprendre

- **Section 1 — Suspend-Resume** : le pattern `input_required` en Mode Direct, `is_resumed` et `InputResponse`, le flow ORIA complet, le TimeoutWatcher
- **Section 2 — Tool Approval** : `tools_requiring_approval` dans le manifest, la mécanique Mode Orchestré, différences avec le Mode Direct
- **Section 3 — Notifications** : le NotificationEngine, les canaux desktop et webhook, comment être alerté quand une tâche attend une décision
