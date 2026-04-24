# Référence — Mode Orchestré ORIA

> *Spécification des opcodes, conditions de replanification et codes d'erreur du mode d'exécution orchestré.*

---

## Intro

Le **Mode Orchestré** est le mode d'exécution où ORIA (Observer-Reasoner-Actor) pilote entièrement la tâche : le LLM génère un plan `ExecutionPlan`, l'`ActorLoop` l'exécute step par step, avec replanification automatique en cas d'erreur. Contrairement au Mode Direct, `agent.run()` n'est **pas** appelé pendant l'exécution des steps.

---

## 1. Table — Activation du Mode Orchestré

| Paramètre | Type | Valeur | Effet |
|---|---|---|---|
| `execution_mode` | str | `"orchestrated"` | Force le Mode Orchestré, ignore les heuristiques |
| `execution_mode` | str | `"direct"` | Force le Mode Direct (supervisé) |
| `execution_mode` | str | `"auto"` (défaut) | Classification automatique (scoring pondéré) |
| `system_prompt` | str | Non null | Injecté au Reasoner pour guider la planification LLM |
| `system_prompt` | str | Absent/null | ⚠️ Avertissement si `execution_mode="orchestrated"` |

**Classification automatique (scoring pondéré) :**
- `step_budget.max_steps > 15` : +0.30
- `input.parts.len() > 3` : +0.20
- tags contient `"multi-step"` : +0.40
- `tools_required.len() > 4` : +0.20
- input texte > 500 chars : +0.10
- mémoire épisodique > 5 entrées : +0.10
- input contient mots-clés planning : +0.10
- **Seuil d'activation** : score ≥ 0.40 → Mode Orchestré

---

## 2. Table — Primitives du Plan d'Exécution

L'`ExecutionPlan` est généré par le Reasoner et validé par `Reasoner::parse_and_validate()`.

| Primitive | Champ | Type | Description |
|---|---|---|---|
| **ExecutionPlan** | `plan_id` | UUID v4 | Identifiant unique du plan (généré à la création) |
| | `task_id` | str | Identifiant de la tâche associée |
| | `steps` | Vec<PlanStep> | Liste des steps à exécuter |
| **PlanStep** | `step_id` | str | Identifiant unique dans le plan (ex: `"s1"`, `"s2"`) |
| | `description` | str | Description en langage naturel de l'action |
| | `tool_hint` | Option<str> | Outil suggéré (`None` = aucun outil natif) |
| | `depends_on` | Vec<str> | Identifiants des steps prédécesseurs (DAG) |
| | `model_hint` | Option<str> | Backend LLM optionnel (défaut si `None`) |

**Validation d'un plan :**
- ✅ Tous les `step_id` sont uniques
- ✅ Tous les `depends_on` référencent des `step_id` existants
- ✅ Aucune dépendance circulaire
- ✅ Total steps ≤ `StepBudget::max_steps`

---

## 3. Table — Opcodes ORIA (Actions du Planner)

Chaque primitive de l'ORIAEngine exécute une séquence Observer → Reason → Act.

### 3.1 Observer

| Opcode | Rôle | Produit |
|---|---|---|
| **Enrichir contexte** | Reçoit `AIPTask`, enrichit avec mémoire + historique + état | `ContextBundle` |
| **Classifier complexité** | Scoring pondéré (7 facteurs) pour choisir mode | `ExecutionMode` |
| **Snapshot mémoire** | Recherche mémoire pertinente par `memory_namespace` | `MemorySnapshot` |
| **Charger historique** | Récupère 5 dernières tâches du `context_id` | `Vec<TaskSummary>` |

### 3.2 Reasoner

| Opcode | Rôle | Produit |
|---|---|---|
| **Planifier initial** | Appel LLM avec `system_prompt` + contexte | `ExecutionPlan` |
| **Parser et valider** | Valide JSON, structure, dépendances, cycles | `Result<ExecutionPlan, PlanValidationError>` |
| **Retry parsing** | Max 3 tentatives avec injection du message d'erreur | `ExecutionPlan` ou `ReasonerError::PlanParseError` |
| **Replanifier** | Reçoit plan original + step échoué + erreur | `ExecutionPlan` alternatif |

### 3.3 ActorLoop

| Opcode | Rôle | Condition |
|---|---|---|
| **Trier topologiquement** | Ordre garantissant dépendances satisfaites | Avant exécution |
| **Exécuter step** | Appelle outil via `ToolProxyTrait` ou LLM via `LlmRouter` | Par étape |
| **Budget check** | Vérifie `is_exhausted()` avant chaque step | Par étape |
| **Enregistrer mémoire** | Episodique fire-and-forget (importance 0.6, max 200 chars) | Après step complété |
| **Persister SQLite** | `start_step()` / `complete_step()` / `fail_step()` / `complete_plan()` | Temps réel |
| **Émettre événements** | `StepStarted`, `StepCompleted`, `StepFailed`, `PlanReplanning`, `PlanCompleted` | Temps réel |
| **Replanifier si retryable** | Déclenche `Reasoner::replan()` si erreur retryable + count < max | Si step échoue |

---

## 4. Table — Conditions de Replanification

La replanification est **automatique** et bornée à **max 2 replans** (au-delà : `MAX_REPLAN_EXCEEDED`).

| Événement | Retryable ? | Action |
|---|---|---|
| **ToolCallFailed** | ✅ Oui | Déclenche replan (problème transitoire) |
| **LlmCallFailed** | ✅ Oui | Déclenche replan (backend indisponible) |
| **ToolNotFound** | ❌ Non | Échec immédiat (erreur permanente) |
| **NoLlmBackend** | ❌ Non | Échec immédiat (config manquante) |
| **RejectedByUser** | ❌ Non | Arrêt immédiat (HITL Mode Orchestré, Sprint 11) |
| **ApprovalChannelClosed** | ❌ Non | Arrêt immédiat (runtime shutdown) |
| **replan_count ≥ max_replans** | — | `MAX_REPLAN_EXCEEDED` : tâche échoue |

