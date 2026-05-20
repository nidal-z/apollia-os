# Apollia OS — Contexte pour Claude Code

> Ce fichier est lu automatiquement par Claude Code à chaque session.
> Il contient les règles non-négociables et le contexte projet.

---

## Projet

**Apollia OS** est un runtime Rust open-source pour l'exécution souveraine d'agents IA autonomes. Il permet à n'importe quel agent Python (LangGraph, CrewAI, custom) de s'exécuter de manière isolée, locale, et outillée — sans dépendance cloud.

**Auteur :** Nidal — CTO & Co-fondateur Apollia  
**Développement :** 8-10h/jour jusqu'au 19 mai 2026 (phase release publique)  
**Phase actuelle :** Release publique v0.1.0 — plan jour par jour dans `docs/internal/WEEK-PLAN.md`. Tous les sprints feature (jusqu'au 43 inclus) et le sprint fact-checking sont livrés. Aucun nouveau sprint feature avant le 19 mai.

---

## Stack technique

| Composant | Technologie |
|---|---|
| Runtime async | Rust + Tokio 1.x |
| Bridge Python | PyO3 0.24 + pyo3-async-runtimes |
| LLM local | llama-cpp-2 (GGUF, Metal, CUDA) |
| STT local | whisper-rs (GGML) |
| Persistance | SQLite + rusqlite + FTS5 |
| API locale | axum (Unix socket + TCP 7771) |
| CLI | clap v4 derive |
| Sérialisation | serde + serde_json |
| Erreurs | thiserror (libs) — anyhow INTERDIT dans le workspace |
| Logging | tracing + tracing-subscriber |
| Desktop | Tauri v2 + Svelte 5 + Tailwind 3.4 |
| Design tokens | CSS custom properties HSL + ADR-077 |
| i18n | svelte-i18n v4 (FR/EN, JSON namespaces) |

---

## Structure workspace

```
crates/
├── apollia-aip/           ← Bridge PyO3 (Rust ↔ Python async, RuntimeContext exposé via stubs SDK)
├── apollia-auth/          ← OAuth2 PKCE, token keyring, auto-refresh (ADR-064 + ADR-089) — multi-account + singleflight refresh + Google/Microsoft providers
├── apollia-cli/           ← Binaire final (clap v4, 13+ sous-commandes, codes de sortie 0-5)
├── apollia-connectors/    ← Native SaaS connectors (ADR-088, ADR-090) — Google Workspace (Gmail/Calendar/Drive Workspace), Microsoft 365 (Outlook/Calendar/OneDrive) — trait Connector + HttpClient avec retry/refresh
├── apollia-core/          ← Types partagés (AgentId, TaskId, StepBudget) — dépendance de tout le reste
├── apollia-desktop/       ← Application Desktop (Tauri v2 + Svelte 5, ~114 commandes IPC, 15 routes)
├── apollia-llm/           ← LLM Backend (llama.cpp, Anthropic, OpenAI, Ollama, Vertex, LlmRouter)
├── apollia-mcp/           ← Client MCP natif (JSON-RPC 2.0, stdio/HTTP/SSE, McpClientManager)
├── apollia-memory/        ← Memory Engine (SQLite FTS5, épisodique/sémantique/procédurale, export/import)
├── apollia-notifications/ ← Notification Engine (desktop notify-rust, webhook reqwest)
├── apollia-oria/          ← ORIA Engine (Observer-Reasoner-Actor, StepBudget, ResilienceLayer, plan cache)
├── apollia-permissions/   ← Permission rules engine (prefix-scoped, 3 scopes : session/project/global)
├── apollia-runtime/       ← Runtime Core (acteurs Tokio, API axum, EventBus, Supervisor)
├── apollia-stt/           ← Speech-to-Text (trait SttBackend, whisper-rs, audio pipeline)
├── apollia-tools/         ← Tool Registry (13 outils natifs, sandbox, audit trail, governance.db)
├── apollia-triggers/      ← Trigger Engine (cron, interval, filewatch, webhook, hot reload SQLite)
└── apollia-workspace/     ← Workspace isolation, agent registry, project scoping, ContextProvider
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
- Décision architecturale significative → ADR dans `docs/adr/ADR-NNN.md` + entrée dans `docs/wiki/Decisions-Log.md`
- Déviation par rapport à la spec → note dans la story concernée
- Jamais de TODO/FIXME dans le code — créer une story ou corriger maintenant

### Skills A2A LLM-facing (SDK Apollia AgentKit)
- `Annotated[T, "description courte"]` sur les params LLM-facing à valeurs énumérées ou structure non-évidente (skip pour params triviaux numériques/booléens)
- `@skill(examples=[{...}])` : au moins 1 payload-modèle réaliste par skill, propagé au tool descriptor LLM-facing
- TypedDict canoniques dans `<agent>/schemas.py` pour remplacer `list[dict[str, Any]]` / `dict[str, Any]` opaques — **sans** `from __future__ import annotations` (PEP 563 casse `TypedDict.__required_keys__`)
- Validation : `python -m apollia inspect <agent.py> --json` doit montrer `description` sur chaque param + `examples` sur chaque skill + sous-schémas structurellement stricts
- Référence : `docs/internal/release/AGENTKIT-REBUILD-2026-05-19.md` (section "Post-rebuild — Optimisations LLM tool descriptors") + commits `bed9e212`, `48f6cd83`, `566b79a1`

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
- **apollia-doc-sync-diff** : Synchroniser book/wiki/help depuis une plage de commits git (via table de routage)
- **apollia-doc-research** : Veille technologique interne (MCP/A2A, concurrents, signaux pivot)

---

## Fichiers de référence

| Besoin | Fichier |
|---|---|
| Architecture complète | `docs/wiki/Architecture-Vue-Ensemble.md` |
| Principes détaillés | `docs/wiki/Architecture-Principes.md` |
| Spec Tool Registry | `docs/wiki/Briques-Tool-Registry.md` |
| Spec Memory Engine | `docs/wiki/Briques-Memory-Engine.md` |
| Spec ORIA Engine | `docs/wiki/Briques-ORIA-Engine.md` |
| Spec Runtime Core | `docs/wiki/Briques-Runtime-Core.md` |
| Spec CLI | `docs/wiki/Briques-CLI.md` |
| Stories et sprints | `docs/internal/STORIES/sprint-index.md` |
| Décisions (ADR) | `docs/wiki/Decisions-Log.md` · fichiers : `docs/adr/` |
| **SDK Python (Apollia AgentKit v0.5.0)** | `sdk/README.md` · décorateurs `@agent` / `@skill` / `@on_message` / `@orchestrated` · ADRs 098-112 |
| Planning launch v0.1.0 | `docs/internal/RELEASE-MOSCOW.md` (MoSCoW) · `docs/internal/release/PLAN-13-JOURS-2026-05-08.md` (séquençage 8 → 20 mai) · `docs/internal/release/BACKLOG-2026-05-08.md` (tâches détaillées) |
| Design system frontend | `docs/wiki/DESIGN-SYSTEM.md` |

---

## État courant — Phase release publique

**Mode :** Semaine de release publique — 8 mai au 20 mai 2026 (annonce mercredi 20 mai).
**Référence journalière :** `docs/internal/release/PLAN-13-JOURS-2026-05-08.md` (plan jour par jour, source de vérité). Le précédent `WEEK-PLAN.md` est archivé dans `docs/internal/archive/2026-04-30-WEEK-PLAN.md`.
**Release :** dimanche 18 mai (repo public + tag v0.1.0) · **Pré-launch soft :** lundi 19 mai · **Annonce officielle :** mercredi 20 mai.

**Pas de nouveau sprint feature avant la release.** Toutes les sessions suivent le PLAN-13-JOURS.

**Tâches release en cours (MoSCoW) :**
- M1 Distribution .dmg — CI prête, jamais testée
- M2 Updater in-app — Tauri plugin + Svelte (à implémenter)
- M3 Onboarding agent — mise à jour system prompt
- M4 Apollia Guide — réécriture knowledge base
- M5 Agents démo — veille-ia (nettoyage) + veille-rse (création) + email-triage (création)
- M6 Commentaires inline — nettoyage AI slop + références internes
- M7 Relecture corpus — gate non délégable (Jour 12)
- M8 Site vitrine — repasse UX/design
- M9 Cloudflare Pages — setup apollia.fr + docs.apollia.fr
- M10 Desktop UX/UI — Claude Design (asynchrone, par batch crédit) + Claude Code
- M11 Screenshots — pour corpus Help (post M10)

**Sprint fact-checking :** terminé (cargo test --workspace ✅, corpus book/wiki corrigés, Help délégué).
**Dernier sprint feature :** Sprint 40 ✅ — Context Bootstrapping & SDK 0.3.0 (2026-04-15).
**Dernière décision :** ADR-082 — Tool Governance : `governance.db` unifié, 3 scopes HITL.

**Documentation :** 3 corpus synchronisés
- `book/src/` : ch01–ch19 + annexes A-F (pédagogique, build : `mdbook build book/`)
- `docs/wiki/` : référence exhaustive — Architecture, Briques, API, Specs, ADRs (ADR-001–082)
- `docs/help/` : aide opérateur desktop (34 articles)
- SDK Python : `sdk/apollia/stubs/` (synchronized post-fact-checking FC12/FC13)

**Référence de travail quotidien :** `docs/internal/WEEK-PLAN.md` — source de vérité jusqu'au 19 mai.