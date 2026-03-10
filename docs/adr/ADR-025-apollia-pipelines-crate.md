# ADR-025 — `apollia-pipelines` : crate dédiée, topologies déclaratives, HITL intégré

**Date :** 2026-03-10
**Statut :** Accepté
**Story associée :** STORY-115 (Sprint 12)

---

## Contexte

Sprint 12 introduit l'orchestration multi-agent par pipeline déclaratif TOML. Deux options architecturales ont été évaluées :

1. **Extension d'`apollia-oria`** — ajouter la notion de pipeline dans le moteur ORIA existant
2. **Nouvelle crate `apollia-pipelines`** — crate isolée avec ses propres types, dépôt SQLite, et acteur Tokio

La question de la gestion HITL (Human-in-the-Loop, Sprint 11) et des topologies fan-out/fan-in/fallback s'est posée : faut-il enrichir `ORIAEngine` ou créer une abstraction propre ?

---

## Décision

**Option choisie : crate dédiée `apollia-pipelines`.**

### Raisons

**1. Responsabilité distincte**
`apollia-oria` exécute un agent en mode Direct ou Orchestré — c'est un moteur de raisonnement par agent. `apollia-pipelines` coordonne plusieurs agents en DAG — c'est un orchestrateur de workflow. Ces deux responsabilités sont fondamentalement différentes (Principe #5).

**2. Inversion de dépendance préservée**
`apollia-pipelines` ne dépend pas d'`apollia-runtime`. L'injection du `TaskSubmitter` (trait) découple le pipeline executor du `TaskRouterHandle` concret. Ce pattern suit ADR-015 (`ToolExecutor`) et ADR-016 (`AgentRunner`).

**3. Schéma SQLite isolé**
Les tables `pipeline_runs` et `pipeline_step_runs` (migration `006_pipeline_tables.sql`) sont gérées par `apollia-pipelines` directement via `rusqlite`, sans passer par `apollia-tools`. Cela évite de polluer le domaine des outils avec les concepts de pipeline.

**4. HITL intégré nativement**
`PipelineExecutor` intègre `wait_for_resume()` qui s'abonne à l'EventBus et écoute `TaskResumed` depuis `apollia-core`. Le pattern subscribe-before-submit garantit qu'aucun événement n'est perdu (invariant documenté dans le module `executor`).

**5. Topologies natives sans duplication**
`topological_layers()` (Kahn BFS) est implémenté dans `apollia-pipelines/src/topo.rs`, distinct de la logique similaire dans `apollia-oria`. La sémantique est différente : ici les layers contiennent des `PipelineStepDef` (pas des `PlanStep` LLM) et la notion de fallback actif change dynamiquement le graphe.

---

## Acteur `PipelineEngine` (STORY-115)

`PipelineEngine` suit le pattern acteur Tokio standard d'Apollia OS :

```
PipelineEngine (struct interne)
  ├── mpsc::channel(256)
  ├── PipelineEngineHandle (clonable, Send + Sync)
  ├── resume au démarrage via find_running_runs()
  ├── ShutdownRequested observé via EventBus
  └── spawne PipelineExecutor par run (tâche Tokio détachée)
```

**Inversion de dépendance `TaskSubmitter` :**
`PipelineEngine` stocke `Arc<dyn TaskSubmitter>`. Lors du spawn d'un `PipelineExecutor<S>`, un `ArcTaskSubmitter` newtype adapte le trait object vers le type générique — même pattern que `AIPBridgeBackend` (Sprint 6).

**Reprise après restart :**
`PipelineRepository::find_running_runs()` réinitialise automatiquement les steps en status `running` à `pending` (côté SQLite). `PipelineExecutor::as_resume()` :
- Précharge `done_steps` depuis les steps déjà terminaux dans `run.step_runs`
- Passe `is_resume = true` pour que `init_step_rows()` ne ré-insère pas les lignes existantes

---

## Conséquences

**Positives :**
- Zéro couplage `apollia-pipelines` ↔ `apollia-runtime` (testabilité maximale)
- L'acteur `PipelineEngine` est en position 8 du Supervisor sans circulaire de dépendance
- Extension future (triggers → pipelines, CLI, dashboard) sans modifier `apollia-oria`

**À surveiller :**
- La migration `006_pipeline_tables.sql` est partagée entre `apollia-pipelines` et `apollia-tools` (le fichier est dans `apollia-tools/migrations/` mais embarqué depuis `apollia-pipelines` via `include_str!`)
- Si `apollia-triggers` doit dispatcher vers `PipelineEngine`, il reçoit `Option<PipelineEngineHandle>` en config — si `None`, il log une erreur mais ne panic pas (STORY-117)

---

## Alternatives rejetées

| Option | Raison du rejet |
|---|---|
| Extension d'`apollia-oria` | Mélange orchestration LLM et workflow multi-agent — violation Principe #5 |
| Pipeline défini dans `apollia-runtime` | Crée une dépendance circulaire : runtime → pipelines → runtime |
| HITL via polling SQLite | Trop lent ; l'EventBus broadcast est O(1) et préserve la réactivité |