**Flux de replanification :**
```
Step échoue
  ↓
[is_retryable() ?]
  ├─ OUI + replan_count < max_replans
  │   ├─ Émet RuntimeEvent::PlanReplanning
  │   ├─ Reasoner::replan(original_plan, failed_step, error_msg)
  │   └─ ActorLoop::execute_remaining() avec nouveau plan
  │
  └─ NON ou replan_count ≥ max_replans
      └─ AIPResult::failed(reason)
```

---

## 5. Table — Codes d'Erreur ORIA

### 5.1 ORIAError (Engine)

| Variante | Condition | Message |
|---|---|---|
| `BudgetExceeded { reason }` | `StepBudget::is_exhausted() = true` | `"step budget exceeded: {reason}"` |
| `ExecutionFailed(String)` | Défaillance agent | `"agent execution failed: {reason}"` |
| `ObserverError(ObserverError)` | Échec contexte/mémoire | Via `ObserverError` |
| `BridgeError(String)` | Problème bridge AIP Python ↔ Rust | `"bridge error: {msg}"` |
| `NoLlmConfigured` | Aucun LLM disponible pour Mode Orchestré | `"no LLM configured for orchestrated execution"` |
| `PlanFailed(ReasonerError)` | Planification échouée | Via `ReasonerError` |
| `ApprovalChannelClosed` | Oneshot fermé avant réponse humaine | `"approval channel closed before human response"` |

### 5.2 ReasonerError

| Variante | Condition | Message |
|---|---|---|
| `LlmFailed(String)` | Appel LLM échoué (réseau, timeout, indisponible) | `"LLM call failed: {msg}"` |
| `PlanParseError { attempts, reason }` | Parsing JSON invalide après N tentatives (max 3) | `"Plan parse/validation failed after {attempts} attempts: {reason}"` |

### 5.3 PlanValidationError (Reasoner)

| Variante | Condition |
|---|---|
| `InvalidJson(String)` | JSON non valide dans réponse LLM |
| `InvalidStructure(String)` | Structure JSON ≠ `{ "steps": [...] }` attendue |
| `DuplicateStepIds` | Plusieurs steps partagent le même `step_id` |
| `UnknownDependency { step_id, dep }` | `depends_on` référence un `step_id` inexistant |
| `CircularDependency` | Cycle détecté dans le DAG |

### 5.4 StepError (ActorLoop)

| Variante | Retryable | Condition |
|---|---|---|
| `ToolCallFailed(String)` | ✅ | Outil a échoué |
| `LlmCallFailed(String)` | ✅ | Appel LLM échoué |
| `NoLlmBackend` | ❌ | Aucun backend LLM configuré |
| `ToolNotFound(String)` | ❌ | Outil non enregistré dans le registre |
| `RejectedByUser { reason }` | ❌ | Utilisateur a refusé le step (HITL, Sprint 11) |
| `ApprovalChannelClosed` | ❌ | Oneshot fermé avant réponse |

---

## 6. Table — Paramètres de Configuration

### 6.1 StepBudgetConfig (par agent)

| Paramètre | Type | Défaut | Portée |
|---|---|---|---|
| `max_steps` | u32 | 20 | Steps **par tâche** (non cumulatif sur replans) |
| `max_tool_calls` | u32 | 30 | Appels outils totaux |
| `wall_clock_timeout` | u64 | 600s | Timeout tâche (non contournable) |
| `token_budget` | Option<u32> | None | Tokens LLM max (si LLM backend supporte) |

### 6.2 ORIAConfig (runtime)

| Paramètre | Type | Défaut | Effet |
|---|---|---|---|
| `max_replans` | u32 | 2 | Nombre max de replanifications par tâche |
| `cache_enabled` | bool | true | Cache SQLite des plans identiques |
| `persistence_enabled` | bool | true | Persistance `~/.apollia/plans.db` |

---

## 7. Références internes

### Fichiers de code source
- **Plan** : `/crates/apollia-oria/src/plan.rs` — `ExecutionPlan`, `PlanStep`
- **Reasoner** : `/crates/apollia-oria/src/reasoner.rs` — `Reasoner`, `ReasonerError`, `PlanValidationError`
- **ActorLoop** : `/crates/apollia-oria/src/actor.rs` — `ActorLoop`, `StepError`, `ToolProxyTrait`
- **Observer** : `/crates/apollia-oria/src/observer.rs` — `ContextBundle`, `ExecutionMode`, classification
- **Engine** : `/crates/apollia-oria/src/engine.rs` — `ORIAEngine`, `ORIAError`

### Librairies associées
- `apollia-core` : `ORIAConfig`, `AIPResult`, `StepBudgetConfig`
- `apollia-llm` : `LlmRouter`, `CompletionRequest`, routing LLM
- `apollia-memory` : `MemoryManager`, enregistrement épisodique
- `apollia-tools` : `TaskRepository`, persistance HITL

---

## 8. Liens

> Pour le pattern d'usage du Mode Orchestré dans un agent Python, voir [book ch09 — Agents en mode orchestré](../../book/src/ch09-00-orchestrated.md).

> Pour la spécification interne complète (Observer, Reasoner, ActorLoop détaillés), voir [Briques-ORIA-Engine.md](./Briques-ORIA-Engine.md).

---

**Dernière mise à jour** : Sprint 40, 2026-04-24
