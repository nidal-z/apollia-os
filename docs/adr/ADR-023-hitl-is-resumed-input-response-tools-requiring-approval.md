# ADR-023 - HITL : re-appel `agent.run()` avec `AIPTask.is_resumed` + `InputResponse`, déclaration `tools_requiring_approval` dans le manifest

**Date :** 2026-03-09
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 11

---

## Contexte

Sprint 11 introduit le Human-in-the-Loop (HITL) : un agent peut suspendre une tâche pour demander une approbation humaine avant de continuer. Deux décisions de design interdépendantes doivent être prises :

1. **Mécanisme de reprise** : comment le runtime communique-t-il la réponse humaine (approuvé/rejeté + raison) à l'agent Python lors de la relance ?
2. **Déclaration des outils sensibles** : où l'agent déclare-t-il les outils qui nécessitent approbation avant exécution (Mode Orchestré) ?

Ces décisions engagent le contrat AIP (ADR-003) et l'interface Python publique. Elles sont difficiles à inverser une fois que des agents tiers les adoptent.

**Contraintes :**
- Principe #3 (contrat minimal) : ajouter le moins possible au contrat `manifest()` + `run()`.
- Principe #4 (fail fast) : un agent mal implémenté (ignore `is_resumed`, lit un mauvais champ) doit produire un échec explicite, pas un comportement silencieux.
- ADR-003 (duck typing) : pas de classe de base obligatoire, validation par `hasattr`.
- ADR-022 (Mode Orchestré Option B) : l'`ActorLoop` exécute les outils directement. Le HITL Mode Orchestré suspend l'`ActorLoop` avant l'exécution d'un step.

---

## Décision

### Mécanisme de reprise - Option 1 (retenue)

Nous réutilisons `agent.run()` comme unique point d'entrée en ajoutant deux champs à `AIPTask` :

```python
@dataclass
class InputResponse:
    approved:     bool
    reason:       str | None          # raison si rejected
    context:      dict                # état sérialisé par l'agent au moment du suspend
    responded_at: datetime

@dataclass
class AIPTask:
    # ...champs existants...
    is_resumed:     bool              = False
    input_response: InputResponse | None = None
```

L'agent déclare `AIPResult.input_required(prompt, context)` pour suspendre. Lors de la relance, ORIA reconstruit un `AIPTask` avec `is_resumed=True` et `input_response` peuplé, puis rappelle `agent.run()`.

### Déclaration des outils sensibles

`tools_requiring_approval` est un champ optionnel de `AgentManifest` :

```python
@dataclass
class AgentManifest:
    # ...champs existants...
    tools_requiring_approval: list[str] = field(default_factory=list)
```

L'`ActorLoop` vérifie ce champ avant chaque step en Mode Orchestré. Si le tool du step figure dans la liste, l'`ActorLoop` suspend et attend l'approbation humaine avant d'exécuter l'outil.

---

## Alternatives considérées

### Option 2 - Nouveau hook optionnel `on_resume(response, ctx)` (rejetée)

**Architecture :** Quand la tâche reprend, ORIA détecte `on_resume` via `hasattr` et l'appelle à la place de `run()`. Si le hook est absent, comportement par défaut (ORIA marque la tâche comme complétée ou échouée selon `approved`).

**Pour :**
- Séparation nette entre logique de premier appel (`run()`) et logique de reprise (`on_resume()`). Un agent simple peut implémenter `run()` sans penser à HITL.
- Cohérence avec le pattern `on_plan_complete()` introduit dans ADR-022.

**Contre :**
- Ajoute une quatrième méthode au contrat AIP (après `manifest()`, `run()`, `on_plan_complete()`). Friction à l'adoption et à la documentation.
- Comportement par défaut ambigu si le hook est absent : valider sans logique métier peut ne pas être correct pour tous les agents.
- Deux chemins d'exécution distincts dans l'`AIPBridge` - duplication du `spawn_blocking` + `asyncio.run()` (ADR-014) pour chaque méthode.
- Un agent qui veut la même logique dans les deux cas (ex. : toujours écrire un log) doit implémenter les deux méthodes.

### Option 3 - Réponse stockée dans `MemoryManager`, agent lit via `ctx.memory` (rejetée)

**Architecture :** Lors de la reprise, ORIA écrit `InputResponse` dans la mémoire épisodique de l'agent. `run()` est rappelé normalement (sans `is_resumed`). L'agent lit la réponse depuis `ctx.memory.recall("hitl_response")`.

**Pour :**
- Aucune modification du contrat `AIPTask` - `run()` reçoit exactement la même structure qu'un premier appel.
- L'agent peut stocker et interroger l'historique complet de ses suspensions.

