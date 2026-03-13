# ADR-026 — Observabilité complète : persistance input/output SQLite, timeline unifiée, troncature configurable

**Date :** 2026-03-13
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 13

---

## Contexte

Après 12 sprints, Apollia OS trace les statuts de tâches et les appels d'outils
(audit trail Sprint 2, STORY-016), mais les données d'exécution restent
fragmentées :

- Les tâches enregistrent leur `TaskStatus` final mais pas leur input complet,
  leur output, ni la durée réelle d'exécution.
- Les steps du Mode Orchestré (Sprint 10) sont exécutés et persistés avec un
  champ `output` et `error`, mais pas l'input rendu après substitution template,
  ni l'outil effectivement utilisé (distinct de `tool_hint`), ni la durée en ms.
- Les appels LLM (Sprint 8) émettent `LlmCallCompleted` sur l'EventBus mais
  rien ne le persiste — impossible de calculer les coûts a posteriori ou de
  débugger un prompt ayant produit un plan défaillant.
- Les triggers (Sprint 9) loguent leur activation mais pas le payload entrant
  complet ni le temps de dispatch (latence réception → soumission tâche).
- Les approbations HITL (Sprint 11) enregistrent `approved`/`reason` et
  `responded_at`, mais pas le timestamp exact de suspension ni la durée
  d'attente humaine en millisecondes.

Cinq décisions structurantes encadrent la résolution de ces lacunes.

### Contraintes

- **Principe #1 — Local-first** : toute donnée d'observabilité doit rester en
  SQLite local, zéro envoi externe sans action explicite.
- **Principe #2 — Zéro dépendance externe** : pas d'outil de migration SQL
  externe (sqlx-migrate, refinery). Le projet utilise `rusqlite` inline.
- **Principe #4 — Fail fast** : si la DB ne s'ouvre pas au démarrage, erreur
  fatale immédiate.
- **ADR-002** : SQLite est le seul moteur de persistance.
- Les prompts LLM peuvent contenir des données personnelles (RGPD).
- Les inputs/outputs peuvent atteindre des centaines de KB (prompts longs,
  sorties bash volumineuses).

---

## Décision

Nous adoptons cinq décisions complémentaires pour l'observabilité complète.

### Décision 1 — Extensions de schéma dans le code Rust existant

Les `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` sont ajoutés dans les fonctions
d'initialisation SQLite existantes de chaque crate (`audit.rs`, `task_repository.rs`,
`plan_repository.rs`, `persistence.rs`). La nouvelle table `llm_calls` est
créée via `CREATE TABLE IF NOT EXISTS` dans un nouveau `repository.rs` de
`apollia-llm`.

### Décision 2 — Persistance SQLite avec troncature configurable

Tous les inputs, outputs, stdout et stderr sont persistés en SQLite avec une
troncature configurable via `ObservabilityConfig` :

```rust
pub struct ObservabilityConfig {
    pub max_input_bytes: usize,       // défaut 32768 (32KB)
    pub max_output_bytes: usize,      // défaut 32768 (32KB)
    pub max_tool_output_bytes: usize, // défaut 10240 (10KB)
    pub debug_log_prompt: bool,       // défaut false
}
```

Pattern de troncature : si `text.len() > max_bytes`, tronquer sur une frontière
UTF-8 valide et suffixer `"\n[TRONQUÉ — N octets total]"`. Un champ
`*_truncated INTEGER NOT NULL DEFAULT 0` accompagne chaque champ texte tronqué.

### Décision 3 — Timeline API comme vue agrégée côté serveur

`GET /api/v1/tasks/{id}/timeline` retourne un `Vec<TimelineEvent>` JSON trié
par timestamp croissant. Le handler lit 5 sources en parallèle (tasks,
plan_steps, llm_calls, tool_invocations, task_approvals), construit un vecteur
de tuples `(timestamp, event)`, trie par timestamp ASC, et sérialise.

### Décision 4 — `prompt_text` LLM nullable, conditionnel à `debug_log_prompt`

La table `llm_calls` a un champ `prompt_text TEXT` nullable. Par défaut
(`debug_log_prompt = false`), ce champ est `NULL`. L'opérateur doit
explicitement activer `debug_log_prompt = true` dans `[observability]` de
`apollia.toml` pour que les prompts soient persistés.

### Décision 5 — Troncature avec marqueur plutôt qu'erreur

Quand un input, output ou prompt dépasse la limite configurée, le runtime
tronque et ajoute un marqueur plutôt que de rejeter l'enregistrement.
L'observabilité partielle est préférable à l'absence d'observabilité.

---

## Alternatives considérées

### Alt 1 — Fichiers `.sql` séparés avec outil de migration (rejetée)

**Pour :** Séparation claire du schéma SQL, tooling mature (sqlx-migrate,
refinery), versioning explicite des migrations.

