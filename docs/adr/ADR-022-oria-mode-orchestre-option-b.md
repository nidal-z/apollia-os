# ADR-022 - ORIA Mode Orchestré : Option B (exécution directe outils) + hook `on_plan_complete`

**Date :** 2026-03-09
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 10

---

## Contexte

ADR-004 a posé le principe de deux modes d'exécution dans ORIA (Direct + Orchestré). Le Mode Direct est implémenté depuis Sprint 4 : `agent.run()` est appelé directement avec supervision `StepBudget`. Sprint 10 implémente le Mode Orchestré.

La question centrale est : **pendant l'exécution d'un plan multi-step, qui exécute les outils - ORIA ou l'agent Python ?**

Trois architectures sont envisageables :

1. **Option A** - ORIA délègue chaque step à `agent.run()` (appel répété)
2. **Option B** - ORIA exécute les outils directement sans appeler `run()`, l'agent est purement déclaratif
3. **Option C** - ORIA appelle `agent.run()` une seule fois avec un contexte enrichi (plan injecté), l'agent itère lui-même

Une décision connexe porte sur le **post-traitement** : comment l'agent récupère-t-il les outputs agrégés de tous les steps ? L'agent Python peut vouloir appliquer une logique métier sur les résultats avant de retourner l'`AIPResult` final. Ce post-traitement est optionnel - un agent sans logique custom doit fonctionner sans modification.

---

## Décision

Nous adoptons **Option B** : ORIA exécute les outils directement pendant le Mode Orchestré via l'`ActorLoop`. `agent.run()` n'est **jamais** appelé pendant l'exécution des steps.

Pour le post-traitement, nous introduisons un **hook optionnel `on_plan_complete(step_results, ctx)`** détecté via duck typing Python (`hasattr`). Si l'agent expose ce hook, ORIA l'appelle après l'exécution complète du plan en lui injectant tous les outputs des steps (`dict[str, str]`). Si le hook est absent, ORIA concatène automatiquement les outputs et retourne un `AIPResult::Completed`.

Le contrat AIP (ADR-003) est étendu minimalement : `manifest()` + `run()` restent suffisants. `on_plan_complete()` est un troisième hook **optionnel**, jamais obligatoire.

---

## Alternatives considérées

### Option A - ORIA délègue chaque step à `agent.run()` (rejetée)

**Architecture :** `ActorLoop` appelle `agent.run(step_context)` pour chaque step. L'agent Python reçoit le contexte du step en cours et ses outils disponibles limités au step.

**Pour :**
- L'agent garde le contrôle de chaque step - logique métier fine possible à chaque étape.
- Compatible avec les agents LangGraph/CrewAI existants qui ont déjà leur propre boucle d'exécution.