**Contre :**
- Injection automatique dans la mémoire sans action explicite de l'agent. Viole l'esprit du Principe #6 (mémoire à initiative de l'agent), même si la lecture reste explicite.
- Couplage fort HITL ↔ `apollia-memory` : une tâche sans mémoire configurée ne peut pas fonctionner en HITL.
- L'agent ne sait pas si c'est une reprise ou un premier appel sans lire la mémoire - logique plus opaque.
- La clé mémoire `"hitl_response"` est un couplage implicite entre le runtime et les agents. Fragile si la clé change.

### Option 1 (retenue) - Re-appel de `agent.run()` avec `AIPTask.is_resumed` + `InputResponse`

**Pour :**
- Contrat AIP minimal : un seul point d'entrée Python (`run()`), deux champs ajoutés à `AIPTask` (type de données existant).
- Le pattern `if task.is_resumed` est idiomatique et explicite - un agent qui l'ignore produira une logique incorrecte facilement détectable à l'exécution ou au test.
- Cohérent avec la philosophie AIPTask comme "snapshot d'état complet au moment de l'exécution" : `is_resumed` + `input_response` font partie de l'état.
- Zéro dépendance sur `apollia-memory` pour le mécanisme HITL de base.
- `AIPBridge` réutilise `call_run()` (ADR-014, `spawn_blocking` + `asyncio.run()`) sans nouveau chemin d'exécution.

**Compromis acceptés :**
- Un agent qui n'implémente pas `if task.is_resumed` sera rappelé avec la même logique qu'un premier appel - comportement potentiellement indésirable. Documenté clairement dans le guide agent.
- `AIPTask` gagne deux champs optionnels (`is_resumed`, `input_response`). Le type de données s'alourdit légèrement.
- `InputResponse.context` est un `dict` - l'agent est responsable de la sérialisation/désérialisation de son état.

---

## Conséquences

**Positives :**
- `agent.run()` reste le seul point d'entrée Python pour les tâches - le contrat AIP ne croît que par les champs `AIPTask`.
- `call_run()` dans `AIPBridge` est réutilisé sans modification pour la reprise - passe simplement l'`AIPTask` enrichi.
- `tools_requiring_approval` est déclaratif : l'`ActorLoop` peut inspecter le manifest avant de générer le plan et afficher les étapes marquées `[approbation requise]` dans le plan arborescent CLI.
- `InputResponse.context` persiste dans SQLite (`task_approvals.context_json`) - l'état de l'agent au moment de la suspension est auditable.

**Négatives / Compromis :**
- Les agents HITL doivent implémenter le pattern `if task.is_resumed` - charge documentaire supplémentaire.
- `AIPTask` est mutable entre le premier appel et la reprise - une sérialisation SQLite complète est requise pour que `rebuild_for_resume()` reconstruise fidèlement l'état.
- Un agent sans logique de reprise sera silencieusement incorrect si HITL est déclenché. Fail fast partiel : la tâche complètera, mais potentiellement avec un résultat incorrect.

**Neutres / À surveiller :**
- `InputResponse.context` est sérialisé/désérialisé en JSON : surveiller les pertes de précision sur les types Python non-JSON-natifs (datetime, Decimal).
- Un agent peut avoir plusieurs suspensions (multi-approval) : chaque `task_approvals` enregistre la suspension ; `AIPTask.input_response` ne contient que la dernière réponse au moment de la relance.
- `tools_requiring_approval` fonctionne uniquement en Mode Orchestré (l'`ActorLoop` en est responsable). En Mode Direct, c'est `AIPResult.input_required()` que l'agent appelle explicitement - les deux mécanismes sont orthogonaux.

---

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : `manifest()` + `run()` restent suffisants. `tools_requiring_approval` est un champ optionnel du manifest (défaut : liste vide). `is_resumed` + `input_response` étendent `AIPTask` sans changer la signature de `run()`.
- **Principe #4 - Fail fast** : Transition `input_required → working` vérifiée côté `ResumeHandler` avant relance. Une tâche non en `input_required` répond `409 CONFLICT` à `POST /api/v1/tasks/{id}/resume`.
- **Principe #6 - Mémoire à initiative de l'agent** : `InputResponse` est transmis dans `AIPTask` (contrat AIP), pas injecté dans `apollia-memory`. L'agent reste maître de ce qu'il mémorise.
- **Principe #7 - Garde-fous non-négociables** : `TimeoutWatcher` annule automatiquement les tâches `input_required` après `input_required_timeout_hours` - l'agent ne peut pas désactiver ce comportement.

---

## Liens

- Stories associées : STORY-092, STORY-093, STORY-094, STORY-095, STORY-096, STORY-097, STORY-098, STORY-103, STORY-106 (Sprint 11)
- ADR précédents liés :
  - ADR-003 - Duck typing AIP : `tools_requiring_approval` suit la même philosophie déclarative via manifest
  - ADR-014 - `spawn_blocking` + `asyncio.run()` : `call_run()` réutilisé pour la reprise
  - ADR-022 - Mode Orchestré Option B : l'`ActorLoop` est le point d'interception pour `tools_requiring_approval`
