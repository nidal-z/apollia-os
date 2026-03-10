# ADR-025 — apollia-pipelines : TOML déclaratif, 5 topologies natives par graph `depends_on`, HITL intégré via EventBus

**Date :** 2026-03-10
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 12

---

## Contexte

Sprint 12 introduit la coordination multi-agent : plusieurs agents indépendants doivent pouvoir
être enchaînés dans un pipeline supervisé par le runtime. Quatre décisions structurantes doivent
être arrêtées avant l'implémentation, car elles engagent des interfaces publiques difficiles
à inverser :

1. **Comment l'opérateur déclare-t-il un pipeline ?**
   Le runtime doit lire des définitions de pipelines depuis une source de configuration.
   Le format et l'emplacement conditionnent la DX et la cohérence avec l'existant (triggers,
   LLM backends — tous dans `apollia.toml`).

2. **Comment les 5 topologies (séquentiel, fan-out, fan-in, conditionnel, fallback) sont-elles
   exprimées ?**
   Un pipeline peut être purement séquentiel, parallèle, avec branches conditionnelles, et avec
   reprise automatique sur échec. La question est : ces topologies sont-elles des primitives
   explicites du TOML ou émergent-elles d'un graph de dépendances `depends_on` ?

3. **Comment le HITL (Sprint 11) s'intègre-t-il dans un pipeline ?**
   Un agent exécuté dans un step peut demander une approbation humaine. Le pipeline doit suspendre
   les steps dépendants, attendre la réponse humaine, puis reprendre. La question est : le pipeline
   gère-t-il ce cas via un mécanisme propre ou réutilise-t-il les événements HITL existants ?

4. **Comment les inputs des steps sont-ils construits à partir des outputs précédents ?**
   Le pipeline doit pouvoir transmettre l'output d'un step à l'input d'un autre via des références
   (`{{steps.ocr.output}}`). La question est : un moteur de templates complet ou un renderer
   minimal suffit-il ?

Les contraintes non-négociables qui encadrent ces choix :

- **Principe #1 — Local-first** : zéro service externe pour l'orchestration. Les définitions de
  pipelines vivent dans `apollia.toml` et l'état dans SQLite local.
- **Principe #4 — Fail fast** : cycles dans le graph, `depends_on` vers un step inexistant,
  `fallback_for` orphelin — toutes ces erreurs doivent être détectées au parsing, pas à l'exécution.
- **Principe #5 — Un acteur, une responsabilité** : `PipelineEngine` reçoit les demandes et
  spawne des `PipelineExecutor` indépendants — jamais d'état partagé entre runs concurrents.
- **ADR-021** — Cohérence avec le pattern TOML-only des triggers.
- **ADR-023** — Le mécanisme HITL (`AIPTask.is_resumed`, `TaskInputRequired`/`TaskResumed`) est
  déjà implémenté au Sprint 11 et doit être réutilisé sans duplication.

---

## Décision

### Décision 1 — Configuration TOML-only `[[pipelines]]` dans `apollia.toml`

Nous étendons `apollia.toml` avec une section `[[pipelines]]`. Chaque pipeline est une liste de
`[[pipelines.steps]]` avec des champs `id`, `agent`, `input`, `depends_on`, `on_failure`,
`condition`, `fallback_for`. La configuration est parsée dans `ApolliaConfig::load()` au démarrage
du runtime.

La validation sémantique est exhaustive au démarrage :
- Unicité des `step.id` au sein d'un pipeline.
- Chaque `depends_on` doit référencer un `step.id` existant dans le même pipeline.
- Chaque `fallback_for` doit référencer un `step.id` existant.
- Absence de cycles (détection via tri topologique — échec = `PipelineConfigError::CyclicDependency`).
- Unicité des `pipeline.id` dans l'ensemble du fichier.