**Contre :** Le projet utilise `rusqlite` inline depuis le Sprint 0. Introduire
un outil de migration ajoute une dépendance et un paradigme non établi. Les
6 crates qui gèrent du SQLite utilisent toutes `CREATE TABLE IF NOT EXISTS`
dans des fonctions d'initialisation Rust. Changer ce pattern pour un seul
sprint est disproportionné. `ALTER TABLE ADD COLUMN IF NOT EXISTS` est
idempotent et s'intègre naturellement dans les fonctions existantes.

### Alt 2 — Stockage fichier pour les inputs/outputs volumineux (rejetée)

**Pour :** Pas de limite de taille, pas de troncature nécessaire, SQLite reste
léger.

**Contre :** Fragmente l'audit trail entre SQLite et le filesystem. Les requêtes
cross-données (timeline, dashboard) nécessitent des lectures fichier + JOIN
logique. La suppression d'un fichier orpheline les références SQLite. Le backup
(copie d'un `.db`) ne suffit plus. Le pattern complique la Timeline API qui
doit agréger 5 sources — ajouter des fichiers comme 6ème source est
disproportionné.

### Alt 3 — Client agrège N requêtes parallèles pour la timeline (rejetée)

**Pour :** API plus simple côté serveur (5 endpoints séparés), chaque endpoint
ne fait qu'une chose.

**Contre :** Le client ne peut pas garantir la cohérence temporelle entre les
appels parallèles (une tâche qui progresse pendant les appels produit des
données incohérentes). Chattiness réseau inutile (5 requêtes au lieu d'une).
L'intégrateur (dashboard HTMX, CLI) doit re-implémenter la logique de merge +
tri à chaque consumer.

### Alt 4 — Toujours persister les prompts LLM (rejetée)

**Pour :** Debugging simple, pas de configuration à gérer.

**Contre :** Les prompts LLM contiennent potentiellement des données
personnelles en clair (noms, emails, montants). Les persister par défaut dans
SQLite non chiffré est problématique RGPD. Un agent traitant des factures
mémoriserait tous les prompts contenant les données client. Le défaut doit
être sûr — l'opt-in est préférable.

### Alt 5 — Rejeter l'enregistrement si l'input dépasse la limite (rejetée)

**Pour :** Pas de troncature = pas de perte de données, alertes claires quand
les limites sont atteintes.

**Contre :** L'opérateur perd toute visibilité sur les tâches avec des
inputs/outputs volumineux. L'absence d'observabilité est pire que
l'observabilité partielle. Le marqueur `[TRONQUÉ — N octets total]` signale
explicitement la perte sans silencer le problème.

---

## Conséquences

**Positives :**
- Zéro boîte noire : chaque action d'agent est traçable a posteriori
- Coûts LLM calculables par backend/modèle sur une période donnée
- Timeline unifiée en un seul appel API, ordonnée chronologiquement
- Troncature protège SQLite des données volumineuses sans perdre la traçabilité
- Prompts LLM protégés par défaut (RGPD-compatible)

**Négatives / Compromis :**
- Taille des bases SQLite augmente (inputs/outputs persistés) — mitigation par
  troncature à 32KB max par champ
- `apollia-llm` acquiert une dépendance `rusqlite` (était stateless avant ce sprint)
- 6 `ALTER TABLE` appliqués à chaque redémarrage (idempotents, coût ~0ms)
- `prompt_text = NULL` par défaut rend le debugging LLM moins direct — nécessite
  `debug_log_prompt = true` explicite

**Neutres / À surveiller :**
- Rotation/archivage des vieilles données d'observabilité (tables illimitées) —
  à adresser dans un sprint futur si les DB deviennent trop volumineuses
- La Timeline API fait 5 requêtes SQLite séquentielles (une par DB) — si la
  latence devient un problème, envisager un cache ou une DB consolidée
- `rusqlite` 0.32 bundle SQLite 3.45+ qui supporte `ALTER TABLE ADD COLUMN
  IF NOT EXISTS` — vérifier en cas de downgrade rusqlite

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : respecté — toutes les données d'observabilité
  restent dans les fichiers SQLite locaux (`audit.db`, `hitl.db`, `plans.db`,
  `triggers.db`, nouveau `llm.db`)
- **Principe #2 — Zéro dépendance externe** : respecté — pas d'outil de
  migration externe, `rusqlite` inline existant
- **Principe #4 — Fail fast** : respecté — si une DB ne s'ouvre pas, erreur
  fatale au démarrage ; troncature plutôt qu'erreur silencieuse
- **Principe #8 — CLI humaine, API machine** : respecté — Timeline API retourne
  du JSON structuré avec `--json` compatible, dashboard HTMX pour l'humain

---

## Liens

- Stories associées : STORY-125 à STORY-134
- ADR précédents : ADR-002 (SQLite seul moteur), ADR-020 (apollia-llm EventBus),
  ADR-021 (triggers TOML persistence), ADR-023 (HITL task_approvals)
- Spec détaillée : `docs/specs/sprint-13-spec.md`
