---
name: apollia-story
description: Créer, affiner et valider des User Stories pour le projet Apollia OS (runtime Rust pour agents IA autonomes). Utilise ce skill systématiquement quand l'utilisateur mentionne "story", "user story", "US", "tâche à implémenter", "sprint", "fonctionnalité à coder", ou demande de décomposer un epic en stories. Ce skill garantit que chaque story est directement implémentable par Claude Code sans ambiguïté, avec des critères d'acceptation testables, des contraintes Rust/PyO3 respectées, et une alignment parfaite avec les 8 principes architecturaux d'Apollia OS.
---

# Apollia OS — Skill de Création de User Stories

Ce skill produit des User Stories directement actionnables par Claude Code pour le projet Apollia OS (runtime Rust open-source pour agents IA autonomes souverains).

## Contexte projet à toujours garder en tête

- **Stack** : Rust + Tokio (runtime), PyO3 + pyo3-async-runtimes (bridge Python), SQLite + FTS5 (mémoire), axum (API), clap v4 (CLI)
- **Architecture** : 6 crates dans un workspace Cargo (`apollia-core`, `apollia-runtime`, `apollia-oria`, `apollia-tools`, `apollia-memory`, `apollia-aip`, `apollia-cli`)
- **Principes non-négociables** : local-first, zéro dépendance externe, fail fast, acteurs Tokio isolés, mémoire à initiative agent, garde-fous non-négociables

Lire `references/architecture-summary.md` pour les détails complets avant de créer une story touchant à une brique spécifique.

---

## Workflow de création d'une story

### Étape 1 — Identifier le contexte

Avant d'écrire quoi que ce soit, déterminer :
1. **Quelle brique** est concernée ? (core / runtime / oria / tools / memory / aip / cli)
2. **Quel sprint** ? (0=types, 1=acteurs, 2=outils, 3=mémoire, 4=bridge, 5=API+CLI, 6=hardening)
3. **Dépendances** : quelles stories doivent être terminées avant celle-ci ?
4. **Estimation** : S (< 2h), M (2-4h), L (4-8h), XL (> 8h, à découper)

### Étape 2 — Appliquer le template

Utiliser **exactement** le template défini dans `references/story-template.md`.

### Étape 3 — Valider la story

Passer la checklist de validation avant de livrer :

**Clarté pour Claude Code :**
- [ ] Le fichier cible (`crates/apollia-XXX/src/xxx.rs`) est explicitement mentionné
- [ ] Les types Rust nécessaires sont nommés (pas juste décrits)
- [ ] Les dépendances Cargo requises sont listées si nouvelles
- [ ] Aucun critère d'acceptation subjectif ("c'est bien", "c'est rapide")

**Alignment architectural :**
- [ ] Zéro dépendance externe introduite sans justification
- [ ] Pattern acteur Tokio respecté si brique runtime
- [ ] Pas d'état partagé entre acteurs (pas de `Arc<Mutex<T>>` cross-acteurs)
- [ ] `thiserror` pour les erreurs (pas `anyhow` en lib)

**Testabilité :**
- [ ] Au moins 1 test unitaire `#[tokio::test]` défini dans les AC
- [ ] Cas d'erreur couverts (pas seulement le happy path)
- [ ] Test d'intégration requis si story touche à 2+ crates

**Cohérence sprint :**
- [ ] La story ne dépend pas de code du sprint suivant
- [ ] Le livrable est démo-able à la fin de la story (pas "sera visible dans 3 stories")

### Étape 4 — Générer les fichiers

Produire systématiquement :
1. `STORIES/sprint-N/story-NNN-titre-court.md` — la story complète
2. Une mise à jour de `STORIES/sprint-N/index.md` — ajout de la story dans l'index du sprint

---

## Règles de rédaction

### Titre
Format : `[SPRINT-N][CRATE] Verbe + Objet concret`
Exemples corrects :
- `[SPRINT-1][RUNTIME] Implémenter AgentRegistry comme acteur Tokio`
- `[SPRINT-4][AIP] Charger un module Python via PyO3 et valider l'AIP`

### User Story
Format strict : `En tant que [runtime / développeur d'agent / opérateur CLI], je veux [action], afin de [bénéfice mesurable].`

Ne pas utiliser "utilisateur" générique — être précis sur le persona :
- `runtime` = le code Rust lui-même qui a besoin d'une capacité
- `développeur d'agent` = celui qui écrit un agent Python
- `opérateur CLI` = celui qui gère le runtime via terminal

### Critères d'Acceptation
Format BDD strict :
```
ÉTANT DONNÉ [état initial précis]
QUAND [action ou événement]
ALORS [résultat vérifiable en test]
```

Chaque ALORS doit être vérifiable par `assert!` ou `assert_eq!` dans un test Rust.

### Definition of Done
Toujours inclure ces items fixes + items spécifiques à la story :
- [ ] `cargo test -p apollia-XXX` passe
- [ ] `cargo clippy -p apollia-XXX -- -D warnings` propre
- [ ] `cargo fmt --check` propre
- [ ] Pas de `unwrap()` dans le code de production (uniquement dans les tests)
- [ ] Docstring sur chaque fonction/struct publique
- [ ] [items spécifiques à la story]

---

## Sizing guide

| Taille | Durée estimée | Caractéristiques |
|--------|--------------|-----------------|
| S | < 2h | Un seul type/struct/enum, tests unitaires simples |
| M | 2-4h | Un acteur complet OU une intégration entre 2 types |
| L | 4-8h | Un composant complet avec tests d'intégration |
| XL | > 8h | TOUJOURS découper en 2+ stories M/L |

En contexte soir/weekend (8-10h/semaine), viser des stories S et M. Une story L = une session de travail complète.

---

## Référence rapide des erreurs courantes

**À ne pas faire :**
- Story sans fichier cible explicite → Claude Code ne sait pas où écrire
- AC qui dit "doit être performant" → non testable, non actionnable
- Dépendance circulaire entre stories du même sprint
- Introduire `tokio::sync::Mutex` pour partager l'état entre acteurs → violation principe #5
- Oublier le cas `ProcessState::Degraded` quand un outil optionnel est absent

**Patterns Rust attendus dans les stories runtime :**
```rust
// Pattern acteur Tokio standard (à référencer dans les stories)
pub struct MonActeur { /* état privé */ }
pub struct MonActeurHandle { tx: mpsc::Sender<Message> }
impl MonActeur {
    pub fn spawn(bus: EventBusSender) -> MonActeurHandle { ... }
    async fn run(mut self, mut rx: mpsc::Receiver<Message>) { ... }
}
```

---

## Référence des fichiers

- `references/architecture-summary.md` — résumé machine-readable de l'architecture complète
- `references/story-template.md` — template de story à remplir
- `references/sprint-index.md` — état actuel de tous les sprints et stories
- `references/rust-patterns.md` — patterns Rust/Tokio attendus par brique
