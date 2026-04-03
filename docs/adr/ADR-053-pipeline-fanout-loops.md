# ADR-053 — Pipeline fan-out et boucles conditionnelles

**Date :** 2026-04-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 34 — Beta Hardening

---

## Contexte

Le Pipeline Engine (Sprint 15, ADR-025) supporte les topologies DAG linéaires : les steps s'exécutent
séquentiellement ou en branches parallèles fixes définies dans le TOML. Deux topologies manquent :

1. **Fan-out sur tableau** : un step produit une liste, et chaque élément doit être traité par un
   sous-step dédié en parallèle. Exemple : analyser 10 fichiers simultanément.

2. **Boucles conditionnelles** : un step peut nécessiter plusieurs passes avant de satisfaire une
   condition de sortie. Exemple : un agent de révision qui itère jusqu'à convergence.

Ces deux patterns sont bloquants pour les cas d'usage PME avancés identifiés dans l'audit beta.
STORY-444 les implémente. Cette ADR formalise les décisions d'architecture.

Les contraintes principales sont :
- Principe #5 (Un acteur, une responsabilité) — pas de logique d'orchestration cross-acteurs
- Principe #7 (Garde-fous non-négociables) — pas de boucles infinies possibles
- Le DAG statique existant ne doit pas être modifié rétroactivement

---

## Décision

### 1. Fan-out via `tokio::JoinSet`

Le fan-out est implémenté comme expansion dynamique d'un step sur un tableau de valeurs de sortie.
Quand un step déclare `fan_out = true` dans sa définition TOML, son output est interprété comme un
tableau JSON. Le Pipeline Executor crée un sous-step pour chaque élément via `tokio::JoinSet`.

```toml
[[pipeline.steps]]
id          = "list-files"
agent       = "file-lister"
fan_out     = true            # output attendu : tableau JSON

[[pipeline.steps]]
id          = "process-file"
agent       = "file-processor"
depends_on  = ["list-files"]  # reçoit chaque élément individuellement
```

Les sous-steps du fan-out sont **éphémères** — ils n'existent pas dans le DAG statique de la
définition TOML. Ils sont créés à l'exécution et détruits à leur completion. Le cycle detector
existant n'est pas affecté car il s'applique uniquement au DAG statique.

La concurrence du fan-out est bornée par `pipeline.max_fan_out_concurrency` (défaut : 8) pour
éviter la saturation du runtime sur de grands tableaux.

### 2. Boucles avec compteur maximum configurable

Les boucles sont déclarées via `loop_until` et `max_iterations` sur un step :

```toml
[[pipeline.steps]]
id             = "review-loop"
agent          = "reviewer-agent"
loop_until     = "output.approved == true"   # condition de sortie (JSONPath)
max_iterations = 5                            # garde-fou obligatoire
```

Comportement :
- Si `loop_until` est vrai à la première passe : 1 seule exécution, condition satisfaite.
- Si `max_iterations` est atteint sans satisfaction : le step transite en `StepRunStatus::Cancelled`
  avec `cancel_reason = "max_iterations_reached"`. La pipeline peut continuer ou échouer selon
  `on_cancel` (défaut : `fail`).

**Les boucles infinies sont impossibles par construction** — `max_iterations` est obligatoire si
`loop_until` est présent. L'absence de `max_iterations` avec `loop_until` est une erreur de
validation au démarrage (Principe #4).

### 3. `StepRunStatus::Cancelled`

La nouvelle variante `Cancelled { reason: String }` est ajoutée à `StepRunStatus`. Elle est
distincte de `Failed` : un step annulé a terminé proprement (pas d'exception), mais n'a pas
satisfait sa condition de sortie. L'audit trail enregistre `cancel_reason` pour la traçabilité.

### 4. Step timeout configurable

Orthogonalement aux boucles, chaque step peut déclarer un timeout maximum :

```toml
[[pipeline.steps]]
id             = "slow-agent"
timeout_secs   = 120    # 120s max, défaut depuis apollia.toml [pipelines] step_timeout_secs
```

Si le timeout expire : `StepRunStatus::Cancelled { reason: "timeout" }`. Implémenté via
`tokio::time::timeout` wrappant l'appel au step.

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| **Exécution séquentielle du fan-out** | Defeat le but — analyser 100 fichiers séquentiellement sur un modèle local à 2 tok/s = des heures. |
| **Framework de workflow externe (Temporal, Airflow)** | Viole Principe #2. Dépendance externe lourde. Le Pipeline Engine existant couvre 95% des cas sans framework dédié. |
| **Boucles infinies avec signal d'arrêt** | Impossible à borner statiquement — un agent peut ignorer le signal. Principe #7 exige un garde-fou non-contournable. |
| **Expansion des nœuds de boucle dans le DAG statique** | Complexité excessive — le cycle detector devrait être étendu pour différencier les cycles intentionnels des cycles erreur. Les nœuds éphémères sont plus simples. |
| **Nœuds dynamiques dans le graphe persisté** | La persistance du run state dans SQLite stocke les steps définis statiquement. Des nœuds dynamiques impliqueraient un schéma flexible difficile à migrer. |

---

## Conséquences

**Positives :**
- Fan-out sur `tokio::JoinSet` : parallélisme natif Tokio sans thread pool externe.
- Boucles conditionnelles : les cas d'usage "retry until convergence" sont couverts.
- `StepRunStatus::Cancelled` : distinction propre entre échec et annulation intentionnelle.
- Garde-fous non-contournables : `max_iterations` obligatoire, `max_fan_out_concurrency` borné.

**Négatives / Compromis :**
- Le fan-out dynamique ajoute de la complexité au Pipeline Executor — les sous-steps éphémères
  doivent être agrégés avant de débloquer les steps dépendants.
- Les boucles augmentent le temps d'exécution d'une pipeline de façon potentiellement non-linéaire —
  à documenter dans le wiki (Briques-Pipelines).
- Risque de deadlock si deux steps avec `depends_on` circulaire se retrouvent dans un fan-out —
  le cycle detector statique ne couvre pas ce cas. Documenté comme limitation V1.

**Neutres / À surveiller :**
- L'agrégation des résultats de fan-out (liste de sorties) doit être déterministe pour l'audit trail.
  Utiliser l'ordre d'index dans le tableau source, pas l'ordre de completion.
- `max_fan_out_concurrency` est un paramètre global — évaluer si un paramètre par-step est nécessaire.

---

## Principes architecturaux impactés

- **Principe #5 — Un acteur, une responsabilité** : Le Pipeline Executor reste le seul orchestrateur.
  Les sous-steps de fan-out sont gérés dans son `JoinSet` interne, pas dans un acteur séparé. Conforme.
- **Principe #7 — Garde-fous non-négociables** : `max_iterations` obligatoire avec `loop_until`.
  `max_fan_out_concurrency` borné. Validation au démarrage. Renforcé.
- **Principe #4 — Fail fast** : Configuration de boucle invalide (`loop_until` sans
  `max_iterations`) → erreur de validation au démarrage, pas à l'exécution. Conforme.

---

## Liens

- Story d'implémentation : STORY-444 (Sprint 34)
- Hardening pipelines préalable : STORY-443 (Sprint 34)
- Config exposée dans : STORY-435 (Sprint 34)
- Implémenté dans : `crates/apollia-pipelines/src/executor.rs`, `crates/apollia-pipelines/src/types.rs`
- ADR fondateur Pipelines : [ADR-025](ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md)