Un pipeline invalide empêche le démarrage du runtime (Principe #4). Un trigger avec `pipeline`
qui référence un pipeline inexistant est également une erreur fatale au démarrage.

### Décision 2 — Topologies natives par graph `depends_on` sans primitive explicite

Nous n'introduisons pas de clé `topology` dans le TOML. Les 5 topologies émergent naturellement
du graph de dépendances et des champs `condition`/`fallback_for` :

| Topologie     | Mécanisme                                                              |
|---------------|------------------------------------------------------------------------|
| Séquentiel    | Chaque step a `depends_on` du précédent → une layer par step           |
| Fan-out       | Plusieurs steps avec le même `depends_on` → même layer, `FuturesUnordered` |
| Fan-in (join) | Un step avec plusieurs `depends_on` → layer suivante seulement quand tous complétés |
| Conditionnel  | `[pipelines.steps.condition]` : step skipped si `when/field/value` false |
| Fallback      | `fallback_for = "step-x"` : step inactif par défaut, activé si `step-x` échoue avec `on_failure = "fallback"` |

L'ordonnancement est calculé par `topological_layers()` : une fonction qui partitionne les steps
en layers (`Vec<Vec<StepId>>`), où tous les steps d'une layer ont leurs dépendances satisfaites
par les layers précédentes. `FuturesUnordered` exécute les steps d'une même layer en parallèle.

### Décision 3 — HITL via EventBus (`TaskInputRequired`/`TaskResumed`) — pas de canal dédié

Nous réutilisons les événements `TaskInputRequired` et `TaskResumed` introduits en Sprint 11
(ADR-023). Quand `wait_for_step()` reçoit `TaskInputRequired` pour la tâche d'un step :

1. `PipelineExecutor` émet `PipelineSuspended` sur l'EventBus.
2. Le run est marqué `waiting_approval` dans SQLite.
3. Les steps dépendants du step suspendu ne sont pas soumis.
4. `wait_for_resume()` écoute l'EventBus jusqu'à `TaskResumed { task_id }`.
5. Après `TaskResumed`, `PipelineExecutor` écoute un nouveau `TaskCompleted`/`TaskFailed`
   sur le `new_task_id` fourni par le `ResumeHandler`.

Le `ResumeHandler` (Sprint 11) reste inchangé — il gère la logique métier approbation/rejet.
Le `PipelineExecutor` ne connaît pas la mécanique HITL interne : il observe uniquement les
événements publiés.

### Décision 4 — TemplateRenderer par remplacement de chaîne, sans moteur de templates externe

Nous implémentons un `TemplateRenderer` minimal dans `apollia-pipelines/src/template.rs` :

```rust
pub struct TemplateContext {
    pub trigger_payload: String,
    pub pipeline_id:     String,
    pub run_id:          String,
    pub step_outputs:    HashMap<StepId, String>,
}
```

`render()` effectue des remplacements directs de chaîne pour les variables statiques
(`{{trigger.payload}}`, `{{pipeline.id}}`, `{{pipeline.run_id}}`), puis itère sur
`step_outputs` pour les variables dynamiques (`{{steps.<id>.output}}`). Les variables non
résolues sont nettoyées via regex (`\{\{[^}]+\}\}` → chaîne vide) — jamais de `panic!`.

Nous n'utilisons pas Handlebars, Tera, ou MiniJinja.

---

## Alternatives considérées

### Option A — API REST CRUD pour les pipelines (rejetée)

Permettre la création de pipelines via `POST /api/v1/pipelines` avec persistance SQLite.
`apollia.toml` serait optionnel.

**Pour :**
- Création programmatique sans modifier le fichier de config.
- Cohérence avec l'API agents/tasks.

**Contre :**
- Double source de vérité (TOML initial + base de données). Même problème qu'en ADR-021 Option A.
- Pas de fail fast naturel : les erreurs de cycle ou de référence manquante apparaissent à
  l'exécution du premier run, pas au démarrage.
- Complexité CRUD complète (endpoints create/update/delete/version) disproportionnée pour Sprint 12.
- Rompt la cohérence avec `[[triggers]]` et `[[llm.backends]]` — tous gérés via TOML.

### Option B — Topologies explicites (type `topology: "fan-out"`) dans le TOML (rejetée)

Introduire une clé `topology` par step ou par pipeline pour déclarer explicitement la topologie.

**Pour :**
- TOML plus lisible pour les utilisateurs novices — la topologie est visible sans lire le graph.

**Contre :**
- Redondant avec le graph `depends_on` — `topology: "fan-out"` ne fait que nommer ce que le
  graph exprime déjà.
- Source de conflits si `topology` contredit le graph réel (deux sources de vérité intra-TOML).
- Les outils d'orchestration matures (Argo Workflows, Airflow) n'utilisent pas de type explicite :
  ils infèrent la topologie du DAG de dépendances — c'est le standard établi.
- Valide uniquement pour les topologies pures ; un pipeline hybride (fan-out + conditionnel +
  fallback sur le même DAG) ne peut pas être typé avec une seule clé.

### Option C — DSL externe type Argo Workflows / Prefect YAML (rejetée)

Adopter un format d'orchestration existant (Argo YAML, Prefect Python, Temporal) pour décrire
les pipelines, et l'implémenter dans Apollia OS.

**Pour :**
- Compatibilité avec l'écosystème existant — les utilisateurs migrant depuis Argo ou Prefect
  n'ont pas à réapprendre un format.
- Expressivité supérieure (retry policies par step, artefacts, volumes).

**Contre :**
- Viole le Principe #2 — ces DSLs supposent un orchestrateur dédié (Kubernetes pour Argo,
  Prefect Cloud pour Prefect).
