# Apollia OS — Contexte pour Claude Code

> Ce fichier est lu automatiquement par Claude Code à chaque session.
> Il contient les règles non-négociables et le contexte projet.

---

## Projet

**Apollia OS** est un runtime Rust open-source pour l'exécution souveraine d'agents IA autonomes. Il permet à n'importe quel agent Python (LangGraph, CrewAI, custom) de s'exécuter de manière isolée, locale, et outillée — sans dépendance cloud.

**Auteur :** Nidal — CTO & Co-fondateur Apollia  
**Développement :** soir/weekend, 8-10h/semaine  
**Phase actuelle :** Sprint 0 — Fondations

---

## Stack technique

| Composant | Technologie |
|---|---|
| Runtime async | Rust + Tokio 1.x |
| Bridge Python | PyO3 + pyo3-async-runtimes |
| Persistance | SQLite + rusqlite + FTS5 |
| API locale | axum (Unix socket + TCP 7771) |
| CLI | clap v4 derive |
| Sérialisation | serde + serde_json |
| Erreurs | thiserror (libs) — anyhow INTERDIT dans le workspace |
| Logging | tracing + tracing-subscriber |

---

## Structure workspace

```
crates/
├── apollia-core/     ← types partagés, dépendance de tout le reste
├── apollia-runtime/  ← Runtime Core (acteurs Tokio, API, EventBus)
├── apollia-oria/     ← ORIA Engine (Observer-Reasoner-Actor)
├── apollia-tools/    ← Tool Registry + outils natifs + sandbox
├── apollia-memory/   ← Memory Engine (SQLite, FTS5)
├── apollia-aip/      ← Bridge PyO3 (Rust ↔ Python async)
└── apollia-cli/      ← Binaire final (clap)
```

---

## 8 Principes non-négociables

Ces principes ne peuvent pas être violés sans créer un ADR explicite.

1. **Local-first** : Zéro octet de données utilisateur ne quitte la machine sans action explicite
2. **Zéro dépendance externe** : Le binaire fonctionne sur tout Linux sans installation préalable
3. **Contrat minimal** : Duck typing Python — `manifest()` + `run()` async suffisent
4. **Fail fast** : Toute erreur détectable au démarrage est détectée au démarrage
5. **Un acteur, une responsabilité** : Pattern acteur Tokio, zéro état partagé entre acteurs
6. **Mémoire à initiative de l'agent** : Jamais d'injection automatique de contexte mémoriel
7. **Garde-fous non-négociables** : StepBudget appliqué par le runtime, non contournable
8. **CLI humaine, API machine** : `--json` global, TTY auto-détecté

---

## Règles d'implémentation absolues

### Rust
- `thiserror` pour toutes les erreurs dans les crates du workspace — `anyhow` INTERDIT
- Zéro `unwrap()` dans le code de production — uniquement dans les tests
- Zéro `todo!()` dans le code de production avant de committer
- Pattern acteur Tokio strict : `mpsc::channel` + handle clonable — jamais `Arc<Mutex<T>>` cross-acteurs
- `tracing::info!(champ = %val, "event")` — jamais de format string dans les logs
- Docstring `///` sur chaque struct, enum, fn publique

### Git
- Commits conventionnels obligatoires : `feat(apollia-core): add AgentManifest type`
  - Préfixes : `feat` `fix` `refactor` `test` `docs` `chore` `perf`
  - Scope = nom de la crate : `apollia-core`, `apollia-runtime`, etc.
- Un commit = une story ou une sous-tâche logique
- Jamais de commit avec `cargo test` qui échoue

### Tests
- `#[tokio::test]` pour tous les tests async
- Structure GIVEN / WHEN / THEN dans les commentaires
- Au moins 1 test du cas d'erreur par composant
- `cargo test -p apollia-XXX` doit passer avant chaque commit sur cette crate

### Documentation
- Décision architecturale significative → ADR dans `docs/Decisions-Log.md`
- Déviation par rapport à la spec → note dans la story concernée
- Jamais de TODO/FIXME dans le code — créer une story ou corriger maintenant

---

## Skills disponibles

Ces skills sont actifs dans ce projet. Les utiliser systématiquement :

- **apollia-story** : Créer/affiner une User Story → `STORIES/sprint-N/story-NNN.md`
- **apollia-sprint** : Planifier/clôturer un sprint → `STORIES/sprint-N/plan.md`
- **apollia-adr** : Documenter une décision architecturale → `docs/adr/ADR-NNN.md`

---

## Fichiers de référence

| Besoin | Fichier |
|---|---|
| Architecture complète | `docs/Architecture-Vue-Ensemble.md` |
| Principes détaillés | `docs/Architecture-Principes.md` |
| Spec Tool Registry | `docs/Briques-Tool-Registry.md` |
| Spec Memory Engine | `docs/Briques-Memory-Engine.md` |
| Spec ORIA Engine | `docs/Briques-ORIA-Engine.md` |
| Spec Runtime Core | `docs/Briques-Runtime-Core.md` |
| Spec CLI | `docs/Briques-CLI.md` |
| Stories et sprints | `STORIES/sprint-index.md` |
| Décisions (ADR) | `docs/Decisions-Log.md` |

---

## État courant

**Sprint actif :** Sprint 6 — Hardening + Agent de démo
**Dernier sprint livré :** Sprint 5 — APIServer + CLI complète (8/8 stories, 289 tests)
**Dernière décision :** ADR-018 — CLI Bootstrap sans Supervisor

Pour l'état détaillé : lire `STORIES/sprint-index.md`.