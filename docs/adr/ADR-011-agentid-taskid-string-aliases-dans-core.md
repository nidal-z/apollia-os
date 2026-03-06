# ADR-011 — AgentId et TaskId comme type aliases String dans apollia-core

**Date :** 2026-03-05
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 1

---

## Contexte

`RuntimeEvent` (STORY-006) utilise deux identifiants omniprésents : `AgentId` (identifie un agent dans le Registry) et `TaskId` (identifie une tâche soumise). Ces types doivent être définis avant toute autre crate du workspace, car `apollia-core` est la seule crate sans dépendance workspace — toute autre localisation créerait un cycle de dépendances.

Deux questions se posent :
1. Où déclarer `AgentId` et `TaskId` ?
2. Sous quelle forme — alias de type ou newtype ?

## Décision

`AgentId` et `TaskId` sont déclarés dans `apollia-core/src/events.rs` comme alias de type simples :

```rust
pub type AgentId = String;
pub type TaskId  = String;
```

Ils sont re-exportés depuis `apollia-core/src/lib.rs` et disponibles dans tout le workspace via `use apollia_core::{AgentId, TaskId}`.

## Alternatives considérées

### Option A — Newtype `struct AgentId(String)` (rejetée pour Sprint 1)

**Pour :** Distingue `AgentId` de `TaskId` au niveau du type checker — impossible de passer l'un à la place de l'autre. Idiomatique en Rust.

**Contre :** Friction à la construction (`AgentId("agent-1".into())` vs `"agent-1".into()`). Nécessite d'implémenter `Display`, `From<String>`, `AsRef<str>`, `Deref` pour rester ergonomique. Sur-ingénierie au Sprint 1 où l'API interne n'est pas encore stabilisée.

**Statut :** Reporté. Migration possible sans impact binaire (les newtype wrappant `String` ont la même représentation mémoire).

### Option B — `uuid::Uuid` natif (rejetée)

**Pour :** UUIDs non ambigus, collision quasi-impossible.

**Contre :** Contraint tous les callers à dépendre de `uuid`. Or, `AgentId` peut être un slug lisible (`"devis-agent"`) dans les contextes de configuration, et un UUID v4 quand le runtime l'assigne. Contraindre le type au niveau du alias est prématuré.

**Statut :** Le runtime génère déjà des UUIDs v4 pour les tâches (`AIPTask.task_id`). La génération reste à la charge du caller, l'alias reste `String`.

### Option C — Déclarer dans `apollia-runtime` (rejetée)

**Pour :** Cohérence — `RuntimeEvent` est surtout utilisé par `apollia-runtime`.

**Contre :** Crée un cycle : `apollia-core` ← `apollia-runtime` ← `apollia-core`. Interdit par le graphe de dépendances du workspace.

### Option retenue — Alias String dans apollia-core

**Pour :** Zéro friction. Pas de cycle. `apollia-core` reste la fondation stable. Migration vers newtype possible à tout moment.

**Compromis acceptés :** Pas de distinction de type entre `AgentId` et `TaskId` au niveau du compilateur — une confusion est théoriquement possible. Acceptable à ce stade car le nombre de call sites est faible et les noms de variables sont explicites.

## Conséquences

**Positives :**
- `RuntimeEvent` et ses dépendances compilent sans cycle.
- Construction triviale depuis n'importe quelle `&str` ou `String`.
- Cohérent avec le pattern déjà utilisé pour `AIPTask.task_id: String`.

**Négatives / Compromis :**
- Le compilateur ne distingue pas `AgentId` de `TaskId` — un bug de confusion reste possible.
- Les aliases de type ne permettent pas d'implémenter des traits spécifiques (ex: `Display` custom).

**Neutres / À surveiller :**
- Si les bugs de confusion `AgentId`/`TaskId` apparaissent en pratique (Sprint 2+), migrer vers newtype. La migration est mécanique et non-breaking au niveau binaire.

## Principes architecturaux impactés

- Principe #4 — Fail fast : les alias simples ne permettent pas de détecter les confusions d'identifiants à la compilation. Compromis explicitement accepté jusqu'à Sprint 3.
- Principe #5 — Un acteur, une responsabilité : `apollia-core` reste le seul point de définition des types partagés.

## Liens

- Story associée : STORY-006 (EventBus broadcast Tokio)
- ADR précédent : ADR-001 (Rust + Tokio)
- Fichier impacté : `crates/apollia-core/src/events.rs`