- Surface d'API massive à implémenter en Sprint 12 (18 stories déjà planifiées).
- Aucun des formats existants n'intègre nativement le HITL `apollia-os`-style.
- `apollia.toml` comme source de vérité unique est un avantage DX ; fragmenter vers un second
  fichier de configuration le dilue.

### Option D — HITL via canal oneshot dédié `PipelineExecutor → ResumeHandler` (rejetée)

Créer un `tokio::sync::oneshot::Sender<ResumeDecision>` que le `PipelineExecutor` passe au
`ResumeHandler` lors de la suspension. Le `ResumeHandler` envoie la décision directement.

**Pour :**
- Communication directe sans passer par l'EventBus broadcast.
- Pas de filtrage d'events par `task_id` nécessaire.

**Contre :**
- Couplage structurel `PipelineExecutor ↔ ResumeHandler` : le `ResumeHandler` doit connaître
  l'existence du pipeline en cours.
- Duplique la logique de suspension déjà dans `ORIAEngine` (Mode Direct) et `ActorLoop`
  (Mode Orchestré via ADR-023) — trois mécanismes HITL distincts dans le codebase.
- `TaskResumed` sur l'EventBus est déjà émis par le `ResumeHandler` (Sprint 11) — le
  `PipelineExecutor` peut l'écouter sans modification du `ResumeHandler`.
- Un `oneshot` est drop si le `PipelineExecutor` est relancé après restart — la reprise
  depuis SQLite ne peut pas restaurer un channel Tokio en mémoire.

### Option E — Moteur de templates complet (Handlebars / Tera / MiniJinja) (rejetée)

Utiliser une crate de templates pour `{{steps.x.output}}`.

**Pour :**
- Expressivité supplémentaire : conditions `{{#if}}`, boucles, filtres.
- Réutilisation d'une crate éprouvée au lieu de code custom.

**Contre :**
- Même décision qu'ADR-024 Option 3 (templates Handlebars/Tera rejetée pour les notifications) :
  dépendance ~500 Ko pour un besoin couvrable par string replace.
