# ADR-035 - Per-step observation en mode Orchestré

**Date :** 2026-03-23
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 20

---

## Contexte

En mode Orchestré ORIA, le runtime pilote directement l'exécution des outils (le `run()` Python de l'agent n'est pas appelé pendant les steps). Actuellement, les outputs de chaque step sont stockés dans un `HashMap` et persistés en SQLite (`plans.db`), mais ne sont **pas injectés** dans le contexte des steps suivants et ne sont **pas auto-enregistrés** en mémoire épisodique.

Le Principe #6 stipule "mémoire à initiative de l'agent" - mais en mode Orchestré, l'agent ne contrôle pas l'exécution. C'est le runtime (`ActorLoop`) qui orchestre les steps. Sans injection de contexte inter-steps, chaque step s'exécute aveuglément sans bénéficier des résultats précédents, ce qui dégrade la qualité des plans multi-steps.

## Décision

Nous adoptons l'injection delta légère en mode Orchestré : après chaque step, le runtime injecte les outputs précédents dans le contexte du step suivant (`StepContext` struct) et auto-enregistre une entrée mémoire épisodique (importance 0.6). Le Principe #6 est relâché UNIQUEMENT en mode Orchestré - en mode Direct, l'agent garde le contrôle total de sa mémoire.

## Alternatives considérées

### Option A - Full re-observation per step via Observer (rejetée)
**Pour :** Re-analyse complète de l'état à chaque step, détection de changements de contexte, possibilité de re-planifier.
**Contre :** Requiert un appel LLM supplémentaire par step, coût prohibitif, latence proportionnelle au nombre de steps.

### Option B - No observation, plan-once-execute (actuel, rejetée)
**Pour :** Simple, aucun overhead, implémentation existante.
**Contre :** Les steps s'exécutent aveuglément sans contexte des steps précédents, impossible de s'adapter aux résultats intermédiaires.

### Option retenue - Lightweight delta injection
**Pour :** Zéro appel LLM supplémentaire, overhead minimal (HashMap lookup + écriture mémoire fire-and-forget), préserve la vitesse d'exécution des steps.
**Compromis acceptés :** Les écritures mémoire sont fire-and-forget (peuvent être perdues en cas de crash), seuls les outputs de steps sont injectés (pas de re-observation complète).

## Conséquences

**Positives :**
- Les steps bénéficient des outputs des steps précédents via `StepContext`
- La mémoire épisodique construit automatiquement une trace d'exécution exploitable
- Aucune pénalité de performance (pas d'appel LLM supplémentaire)

**Négatives / Compromis :**
- Le Principe #6 est relâché en mode Orchestré (exception documentée)
- Les écritures mémoire fire-and-forget peuvent être perdues en cas de crash (acceptable - l'audit trail capture toujours les invocations d'outils)

**Neutres / À surveiller :**
- Surveiller la croissance de la base mémoire épisodique pour les plans avec beaucoup de steps
- Évaluer si l'importance fixe 0.6 est appropriée ou si elle doit varier selon le type de step

## Principes architecturaux impactés
- Principe #6 - Mémoire à initiative de l'agent : relâché en mode Orchestré uniquement (l'agent ne pilote pas l'exécution, c'est le runtime)
- Principe #5 - Un acteur, une responsabilité : `ActorLoop` gère désormais aussi le `StepContext` (acceptable - c'est l'orchestrateur d'exécution)

## Liens
- Story associée : STORY-228, STORY-229, STORY-230