**Contre :**
- L'agent doit gérer lui-même l'état inter-steps (mémoriser les outputs du step précédent). Complexité reportée sur le développeur d'agent.
- `run()` appelé N fois pour une tâche → comportement difficilement prévisible si l'agent a des side effects.
- `StepBudget` doit être partagé et passé entre chaque appel `run()` - interface AIP plus complexe (viole Principe #3).
- Tests plus difficiles : la boucle d'exécution est dans du code Python hors du contrôle du runtime.

### Option C - ORIA injecte le plan dans `agent.run()` unique (rejetée)

**Architecture :** `agent.run()` est appelé une seule fois avec un `RuntimeContext` enrichi contenant le plan complet. L'agent itère lui-même sur les steps via `ctx.plan.steps`.

**Pour :**
- L'agent garde la vue d'ensemble du plan.
- Un seul appel `run()` - comportement proche du Mode Direct.

**Contre :**
- Expose la structure interne d'`ExecutionPlan` à l'agent Python - couplage fort entre AIP et le format de plan ORIA.
- L'agent doit implémenter sa propre boucle d'exécution des outils - duplique la logique `ActorLoop`.
- La persistance SQLite par step, la `ResilienceLayer`, et le `StepBudget` par step sont contournés - les garde-fous du runtime ne s'appliquent plus.
- `run()` peut bloquer longtemps - difficile à interrompre ou replanifier sans coopération de l'agent.

### Option retenue - Option B : ORIA exécute les outils directement

**Architecture :** L'agent fournit son `manifest()` avec `system_prompt` et `tools_required`. ORIA génère le plan via `Reasoner`, l'`ActorLoop` exécute chaque step en appelant directement les outils Rust via `ToolProxy`. L'agent Python n'est pas impliqué dans l'exécution des steps. `on_plan_complete()` (optionnel) est le seul point de ré-entrée Python après l'exécution.

**Pour :**
- L'agent devient **déclaratif** - il décrit ce qu'il veut (manifest + system_prompt), ORIA décide comment l'exécuter. Friction minimale pour le développeur d'agent.
- `ResilienceLayer`, `StepBudget`, persistance SQLite, et replanification sont entièrement sous contrôle du runtime - les garde-fous (Principe #7) sont appliqués systématiquement.
- Tests unitaires sans Python : `ActorLoop` peut être testé avec un mock `CompletionModel` et un mock `ToolProxy` sans interpréteur Python.
- L'`ActorLoop` peut à terme paralléliser les steps indépendants (steps sans `depends_on` communs) - optimisation impossible avec Option A ou C.

**Compromis acceptés :**
- L'agent perd le contrôle fin sur chaque step. `on_plan_complete()` compense partiellement pour le post-traitement.
- La qualité du plan dépend entièrement du `Reasoner` (LLM) - un plan mal généré ne peut pas être corrigé par l'agent step par step (seulement via replanification max 2 fois).
- Les agents LangGraph/CrewAI existants avec leur propre boucle ReAct ne bénéficient pas du Mode Orchestré - ils continuent en Mode Direct.

---

## Spécification du hook `on_plan_complete`

### Détection (duck typing, ADR-003)

```rust
// AIPAgent::has_on_plan_complete() via hasattr Python
fn has_on_plan_complete(&self) -> bool {
    Python::with_gil(|py| {
        agent.getattr(py, "on_plan_complete")
            .map(|attr| !attr.is_none(py))
            .unwrap_or(false)  // toute erreur → false, jamais de panique
    })
}
```

### Injection (AIPBridge, pattern ADR-014)

```rust
// spawn_blocking + asyncio.run() - même pattern que call_run() et call_on_start()
pub async fn call_on_plan_complete(
    &self,
    agent:        &Py<PyAny>,
    step_results: HashMap<String, String>,
    ctx:          RuntimeContext,  // #[pyclass]
) -> Result<AIPResult, AIPBridgeError>
```

`step_results` est converti en `PyDict` via `PyDict::new_bound(py)` + `set_item()`. Le hook Python reçoit `dict[str, str]` et `RuntimeContext`.

### Fallback automatique (absence de hook)

```rust
// Concaténation des outputs si on_plan_complete() absent
let text = outputs.values().cloned().collect::<Vec<_>>().join("\n\n");
AIPResult::completed(&text)
```

---

## Conséquences

**Positives :**
- Agent déclaratif : `manifest()` + `system_prompt` suffisent pour tirer parti du Mode Orchestré.
- Tous les garde-fous runtime (`StepBudget`, `ResilienceLayer`, audit SQLite) s'appliquent à chaque step sans coopération de l'agent.
- `ActorLoop` entièrement testable en Rust pur (mock `CompletionModel`, mock `ToolProxy`).
- Architecture extensible : parallélisme des steps indépendants, caching d'outputs, rejoue de step → faisables sans modifier le contrat AIP.
- `on_plan_complete()` offre un point de ré-entrée Python pour la logique métier custom sans rompre le Principe #3.

**Négatives / Compromis :**
- L'agent ne peut pas modifier dynamiquement le plan step par step - la replanification est déléguée à ORIA (max 2 fois par tâche).
- `system_prompt` devient obligatoire en Mode Orchestré → validation fail fast dans `execute_orchestrated()` (Principe #4 respecté).
- Le hook `on_plan_complete()` ajoute une troisième méthode au contrat AIP documenté, même si elle reste optionnelle (documentation à maintenir).
- La sérialisation `HashMap<String, String>` → `PyDict` est la zone la plus délicate côté PyO3 - à surveiller si les outputs contiennent des caractères non-ASCII.

**Neutres / À surveiller :**
- Précision du `Reasoner` sur des plans de plus de 8 steps : surveiller le taux de replanification en production.
- Si `on_plan_complete()` lève une exception Python, le résultat de la tâche est `failed` - l'agent doit gérer ses propres erreurs dans le hook.
- Évolution future possible : `on_step_complete(step_id, output, ctx)` si des agents ont besoin d'un callback par step (hors scope Sprint 10).

---

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : Le contrat AIP reste `manifest()` + `run()`. `on_plan_complete()` est un troisième hook optionnel - il n'est jamais requis. Un agent sans cette méthode fonctionne identiquement.
- **Principe #4 - Fail fast** : `system_prompt: None` + `execution_mode="orchestrated"` → `AIPResult::failed("MISSING_SYSTEM_PROMPT")` retourné immédiatement, sans appel au `Reasoner`.
- **Principe #7 - Garde-fous non-négociables** : `StepBudget::from_capped()` est appliqué dans `execute_orchestrated()` - le budget global du runtime plafonne celui déclaré dans le manifest, sans exception.
- **Principe #5 - Un acteur, une responsabilité** : `ActorLoop` est responsable de l'exécution des steps. `Reasoner` est responsable de la génération du plan. `ORIAEngine` orchestre les deux sans mélanger les responsabilités.

---

## Liens

- Stories associées : STORY-079 à STORY-091 (Sprint 10)
- ADR précédents liés :
  - ADR-003 - Duck typing AIP : `on_plan_complete()` suit le même principe `hasattr`
  - ADR-004 - Deux modes d'exécution ORIA : cette ADR précise l'implémentation du Mode Orchestré
  - ADR-014 - `spawn_blocking` + `asyncio.run()` : pattern réutilisé pour `call_on_plan_complete()`
  - ADR-016 - Trait `AgentRunner` : pattern trait testable réutilisé pour `ToolProxyTrait` et `CompletionModel`