- Les variables pipeline sont statiques et connues au compile time — pas besoin de conditions
  dans les templates (la logique conditionnelle est dans `StepCondition`, pas dans l'input).
- Bugs silencieux : une variable mal orthographiée dans Handlebars produit une chaîne vide sans
  erreur — même comportement que notre regex cleanup, mais avec plus de surface.
- Syntaxe Handlebars incompatible avec la syntaxe `{{steps.x.output}}` choisie (`.` dans une
  variable est valide en Tera, mais pas en Handlebars sans helper custom).

### Option retenue — TOML `[[pipelines]]` + graph `depends_on` + EventBus HITL + TemplateRenderer minimal

**Pour :**
- **Cohérence** : même pattern que `[[triggers]]` (ADR-021) et `[[llm.backends]]` (ADR-020).
  `apollia.toml` reste la source de vérité unique pour l'ensemble du runtime.
- **Fail fast** : validation sémantique exhaustive au démarrage — cycles, références manquantes,
  IDs dupliqués détectés avant le premier run.
- **Minimalisme** : les 5 topologies émergent du graph `depends_on` sans primitive TOML
  supplémentaire — moins de surface de config à documenter et à valider.
- **Réutilisation** : EventBus HITL (Sprint 11), tri topologique (apollia-oria), SQLite
  (apollia-tools migrations) — pas de nouveaux systèmes.
- **Testabilité** : `PipelineExecutor` testable avec un mock `TaskRouter` + mock `EventBus`.
  `topological_layers()` est une fonction pure. `TemplateContext::render()` est pure.

**Compromis acceptés :**
- La validation `enabled = false` pour les triggers ne s'applique pas aux pipelines (tous
  les pipelines sont toujours validés au démarrage — pas de concept "désactivé" au parsing).
- `FuturesUnordered` par layer : si une layer contient un step HITL, tous les autres steps
  de la même layer s'exécutent en parallèle pendant la suspension — comportement correct mais
  potentiellement surprenant (documenté dans la spec).
- La reprise après restart nécessite que les steps déjà complétés soient rechargés depuis
  SQLite — la reconstruction du `TemplateContext` doit réévaluer les outputs persists.
- Regex cleanup des variables non résolues masque les typos dans les templates — validé au
  parsing TOML uniquement pour les variables statiques, pas pour les `{{steps.x.output}}`.

---

## Conséquences

**Positives :**
- `apollia.toml` est la source de vérité unique pour trigger → pipeline → agent : la chaîne
  complète est déclarative, versionnée, et vérifiable dans le même fichier.
- Le tri topologique par layers garantit une exécution déterministe : fan-out et fan-in se
  comportent de la même façon quelle que soit l'ordre de déclaration des steps dans le TOML.
- `PipelineEngine` réutilise `TaskRouter` sans modification : un step pipeline est une tâche
  ordinaire vue du runtime. Tous les garde-fous existants (StepBudget, ResilienceLayer, audit
  SQLite) s'appliquent automatiquement.
- La reprise après restart est native : `pipeline_runs` en status `running` dans SQLite
  reconstituent leur état depuis `pipeline_step_runs` complétés. Les steps skipped et
  completed ne sont pas re-soumis.
- `PipelineCompleted`/`PipelineFailed` sur l'EventBus sont captés par `NotificationEngine`
  (Sprint 11, ADR-024) sans modification.

**Négatives / Compromis :**
- `apollia-pipelines` est une 9e crate workspace. La surface de code à maintenir augmente.
  Dette justifiée par la séparation claire des responsabilités (Principe #5).
- La migration SQLite `006_pipeline_tables.sql` ajoute deux tables dans `apollia-tools`
  (où vivent toutes les migrations). Couplage géographique accepté pour la cohérence
  du système de migrations.
- Le fallback modifie le graph à l'exécution (`activate_fallback()` marque le step original
  comme `FallbackActive`). La re-évaluation des layers après activation d'un fallback nécessite
  un `break` sur la boucle principale et un recalcul de `topological_layers()` — complexité
  locale, isolée dans `PipelineExecutor::execute()`.
