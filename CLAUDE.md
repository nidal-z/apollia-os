# Apollia OS — Contexte pour Claude Code

> Ce fichier est lu automatiquement par Claude Code à chaque session.
> Il contient les règles non-négociables et le contexte projet.

---

## Projet

**Apollia OS** est un runtime Rust open-source pour l'exécution souveraine d'agents IA autonomes. Il permet à n'importe quel agent Python (LangGraph, CrewAI, custom) de s'exécuter de manière isolée, locale, et outillée — sans dépendance cloud.

**Auteur :** Nidal — CTO & Co-fondateur Apollia  
**Développement :** soir/weekend, 8-10h/semaine  
**Phase actuelle :** Sprint book-rewrite livré (20/20 stories — réécriture complète du book mdBook, structure pédagogique "The Rust Book", chapitres ch01–ch19 + annexes A-F). Sprint 32 livré précédemment (A2A complet + Distribution locale + Worker Agents communautaires, 8/8 stories, ADR-050).

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
├── apollia-core/          ← types partagés, dépendance de tout le reste
├── apollia-runtime/       ← Runtime Core (acteurs Tokio, API, EventBus, Supervisor)
├── apollia-oria/          ← ORIA Engine (Observer-Reasoner-Actor, StepBudget, ResilienceLayer)
├── apollia-tools/         ← Tool Registry + outils natifs + sandbox + audit trail
├── apollia-memory/        ← Memory Engine (SQLite, FTS5, épisodique/sémantique/procédurale)
├── apollia-aip/           ← Bridge PyO3 (Rust ↔ Python async, RuntimeContext)
├── apollia-llm/           ← LLM Backend (llama.cpp local, Anthropic, OpenAI, Ollama, LlmRouter)
├── apollia-mcp/           ← Client MCP natif (JSON-RPC 2.0, stdio/HTTP/SSE, McpClientManager)
├── apollia-triggers/      ← Trigger Engine (cron, interval, filewatch, webhook, hot reload)
├── apollia-notifications/ ← Notification Engine (desktop notify-rust, webhook reqwest)
├── apollia-pipelines/     ← Pipeline Engine (topologie DAG, fan-out/fan-in, HITL, fallback)
├── apollia-stt/           ← Speech-to-Text (trait SttBackend, whisper-rs, audio pipeline)
├── apollia-desktop/       ← Application Desktop (Tauri v2 + Svelte 5, 114 commandes IPC, 15 routes)
└── apollia-cli/           ← Binaire final (clap, 13 sous-commandes)
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

**Convention book/wiki (sprint book-wiki-separation) :**
- `book/src/` = contenu pédagogique ("The Rust Book") — apprendre en faisant, exemples concrets, 1-2 patterns
- `docs/wiki/` = référence technique exhaustive ("docs.rs") — specs complètes, tables de paramètres, codes d'erreur
- Le book explique le concept et montre 1-2 exemples → lien wiki pour les specs complètes
- **Règle absolue : le book ne duplique JAMAIS une table de référence présente dans le wiki**
- Pattern de lien obligatoire : `> **Référence technique :** [Nom-Page](https://github.com/nidal-z/apollia-os/wiki/Nom-Page)`

---

## Skills disponibles

Ces skills sont actifs dans ce projet. Les utiliser systématiquement :

- **apollia-story** : Créer/affiner une User Story → `docs/internal/STORIES/sprint-N/story-NNN.md`
- **apollia-sprint** : Planifier/clôturer un sprint → `docs/internal/STORIES/sprint-N/plan.md`
- **apollia-adr** : Documenter une décision architecturale → `docs/adr/ADR-NNN.md`
- **apollia-doc-setup** : Initialiser docs/ + book/ mdBook (usage unique, première fois)
- **apollia-doc-sync** : Mettre à jour la doc après sprint/story/changement architectural/diagramme
- **apollia-doc-research** : Veille technologique interne (MCP/A2A, concurrents, signaux pivot)

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
| Stories et sprints | `docs/internal/STORIES/sprint-index.md` |
| Décisions (ADR) | `docs/Decisions-Log.md` |

---

## État courant

**Dernier sprint livré :** Sprint 40 — Context Bootstrapping & SDK 0.3.0 : protocole ContextBootstrap (classe abstraite SDK), ProjectContextBootstrap partagé, adoption dans les 4 assistants (spec/dev/review/document), recall_entry()/recall_all() exposés en Python, AgentManifestDict v2, ConversationalAgent stub, tests d'intégration bootstrap (6/6 stories, ADR-070, ADR-071). ✅
**Sprint précédent :** Sprint 39 — Agents qui travaillent : 4 assistants opérationnels (spec/dev/review/document), memory namespace project-scoped, restructuration agents/, smoke tests (7/7 stories, ADR-070). ✅
**Avant :** Sprint 37 — Parité complète TypeScript (OAuth2 PKCE, auto-updater, code review agent, mDNS MCP, hot reload MCP, HITL MCP SQLite, memory export/import, purge configurable, OnBusyPolicy::Queue, filtrage notifs, templates pipeline, CUDA CI, Bedrock, Vertex AI, Notebook tool, 15/15 stories, ADR-064→068). ✅
**MVP validé :** start → agent start → run → stop fonctionne E2E (mars 2026)
**Dernière décision :** ADR-072 — Outils web natifs : architecture 2-étages `web_search` (trait `SearchBackend` pluggable, DuckDuckGo default + Brave feature-gated) et `web_read` (fetch + extraction `dom_smoothie`, SSRF-guarded). Opt-in via `apollia.toml`. Bloc 1.3 du LAUNCH-BACKLOG livré.

**Book mdBook :** structure pédagogique complète dans `book/src/` — ch01 (Premiers pas) → ch19 (CLI) + annexes A-F. Build propre : `mdbook build book`. Sources des chapitres : `docs/wiki/`. Convention : book = apprendre, wiki = référence (voir règle Documentation ci-dessus).

Pour l'état détaillé : lire `docs/internal/STORIES/sprint-index.md`.