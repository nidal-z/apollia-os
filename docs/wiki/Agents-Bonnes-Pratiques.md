# Agents - Bonnes pratiques (checklist référence)

> Checklist consultée. Pour le **pourquoi** et les exemples détaillés, suivre les liens vers le book.
> Format : items checkables. Chaque item = une règle vérifiable.

---

## StepBudget - anticiper avant d'être arrêté

- [ ] **Vérifier le budget avant chaque itération** : `ctx.step_budget.steps_remaining` doit être contrôlé en début de boucle. Retourner un résultat partiel plutôt qu'attendre `BudgetExceeded`. → [book ch07-01](../../book/src/ch07-01-step-budget.md)
- [ ] **Adapter la profondeur au budget restant** : ratio `steps_remaining / max_steps` > 0.5 → mode normal ; < 0.2 → mode dégradé.
- [ ] **Déclarer un budget explicite dans le manifest** si l'agent dépasse les défauts (10 steps / 20 tool_calls / 300s). Le runtime plafonne via `min(agent_budget, runtime_defaults)`.
- [ ] **Coût mémorisé** : appel LLM = 1 step ; appel outil = 1 tool_call ; retry RetryPolicy = 1 tool_call additionnel.

---

## Mémoire - lire avant d'appeler un LLM

- [ ] **Chercher en mémoire avant chaque génération coûteuse** : `await ctx.memory.search(query, limit=5)` est gratuit, un LLM ne l'est pas. → [book ch05-02](../../book/src/ch05-02-fts5-search.md)
- [ ] **Filtrer les résultats par score** (≥ 0.3 typiquement) avant de les injecter dans le prompt.
- [ ] **Mémoriser les résultats utiles** avec une `importance` proportionnelle à la qualité ou à la rareté de l'information.
- [ ] **Toujours passer `task_id`** lors d'un `record()` pour la traçabilité dans l'audit.

---

## Outils - gérer les échecs explicitement

- [ ] **Ne jamais laisser propager une exception d'outil** sans la catcher et retourner un `AIPResult` explicite avec code d'erreur. → [book ch04-02](../../book/src/ch04-02-calling.md)
- [ ] **Tester `exit_code` sur `bash_executor`** - un exit code non nul ne déclenche pas d'exception, c'est à l'agent de le détecter.
- [ ] **Logger les détails d'erreur** (`stderr`, `exit_code`) via `ctx.log.warn(...)` avant de retourner un échec.

---

## Concurrence

- [ ] **Déclarer `max_concurrent_tasks`** explicitement dans le manifest. Défaut = 1. → [book ch03-01](../../book/src/ch03-01-manifest.md)
- [ ] **Garder `max_concurrent_tasks: 1` si l'agent maintient un état interne** entre appels (`self.X` modifié dans `run()`).
- [ ] **Augmenter à 2-5 uniquement si `run()` est stateless** (aucun attribut de `self` muté).

---

## Outils dangereux

- [ ] **Activer `dangerous_tools_allowed: true` uniquement si nécessaire**. Ce flag est audité dans le `AuditTrail`. → [Briques-Tool-Registry](./Briques-Tool-Registry.md)
- [ ] **Documenter dans la description de l'agent** la raison de l'activation (transparence pour l'opérateur).
- [ ] **Réserver aux agents de confiance** sous contrôle direct de l'opérateur - jamais dans un agent communautaire publié sans avertissement explicite.

---

## Logging

- [ ] **Utiliser `ctx.log.info/warn/error`** avec champs structurés (kwargs), jamais `print`.
- [ ] **Ne pas dupliquer `agent_id` ou `task_id`** dans les kwargs - le runtime les ajoute automatiquement.
- [ ] **Convention nommage** : verbe en snake_case (`processing_step`, `tool_failed`, `model_loaded`).

---

## Lifecycle (`on_start()` / `on_stop()`)

- [ ] **Charger les ressources coûteuses dans `on_start()`**, jamais dans `__init__()` (qui est appelé sans `ctx`). → [book ch03-03](../../book/src/ch03-03-lifecycle.md)
- [ ] **Libérer les ressources dans `on_stop()`** (connexions, modèles GPU, fichiers ouverts).
- [ ] **Logger l'init et la libération** pour faciliter le diagnostic au démarrage du runtime.

---

## Erreurs connues à éviter

- [ ] **Boucle non bornée par budget** - toujours vérifier `steps_remaining` dans la condition de boucle.
- [ ] **Exception non catchée dans `run()`** - termine la tâche en `failed` avec message générique. Toujours wrap dans `try/except` les blocs susceptibles d'échouer.
- [ ] **Appel d'outil non déclaré** dans `tools_required` ou `tools_optional` du manifest → `ToolNotAllowed`.
- [ ] **Promesse non await** - un `await` oublié sur un appel async produit un warning silencieux et un comportement imprévisible.

---

## Tests

- [ ] **Test unitaire avec mock RuntimeContext** - utiliser les helpers du SDK (`apollia_sdk.testing`). → [book ch08-04](../../book/src/ch08-04-tests.md)
- [ ] **Test d'intégration avec runtime réel** au moins pour le golden path.
- [ ] **Test du cas budget épuisé** - vérifier que l'agent retourne un résultat partiel cohérent.
- [ ] **Test du cas outil indisponible** (MCP désactivé, etc.) - vérifier la dégradation gracieuse.

---

## Publication

- [ ] **Manifest complet** - `description`, `version` (semver), `author`, `tools_required`, `tools_optional`. → [book ch08-05](../../book/src/ch08-05-publish.md)
- [ ] **README de l'agent** - exemple de tâche, format d'input attendu, exemple de `[permissions]` minimal pour l'opérateur.
- [ ] **Tag semver et changelog** entre versions.
- [ ] **Test E2E sur version publiée** avant tag final.

---

## Voir aussi

- [Agents-RuntimeContext-Guide](./Agents-RuntimeContext-Guide.md) - table des services injectés
- [Briques-AIP-Specification](./Briques-AIP-Specification.md) - tous les champs du manifest
- [Securite-Guardrails](./Securite-Guardrails.md) - plafonds runtime