- `regex = "1"` ajouté comme dépendance workspace pour le cleanup des variables non résolues
  dans `TemplateRenderer`. Usage unique — alternative envisagée (loop char-by-char) plus
  verbeux pour un gain de dépendance marginal.

**Neutres / À surveiller :**
- `TriggerDefinition.pipeline` (nouveau champ, exclusif avec `agent`) : la validation
  mutuellement exclusive est détectée au parsing — à tester avec un TOML qui déclare les deux.
- Concurrence des `PipelineRun` : deux runs du même pipeline peuvent s'exécuter en parallèle
  si `on_busy = "queue"` n'est pas configuré sur le trigger. Comportement voulu mais à
  documenter explicitement (les steps accèdent aux mêmes agents — le `ExecutionCoordinator`
  applique le `max_concurrent_tasks` du manifest agent).
- `wait_for_resume()` boucle sur l'EventBus en attendant `TaskResumed { task_id }` — si le
  `task_id` ne correspond jamais (reject du ResumeHandler émet `TaskFailed`, pas `TaskResumed`),
  la boucle doit gérer `TaskFailed` comme cas terminal. À vérifier dans STORY-114.
- Rotation TTL des `notification_logs` mentionnée en ADR-024 : `pipeline_runs` accumulés sans
  purge → à prévoir dans un sprint ultérieur (archivage runs > 90j).

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : définitions TOML locales, état SQLite local, exécution sans
  service d'orchestration externe. Aucune donnée ne quitte la machine.
- **Principe #2 — Zéro dépendance externe** : `apollia-pipelines` n'ajoute que `regex = "1"`
  comme nouvelle dépendance workspace. Les crates d'orchestration (Temporal, Argo) ne sont pas
  utilisées.
- **Principe #4 — Fail fast** : `ApolliaConfig::load()` valide cycles, références manquantes
  et IDs dupliqués avant le démarrage du runtime. Un pipeline invalide = démarrage refusé.
- **Principe #5 — Un acteur, une responsabilité** : `PipelineEngine` est l'acteur de dispatch.
  Chaque `PipelineRun` est un `tokio::spawn(PipelineExecutor)` autonome sans état partagé.
- **Principe #7 — Garde-fous non-négociables** : le `StepBudget` est appliqué par le runtime
  sur chaque tâche soumise par le `PipelineExecutor` — l'orchestrateur pipeline ne peut pas
  le contourner.
- **Principe #8 — CLI humaine, API machine** : `apollia-os pipeline run|list|runs|status`
  suit le pattern noun-verb (ADR-008). `--json` disponible sur toutes les commandes.

---

## Liens

- Stories associées : STORY-107 → STORY-124 (Sprint 12)
- STORY-107 : types fondamentaux `PipelineDefinition`, `PipelineRun`, etc.
- STORY-108 : migration SQLite `006_pipeline_tables.sql` + `PipelineRepository`
- STORY-109 : `TemplateRenderer` (Décision 4 de cet ADR)
- STORY-110 : `topological_layers()` (Décision 2)
- STORY-111 : `PipelineExecutor` séquentiel + fan-out
- STORY-114 : HITL dans les pipelines (Décision 3)
- STORY-118 : parsing config `[[pipelines]]` dans `apollia.toml` (Décision 1)
- ADR précédents liés :
  - ADR-021 — apollia-triggers TOML-only : même pattern de configuration pour les pipelines
  - ADR-023 — HITL `is_resumed` + `InputResponse` : les événements `TaskInputRequired`/`TaskResumed`
    réutilisés sans modification par `PipelineExecutor`
  - ADR-024 — apollia-notifications trait+JSON fixe : `PipelineCompleted`/`PipelineFailed`
    captés par `NotificationEngine` sans nouveau mécanisme
  - ADR-004 — Deux modes ORIA : les steps pipeline utilisent les deux modes selon le manifest
    de chaque agent cible
  - ADR-008 — Pattern noun-verb CLI : `apollia-os pipeline <verb>`
