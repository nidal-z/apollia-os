# Log des Décisions Architecturales

> *Chaque décision majeure documentée avec son contexte, les alternatives considérées, et la justification.*

---

## Format

```
## ADR-NNN — Titre court
**Date :** YYYY-MM-DD
**Statut :** Accepté | Remplacé par ADR-NNN | En discussion
**Contexte :** Pourquoi cette décision était nécessaire
**Décision :** Ce qui a été décidé
**Alternatives considérées :** Ce qui a été écarté et pourquoi
**Conséquences :** Impact architectural
```

---

## ADR-001 — Rust comme langage principal du runtime

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** Le runtime doit être distribué comme binaire unique sans dépendances système. Il doit aussi être performant (supervision d'acteurs async) et sûr (gestion de sandbox).

**Décision :** Rust + Tokio pour le runtime. Python uniquement pour les agents (via PyO3).

**Alternatives considérées :**
- Go : Bon pour les binaires uniques, mais écosystème async/acteurs moins mature que Tokio. PyO3 n'existe pas en Go.
- Python : Souffrirait du GIL, des dépendances système, et du packaging binaire unique.
- Node.js : Pas de vrai binaire unique, performances inférieures pour l'isolation sandbox.

**Conséquences :** Courbe d'apprentissage Rust pour les contributeurs. Compensée par les garanties de sécurité mémoire et les performances.

---

## ADR-002 — SQLite comme seul moteur de persistance

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** Le principe "zéro dépendance externe" interdit PostgreSQL, Redis, et Qdrant.

**Décision :** SQLite avec FTS5 (plein texte) et sqlite-vec optionnel (vectoriel). Un fichier `.db` par namespace mémoire.

**Alternatives considérées :**
- PostgreSQL : Nécessite un service séparé. Interdit par le principe #2.
- DuckDB : Moins adapté aux lectures/écritures fréquentes d'une mémoire d'agent.
- Fichiers JSON : Pas searchable, pas de TTL, pas de FTS.
- LanceDB : Moins mature que sqlite-vec, dépendance supplémentaire.

**Conséquences :** Concurrence limitée (WAL mode atténue), pas de recherche vectorielle sans modèle d'embedding (FTS5 compense pour les cas PME).

---

## ADR-003 — Duck typing pour l'AIP

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** L'AIP doit être adoptable avec un minimum de friction. Forcer une classe de base oblige les agents existants (LangGraph, CrewAI) à hériter d'une classe Apollia OS.

**Décision :** `manifest()` et `run()` async suffisent. La classe `AIPAgent` est optionnelle. Validation par `hasattr` + inspection des signatures.

**Alternatives considérées :**
- Classe de base obligatoire : Plus strict, mais friction à l'adoption.
- `typing.Protocol` Python : Élégant, mais nécessite que l'agent importe le protocole.
- Descripteur YAML/TOML séparé : Source de vérité dupliquée.

**Conséquences :** `AIPWrapper` nécessaire pour certains cas edge. Validation moins stricte au niveau du type checker.

---

## ADR-004 — Deux modes d'exécution ORIA (Direct + Orchestré)

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** La recherche sur RP-ReAct (2025) montre que le mode Reasoner-Planner est optimal pour les tâches complexes mais introduit overhead sur les tâches simples. 80% des cas PME sont atomiques.

**Décision :** Classification automatique à l'entrée de chaque tâche. Mode Direct pour les cas simples, Mode Orchestré pour les cas complexes.

**Alternatives considérées :**
- Mode unique Orchestré : Inefficace pour les cas simples.
- Mode unique Direct : Impossible pour les tâches réellement multi-step.
- Choix laissé à l'agent : Trop de configuration pour la cible PME.

**Conséquences :** Deux chemins de code à maintenir. Compensé par la résilience en production.

---

## ADR-005 — Sandbox multi-plateforme : Linux namespaces, macOS DevMode, Windows 3-couches Chromium

**Date :** 2026-03 (Linux + macOS) / 2026-04-03 (Windows)
**Statut :** Accepté

**Contexte :** Isolation d'exécution des outils natifs (bash, python) sans Docker (Principe #2). Le runtime doit fonctionner sur Linux/macOS/Windows sans installation préalable. Les APIs d'isolation natives diffèrent par plateforme.

**Décision :** (1) **Linux :** `unshare --pid --mount --fork` (PID + mount namespaces). Roadmap v0.2 → nsjail, v1.0 → gVisor optionnel. (2) **macOS :** `SandboxMode::Dev` — pas de sandbox réel (`sandbox-exec` deprecated depuis macOS 10.15, API privée non documentée). Exécution directe avec `tracing::warn!` à **chaque invocation**. Détecté à la compilation via `#[cfg(target_os = "linux")]`. CI Linux valide le chemin réel. (3) **Windows :** 3 couches Chromium — Job Object (terminaison auto + pas de dialog), Restricted Token (`CreateRestrictedToken`, suppression `SeDebugPrivilege` etc.), AppContainer (`apollia-sandbox-<agent_id>`, nettoyé après exécution). Dégradation gracieuse vers couches 1+2 si AppContainer échoue. Implémenté dans `sandbox_windows.rs` sous `#[cfg(target_os = "windows")]`.

**Alternatives considérées :** Docker (viole Principe #2), `sandbox-exec` macOS (deprecated — dette technique garantie), Warning au démarrage seulement (trop discret), WSL/Docker Windows (dépendances externes), Job Object seul sur Windows (insuffisant — ne couvre pas filesystem/réseau).

**Conséquences :** Isolation native sans dépendance externe. Pas d'isolation sur macOS dev (code de confiance — acceptable). AppContainer crée un profil persistant à nettoyer en cas de crash. User namespaces doivent être activés sur Linux (standard Linux 3.8+).

**Principes impactés :** Principe #2 — Zéro dépendance externe, Principe #4 — Fail fast (mode Dev visible), Principe #7 — Garde-fous non-négociables (sandbox toujours actif en production Linux).

---

## ADR-006 — REST JSON (pas gRPC) pour l'API locale

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** L'API est consommée par la CLI et des SDKs Python. gRPC est plus performant mais requiert la génération de code protobuf.

**Décision :** REST/JSON sur axum. Unix socket pour la CLI, TCP localhost pour les intégrations.

**Alternatives considérées :**
- gRPC : Génération protobuf, complexité client Python, pas debuggable avec curl.
- Unix socket seul : Complique les intégrations non-Rust.
- WebSocket : Over-engineered pour une API request/response standard.

**Conséquences :** Légèrement moins performant que gRPC. Non significatif pour l'usage cible.

---

## ADR-007 — Mémoire à l'initiative de l'agent

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** La "mémoire automatique" génère des appels LLM incontrôlés et des coûts imprévisibles.

**Décision :** L'agent appelle explicitement `ctx.memory.search()`, `ctx.memory.recall()`, etc. Pas d'injection automatique par le runtime.

**Alternatives considérées :**
- Injection automatique des épisodes récents : Coûteux, bruit dans le contexte.
- Injection après retrieval intelligent : Appel LLM caché supplémentaire.

**Conséquences :** Plus de contrôle pour le développeur, moins d'automatisme.

---

## ADR-008 — Pattern `noun verb` pour la CLI

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** Deux patterns dominent les CLIs modernes : `verb-noun` et `noun-verb`. Le choix doit être cohérent.

**Décision :** `noun-verb` : `apollia-os agent start`, `apollia-os task list`, `apollia-os memory inspect`.

**Alternatives considérées :**
- `verb-noun` : Moins naturel pour explorer les capacités d'un objet.
- Mixte : Incohérent, source de confusion.

**Conséquences :** Les commandes de niveau 1 (`start()`, `stop()`, `status`) sont des exceptions justifiées par leur fréquence.

---

## ADR-009 — Tokenizer FTS5 `unicode61`

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** La cible est le marché PME français. "réunion" doit matcher "reunion", "société" doit matcher "societe".

**Décision :** `unicode61` obligatoire.

**Alternatives considérées :**
- `simple` : Ne gère pas les accents. Inacceptable.
- `porter` : Stemming anglais uniquement.
- Tokenizer custom ICU : Dépendance supplémentaire.

**Conséquences :** Légèrement plus lent que `simple`. Non significatif pour les volumes PME.

---

## ADR-010 — Pivot du SaaS Python vers le Runtime Rust open-source

**Date :** 2026-03
**Statut :** Accepté (décision fondatrice)

**Contexte :** Voir [Pivot & Renouveau](./Vision-Pivot-et-Renouveau) pour le contexte complet.

**Décision :** Arrêt du développement SaaS full-stack (FastAPI + SvelteKit). Extraction du noyau technique (ORIA, sandbox, mémoire) dans un runtime Rust open-source.

**Alternatives considérées :**
- Continuer le SaaS : Marché encombré, cycle de vente long, ressources insuffisantes.
- SaaS Python avec runtime Python : Viole les principes #1 et #2.
- Open-source le SaaS complet : Trop complexe à opérer pour la communauté.

**Conséquences :** Abandon du code SaaS. Conservation de la valeur architecturale. Nouveau modèle économique open-source.

---

## ADR-011 — AgentId et TaskId ajoutés dans apollia-core (STORY-006)

**Date :** 2026-03-05
**Statut :** Accepté

**Contexte :** `RuntimeEvent` utilise `AgentId` et `TaskId`. Ces types doivent vivre dans `apollia-core` (zéro dépendance workspace) pour éviter des cycles.

**Décision :** `pub type AgentId = String` et `pub type TaskId = String` dans `apollia-core/src/events.rs`. Alias de type (pas de newtype) pour la friction minimale à l'utilisation.

**Alternatives considérées :**
- Newtype wrapping `String` : Plus de sécurité de type mais friction à la construction (`.into()` partout), non justifiée au Sprint 1.
- UUID natif (`uuid::Uuid`) : Contraint les callers à dépendre de `uuid`, sur-ingénierie pour des IDs qui peuvent être slugs ou UUIDs selon le contexte.

**Conséquences :** Les alias resteront des `String` jusqu'à ce qu'un besoin de distinction de type fort soit identifié. Migration vers newtype possible sans impact binaire.

---


## ADR-013 — Configuration PyO3 Python sur macOS via PYO3_PYTHON

**Date :** 2026-03-06
**Statut :** Accepté

**Contexte :** Sur macOS, le Python system (CommandLineTools 3.9) provoque un échec de link PyO3 (`library 'python3.9' not found`) car le chemin framework est incorrect. Sur Linux, aucun problème.

**Décision :** Utiliser `PYO3_PYTHON` pour pointer vers un Python Homebrew (3.12+) sur macOS. Pas de forçage dans `.cargo/config.toml` (trop machine-spécifique).

**Alternatives considérées :** Forcer dans .cargo/config.toml (rejetée : chemin varie par machine), exiger Xcode.app (rejetée : 12+ GB disproportionné), attendre fix PyO3 (rejetée : pas un bug PyO3).

**Conséquences :** Étape setup supplémentaire pour contributeurs macOS. Zéro impact Linux/CI. Compatible Principe #2 (dépendance dev uniquement, pas production).

**Principes impactés :** Principe #2 — Zéro dépendance externe (non violé : dépendance dev)

[Détail → docs/adr/ADR-013-pyo3-python-config-macos.md](adr/ADR-013-pyo3-python-config-macos.md)

---

## ADR-014 — Bridge AIP utilise spawn_blocking + asyncio.run()

**Date :** 2026-03-06
**Statut :** Accepté

**Contexte :** STORY-026 specifie `pyo3_async_runtimes::tokio::into_future` pour convertir les coroutines Python en Futures Rust. En pratique, `into_future` necessite un event loop asyncio actif en arriere-plan et un custom test harness, complexite disproportionnee pour Sprint 4.

**Decision :** Utiliser `tokio::task::spawn_blocking` + `asyncio.run()` pour executer les coroutines Python. Le GIL est tenu uniquement sur le blocking thread pool, jamais sur les workers Tokio.

**Alternatives considerees :** `into_future` + custom test harness (rejetee : complexite), `asyncio.run()` synchrone sans `spawn_blocking` (rejetee : bloque le worker Tokio)

**Consequences :** Tests compatibles `#[tokio::test]` standard, zero initialisation globale. Un thread blocking par appel agent concurrent. Migration vers `into_future` possible quand le runtime initialisera l'event loop asyncio.

**Principes impactes :** Principe #5 — Un acteur, une responsabilite (respecte)

[Détail → docs/adr/ADR-014-bridge-spawn-blocking-asyncio-run.md](adr/ADR-014-bridge-spawn-blocking-asyncio-run.md)

---

## ADR-015 — Trait ToolExecutor pour abstraire l'execution des outils

**Date :** 2026-03-06
**Statut :** Accepte

**Contexte :** STORY-027 (ToolProxy) necessite un point d'entree unifie pour executer les outils par nom, mais `ToolRegistryHandle` est un catalogue pur (register/get/list) sans methode d'execution.

**Decision :** Introduire un trait `ToolExecutor` (Send+Sync) avec `execute(tool_name, input) -> Result<Value, String>`. `ToolProxy` detient un `Arc<dyn ToolExecutor>`.

**Alternatives considerees :** Ajouter execute() a ToolRegistryHandle (rejetee : couple catalogue et execution, modifie Sprint 2), Execution hardcodee dans ToolProxy (rejetee : impossible a tester unitairement)

**Consequences :** Tests unitaires sans Python ni outils reels. Champ `executor` ajoute au struct ToolProxy par rapport a la spec initiale. Le NativeToolExecutor concret sera implemente dans une story ulterieure.

**Principes impactes :** Principe #5 — Un acteur, une responsabilite (respecte)

[Detail -> docs/adr/ADR-015-tool-executor-trait-abstraction.md](adr/ADR-015-tool-executor-trait-abstraction.md)

---

## ADR-016 — Trait AgentRunner pour decoupler ORIAEngine de AIPBridge

**Date :** 2026-03-06
**Statut :** Accepte

**Contexte :** STORY-030 (ORIAEngine execute_direct) necessite de tester la supervision StepBudget sans Python reel. `AIPBridge` depend de PyO3 et ne peut etre instancie sans interpreteur.

**Decision :** Introduire un trait `AgentRunner` (Send+Sync) avec `call_run(task) -> Pin<Box<dyn Future<...>>>`. `execute_direct()` prend `&dyn AgentRunner` au lieu de `&AIPBridge`.

**Alternatives considerees :** Prendre `&AIPBridge` directement (rejetee : tests impossibles sans Python), fonction libre inner (rejetee : le Future doit etre fourni par le caller)

**Consequences :** Tests unitaires sans Python. Pattern coherent avec ADR-015 (ToolExecutor). Signature diverge legerement de la spec STORY-030 initiale.

**Principes impactes :** Principe #5 — Un acteur, une responsabilite (respecte), Principe #7 — Garde-fous non-negociables (respecte)

[Detail -> docs/adr/ADR-016-agent-runner-trait-abstraction.md](adr/ADR-016-agent-runner-trait-abstraction.md)

---

## ADR-017 — hyper-util explicite pour Unix socket serving

**Date :** 2026-03-06
**Statut :** Accepte

**Contexte :** STORY-033 (APIServer axum). `axum::serve()` en 0.7.9 n'accepte que `TcpListener`. Le Unix socket listener necessite une boucle accept manuelle via hyper-util.

**Decision :** Ajouter `hyper-util = { version = "0.1", features = ["tokio", "server-auto", "service"] }` au workspace et a `apollia-runtime`. Ajouter la feature `util` a `tower = "0.4"` pour `ServiceExt`.

**Alternatives considerees :** Upgrader axum a 0.8+ (rejetee : breaking changes trop importants), proxy TCP interne (rejetee : complexite inutile)

**Consequences :** Unix socket fonctionne avec le meme Router que TCP. Code asymetrique (axum::serve pour TCP vs boucle manuelle pour Unix). Simplifiable quand axum 0.8 sera adopte.

**Principes impactes :** Principe #1 — Local-first (respecte), Principe #2 — Zero dependance externe (respecte, dep transitive)

[Detail -> docs/adr/ADR-017-hyper-util-unix-socket-serving.md](adr/ADR-017-hyper-util-unix-socket-serving.md)

---

## ADR-018 — CLI Bootstrap sans Supervisor

**Date :** 2026-03-06
**Statut :** Accepte

**Contexte :** STORY-037 (CLI niveau 1) depend de STORY-039 (Supervisor) non encore implementee. La commande `start()` doit demarrer le runtime en foreground.

**Decision :** Bootstrap sequentiel inline dans la commande `start()` (EventBus -> AgentRegistry -> TaskRouter -> APIServer). Endpoint `POST /api/v1/shutdown` emet `RuntimeEvent::ShutdownRequested` via EventBus. Sera remplace par le Supervisor (STORY-039).

**Alternatives considerees :** Attendre STORY-039 (rejetee : bloque le Sprint Goal), implementer le Supervisor dans STORY-037 (rejetee : augmente trop la taille de la story)

**Consequences :** CLI fonctionnelle immediatement. Code bootstrap temporaire a remplacer. Endpoint shutdown reutilisable par STORY-039/040.

**Principes impactes :** Principe #5 — Un acteur, une responsabilite (respecte), Principe #8 — CLI humaine, API machine (respecte)

[Detail -> docs/adr/ADR-018-cli-bootstrap-sans-supervisor.md](adr/ADR-018-cli-bootstrap-sans-supervisor.md)

---

## ADR-019 — Trait AgentLoader pour decoupler apollia-runtime de PyO3

**Date :** 2026-03-06
**Statut :** Accepte

**Contexte :** STORY-044 resout DT-031 (manifest_from_path placeholder). Le handler `start_agent` doit charger le module Python reel via AIPLoader, mais ajouter apollia-aip comme dependance de apollia-runtime couplerait le runtime a PyO3.

**Decision :** Introduire un trait `AgentLoader` (Send+Sync) avec `load_and_validate(path) -> Result<AgentManifest, String>`. Injecte dans `AppState` via `Arc<dyn AgentLoader>`. L'implementation concrete `AIPAgentLoader` vit dans apollia-cli.

**Alternatives considerees :** apollia-runtime depend directement de apollia-aip (rejetee : couple runtime a PyO3, casse 73 tests), feature flag python (rejetee : complexite conditionnelle)

**Consequences :** Tests unitaires sans Python. Pattern coherent avec ADR-015 (ToolExecutor), ADR-016 (AgentRunner). Champ supplementaire dans AppState.

**Principes impactes :** Principe #3 — Contrat minimal (respecte), Principe #5 — Un acteur, une responsabilite (respecte)

[Detail -> docs/adr/ADR-019-agent-loader-trait-decouplage-runtime-pyo3.md](adr/ADR-019-agent-loader-trait-decouplage-runtime-pyo3.md)

---

## ADR-020 — apollia-llm : moteur d'inférence embarqué (llama.cpp), backends cloud, feature flags

**Date :** 2026-03-08 (architecture) / 2026-03-26 (llama.cpp) / 2026-04-04 (Bedrock + Vertex)
**Statut :** Accepté

**Contexte :** Sprint 8 introduit `ctx.llm`. Trois contraintes : inférence locale offline (Principe #1), zéro daemon tiers requis (Principe #2), fail fast si modèle absent (Principe #4). Sprint 25 : `mistralrs` v0.7 bloquant — 16 architectures GGUF seulement, crash Metal sur MoE, streaming non-`'static`. Sprint 37 : intégration Bedrock (AWS) et Vertex AI (Google).

**Décision :** Crate `apollia-llm` avec feature flags : `cloud` (défaut, clients HTTP via `async-openai` + `reqwest`) et `local` (compile `EmbeddedBackend` via `llama-cpp-2`, lié statiquement — 30+ architectures GGUF, Metal MoE natif, streaming token-by-token). Le modèle `.gguf` est toujours un fichier externe dans `~/.apollia/models/`. Backend absent → warning. Aucun backend → `ctx.llm = None`, agent `DEGRADED`. **Bedrock :** signature SigV4 native via `aws-sigv4` + `reqwest` (aws-sdk-rust complet rejeté — +50 crates, +8 MB, 2% des fonctionnalités utilisées). **Vertex AI :** Application Default Credentials via `gcp-auth` (clé de service JSON rejetée — secret statique exfiltrable, pas d'expiration).

**Alternatives considérées :** Daemon externe (viole Principe #2), GGUF dans le binaire (~2 Go inutilisables), attendre mistral.rs 0.8+ (délai inconnu, pas de garantie Metal MoE), llama-server externe (viole Principe #2), aws-sdk-rust complet (dépendances excessives), clé de service JSON primaire Vertex (risque sécurité).

**Conséquences :** 30+ architectures GGUF, Metal MoE fonctionnel, streaming natif, Bedrock + Vertex sans SDK lourd. Build chain C++ requis pour `feature = "local"` (déjà présente via apollia-stt). Deux binaires CI (`cloud` + `local`). ADC Vertex requiert `gcloud` installé.

**Principes impactés :** Principe #1 — Local-first, Principe #2 — Zéro dépendance (statique + aws-sigv4 minimal), Principe #4 — Fail fast, Principe #7 — Garde-fous (StepBudget dans run_tools)

[Détail complet → docs/adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md](adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md)

---


## ADR-022 — ORIA Mode Orchestré : Option B (exécution directe outils) + hook `on_plan_complete`

**Date :** 2026-03-09
**Statut :** Accepté

**Contexte :** Sprint 10 implémente le Mode Orchestré d'ORIA (ADR-004). La question centrale est : pendant l'exécution d'un plan multi-step, qui exécute les outils — ORIA ou l'agent Python ? Trois options ont été considérées : (A) ORIA délègue chaque step à `agent.run()`, (B) ORIA exécute les outils directement sans appeler `run()`, (C) ORIA injecte le plan dans un `agent.run()` unique. Une décision connexe porte sur le post-traitement optionnel des outputs agrégés par l'agent.

**Décision :** Option B — ORIA exécute les outils directement via `ActorLoop`. `agent.run()` n'est jamais appelé pendant les steps du plan. L'agent est déclaratif : il fournit `manifest()` + `system_prompt`. Hook optionnel `on_plan_complete(step_results, ctx)` détecté via `hasattr` Python (duck typing ADR-003) pour le post-traitement métier custom. Si absent, ORIA concatène automatiquement les outputs.

**Alternatives considérées :** Option A (rejetée : état inter-steps reporté sur l'agent, `StepBudget` partagé complexifie l'interface AIP), Option C (rejetée : expose `ExecutionPlan` à l'agent, contourne `ResilienceLayer` et persistance SQLite par step)

**Conséquences :**
- Agent déclaratif : `manifest()` + `system_prompt` suffisent pour le Mode Orchestré.
- Tous les garde-fous runtime (`StepBudget`, `ResilienceLayer`, audit SQLite) appliqués systématiquement sans coopération de l'agent.
- `ActorLoop` testable en Rust pur (mock `CompletionModel` + mock `ToolProxy`).
- `system_prompt` obligatoire en Mode Orchestré → fail fast si absent (Principe #4).
- L'agent ne peut pas modifier le plan step par step — replanification déléguée à ORIA (max 2 fois).

**Principes impactés :** Principe #3 — Contrat minimal (respecté), Principe #4 — Fail fast (respecté), Principe #7 — Garde-fous non-négociables (respecté), Principe #5 — Un acteur une responsabilité (respecté)

[Détail complet → docs/adr/ADR-022-oria-mode-orchestre-option-b.md](adr/ADR-022-oria-mode-orchestre-option-b.md)

---

## ADR-023 — HITL : re-appel `agent.run()` avec `AIPTask.is_resumed` + `InputResponse`, `tools_requiring_approval` dans le manifest

**Date :** 2026-03-09
**Statut :** Accepté

**Contexte :** Sprint 11 introduit le Human-in-the-Loop. Deux décisions de design encadrent le contrat AIP : (1) comment le runtime communique-t-il la réponse humaine à l'agent lors de la relance ? (2) comment l'agent déclare-t-il les outils nécessitant approbation avant exécution en Mode Orchestré ?

**Décision :** Réutiliser `agent.run()` comme unique point d'entrée. `AIPTask` est enrichi de deux champs optionnels (`is_resumed: bool`, `input_response: InputResponse | None`). L'agent vérifie `if task.is_resumed` pour brancher sa logique de reprise. `tools_requiring_approval: list[str]` est un champ optionnel de `AgentManifest` — l'`ActorLoop` le consulte avant chaque step en Mode Orchestré.

**Alternatives considérées :**
- Option 2 — Nouveau hook `on_resume(response, ctx)` (rejetée : quatrième méthode AIP, comportement par défaut ambigu si absent, duplication `spawn_blocking`)
- Option 3 — Réponse dans `MemoryManager`, agent lit via `ctx.memory` (rejetée : injection implicite contraire à Principe #6, couplage fort HITL ↔ apollia-memory)

**Conséquences :**
- `call_run()` dans `AIPBridge` réutilisé sans modification pour la reprise — zéro nouveau chemin d'exécution.
- `InputResponse.context` persiste dans SQLite (`task_approvals`) — état de l'agent au moment de la suspension auditable.
- Un agent qui n'implémente pas `if task.is_resumed` produira un comportement incorrect mais explicite à l'exécution.
- `TimeoutWatcher` annule automatiquement après `input_required_timeout_hours` — garde-fou Principe #7.

**Principes impactés :** Principe #3 — Contrat minimal (respecté), Principe #4 — Fail fast (ResumeHandler valide l'état), Principe #6 — Mémoire à initiative de l'agent (respecté), Principe #7 — Garde-fous non-négociables (TimeoutWatcher)

[Détail complet → docs/adr/ADR-023-hitl-is-resumed-input-response-tools-requiring-approval.md](adr/ADR-023-hitl-is-resumed-input-response-tools-requiring-approval.md)

---

## ADR-024 — apollia-notifications : trait `NotificationChannel`, 3 canaux (desktop/SSE/webhook), payload JSON fixe Apollia

**Date :** 2026-03-09
**Statut :** Accepté

**Contexte :** Sprint 11 introduit `apollia-notifications`. Trois décisions structurantes engagent l'interface publique et la configuration `apollia.toml` : (1) architecture push (EventBus) ou pull (polling SQLite), (2) canaux câblés en dur ou via trait commun extensible, (3) payload webhook JSON fixe Apollia ou templates configurables (Handlebars/Tera).

**Décision :** `NotificationEngine` s'abonne directement à l'`EventBus` (push). Trait `NotificationChannel: Send + Sync` avec `send(&Notification)` — trois implémentations initiales : `DesktopChannel` (notify-rust v4), `SseChannel` (bridge EventBus → dashboard), `WebhookChannel` (reqwest). Payload webhook JSON fixe versionné (`"runtime": "apollia-os"`, `"version": "..."`, `"severity"`, `"metadata.resume_url"`). La transformation vers les formats propriétaires (Slack Block Kit, PagerDuty) est déléguée à l'intégrateur (n8n, Zapier).

**Alternatives considérées :**
- Polling SQLite (rejetée : latence 1–5s inacceptable pour HITL interactif, événements éphémères non capturables, complexité curseurs)
- Templates Handlebars/Tera (rejetée : dépendance ~500 Ko, courbe d'apprentissage, bugs silencieux, transformation déjà couverte côté intégrateur)
- Dashboard SSE uniquement, sans crate notifications (rejetée : l'utilisateur ne sait pas qu'une approbation l'attend si le dashboard n'est pas ouvert — objectif HITL non atteint)
- Canaux câblés en dur sans trait (rejetée : ajout de canal → modification engine, tests impossibles sans canaux concrets)

**Conséquences :**
- `NotificationEngine` testable via `MockChannel : NotificationChannel`.
- Latence quasi-nulle (abonnement EventBus direct).
- Échec canal → `tracing::warn!` uniquement, runtime non affecté.
- `notify-rust v4` requiert libnotify sur Linux headless → `NotifError::DesktopUnavailable` non-critique.
- Table SQLite `notification_logs` nécessaire pour `apollia-os notify logs` — rotation TTL 30j à prévoir Sprint 12.

**Principes impactés :** Principe #1 — Local-first (desktop/SSE offline), Principe #2 — Zéro dépendance externe (dépendances compilées statiquement), Principe #4 — Fail fast (config validée au démarrage), Principe #5 — Un acteur une responsabilité (dispatch uniquement)

[Détail complet → docs/adr/ADR-024-apollia-notifications-trait-channel-json-fixe.md](adr/ADR-024-apollia-notifications-trait-channel-json-fixe.md)

---

## ADR-025 — apollia-pipelines : TOML déclaratif, 5 topologies natives par graph `depends_on`, HITL intégré via EventBus

**Date :** 2026-03-10
**Statut :** SUPERSEDED — crate retirée du workspace v0.1.0 (composition multi-agent désormais via triggers + agents ReAct autonomes)

**Contexte :** Sprint 12 introduit `apollia-pipelines` pour la coordination multi-agent. Quatre décisions structurantes engagent des interfaces difficiles à inverser : (1) format de déclaration des pipelines, (2) expression des topologies (séquentiel, fan-out, fan-in, conditionnel, fallback), (3) intégration du HITL Sprint 11 dans le cycle de vie pipeline, (4) rendu des templates `{{steps.x.output}}` entre steps.

**Décision :** (1) Configuration TOML-only via `[[pipelines]]` + `[[pipelines.steps]]` dans `apollia.toml` — même source de vérité que les triggers (ADR-021). Validation sémantique exhaustive dans `ApolliaConfig::load()` : unicité des IDs, `depends_on` existants, `fallback_for` valides, absence de cycles. Pipeline invalide = démarrage refusé (Principe #4). (2) Les 5 topologies émergent du graph `depends_on` + champs `condition`/`fallback_for` sans primitive TOML explicite. `topological_layers()` partitionne les steps en layers — `FuturesUnordered` exécute une layer en parallèle (fan-out naturel), le join est implicite (fan-in = step avec plusieurs `depends_on`). (3) `PipelineExecutor` réutilise les événements `TaskInputRequired`/`TaskResumed` (ADR-023) sans nouveau mécanisme HITL — il observe l'EventBus, émet `PipelineSuspended`, attend `TaskResumed`, reprend avec le nouveau `task_id`. (4) `TemplateRenderer` par remplacement de chaîne (`render()` pur, pas de moteur externe) — variables non résolues nettoyées via regex, jamais de `panic!`.

**Alternatives considérées :**
- API REST CRUD pipelines + SQLite (rejetée : double source de vérité TOML/base, pas de fail fast au démarrage — même raison qu'ADR-021 Option A)
- Topologies explicites `topology: "fan-out"` dans le TOML (rejetée : redondant avec `depends_on`, conflits possibles entre clé et graph réel)
- DSL externe type Argo Workflows / Prefect (rejetée : viole Principe #2, surface massive, pas d'intégration HITL native)
- Canal oneshot dédié `PipelineExecutor → ResumeHandler` pour le HITL (rejetée : duplique le mécanisme Sprint 11, non restaurable après restart depuis SQLite)
- Moteur de templates Handlebars/Tera (rejetée : même décision qu'ADR-024 — dépendance ~500 Ko inutile pour du string replace)

**Conséquences :**
- `apollia.toml` est la source de vérité unique pour trigger → pipeline → agent : chaîne complète déclarative et versionnée.
- `PipelineEngine` réutilise `TaskRouter` sans modification — StepBudget, ResilienceLayer et audit SQLite s'appliquent automatiquement à chaque step.
- Reprise après restart native : `pipeline_runs` en status `running` rechargés depuis SQLite, steps complétés non re-soumis.
- `PipelineCompleted`/`PipelineFailed` captés par `NotificationEngine` (ADR-024) sans modification.
- `regex = "1"` nouvelle dépendance workspace pour le TemplateRenderer cleanup.

**Principes impactés :** Principe #1 — Local-first, Principe #2 — Zéro dépendance externe, Principe #4 — Fail fast, Principe #5 — Un acteur une responsabilité, Principe #7 — Garde-fous non-négociables, Principe #8 — CLI humaine

[Détail complet → docs/adr/ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md](adr/ADR-025-apollia-pipelines-toml-declaratif-topologies-natives-hitl-integre.md)

---

## ADR-026 — Observabilité complète : persistance input/output SQLite, timeline unifiée, troncature configurable

**Date :** 2026-03-13
**Statut :** Accepté

**Contexte :** Après 12 sprints, les données d'exécution sont fragmentées : inputs/outputs non persistés, appels LLM éphémères, durées non mesurées. L'opérateur ne peut pas diagnostiquer a posteriori ce qui s'est passé lors de l'exécution d'un agent.

**Décision :** (1) Extensions de schéma SQLite dans les fonctions d'initialisation Rust existantes (`ALTER TABLE ADD COLUMN IF NOT EXISTS`), pas d'outil de migration externe. (2) Persistance input/output avec troncature configurable (`max_input_bytes = 32KB` défaut) + marqueur `[TRONQUÉ]` + flag `*_truncated`. (3) Timeline API côté serveur (`GET /api/v1/tasks/{id}/timeline`) agrège 5 sources SQLite en un seul appel. (4) `prompt_text` LLM nullable, `debug_log_prompt = false` par défaut (RGPD). (5) Troncature avec marqueur plutôt que rejet (observabilité partielle > absence).

**Alternatives considérées :** Fichiers `.sql` séparés + outil migration (rejetée : paradigme non établi dans le projet), stockage fichier pour volumes (rejetée : fragmente l'audit trail), client agrège N requêtes (rejetée : cohérence temporelle impossible), toujours persister prompts LLM (rejetée : risque RGPD données personnelles en clair)

**Conséquences :**
- Zéro boîte noire : chaque action traçable via Timeline API
- `apollia-llm` acquiert `rusqlite` (nouvelle table `llm_calls`)
- Taille DB augmente mais bornée par troncature 32KB/champ
- Rotation/archivage des données à prévoir dans un sprint futur

**Principes impactés :** Principe #1 — Local-first (respecté), Principe #2 — Zéro dépendance externe (respecté), Principe #4 — Fail fast (respecté), Principe #8 — CLI humaine, API machine (respecté)

[Détail complet → docs/adr/ADR-026-observabilite-complete-persistance-timeline-troncature.md](adr/ADR-026-observabilite-complete-persistance-timeline-troncature.md)

---

## ADR-027 — apollia-desktop : processus unique Tauri + runtime embarqué

**Date :** 2026-03-13
**Statut :** Accepté

**Contexte :** Apollia OS est mature fonctionnellement (13 sprints livrés) mais accessible uniquement via CLI. Les utilisateurs non-techniques (PME) ont besoin d'une application desktop native — double-clic → fenêtre → agents visibles. La question : comment le frontend Tauri communique-t-il avec le runtime Rust ?

**Décision :** Processus unique — `apollia-desktop` (Tauri v2) démarre le runtime en interne via `init_embedded()`. Commandes Tauri `#[tauri::command]` pour les mutations ponctuelles (wrappent les handles Tokio). SSE EventBus existant (`localhost:7771/api/v1/dashboard/stream`) pour les flux temps réel. Pas de canal Tauri events en doublon. Le CLI reste fonctionnel via le Unix socket existant.

**Alternatives considérées :** Deux processus séparés (rejetée : synchronisation démarrage complexe, deux binaires à distribuer, sérialisation HTTP inutile alors que les handles Tokio sont disponibles en mémoire), WebView navigateur sans Tauri (rejetée : friction inacceptable pour utilisateur non-technique, pas de packaging natif, pas de tray icon, pas de file picker)

**Conséquences :**
- Distribution simplifiée : un seul `.dmg` / `.AppImage`
- Handles Tokio réutilisés sans modification architecturale
- Binaire plus gros (~50MB avec Tauri + WebView engine)
- Risque de conflit linker PyO3 + Tauri sur macOS — à diagnostiquer dès STORY-135

**Principes impactés :** Principe #1 — Local-first (renforcé), Principe #2 — Zéro dépendance externe (respecté), Principe #4 — Fail fast (respecté), Principe #8 — CLI humaine, API machine (étendu au desktop)

[Détail complet → docs/adr/ADR-027-apollia-desktop-processus-unique-tauri-runtime-embarque.md](adr/ADR-027-apollia-desktop-processus-unique-tauri-runtime-embarque.md)

---

## ADR-028 — Frontend Svelte : UX first, UI sprint dédié

**Date :** 2026-03-13
**Statut :** Accepté

**Contexte :** Le Sprint 14 introduit une app desktop (ADR-027). Le dashboard HTMX existant est insuffisant pour les interactions complexes (HITL real-time, timeline interactive, file picker natif). Choix de la stack frontend et stratégie de design à définir.

**Décision :** Svelte 5 (runes) + Vite + shadcn-svelte (headless). Pour Sprint 14-15, aucune customisation visuelle — shadcn par défaut. La patte visuelle Apollia sera appliquée dans un sprint UI dédié après validation des parcours utilisateurs sur des utilisateurs réels.

**Alternatives considérées :** React/Next.js (rejetée : overhead, pas d'expérience récente, bundle lourd, framework SSR inadapté à Tauri), HTMX étendu (rejetée : insuffisant pour HITL compteur live, timeline interactive, file picker natif, pas de typage TS), design custom immédiat (rejetée : risque de bikeshedding, budget solo)

**Conséquences :**
- Stack légère, rapide à développer, composants accessibles out-of-the-box
- Look "shadcn générique" pendant 2-3 sprints — acceptable en phase validation
- Migration vers thème custom = uniquement surcharge CSS, zéro refactoring composants
- Navigation par store Svelte (pas de deep linking — acceptable pour app desktop)

**Principes impactés :** Principe #1 — Local-first (respecté : assets compilés embarqués), Principe #2 — Zéro dépendance externe (respecté : composants shadcn copiés localement), Principe #8 — CLI humaine, API machine (étendu au desktop Svelte)

[Détail complet → docs/adr/ADR-028-frontend-svelte-ux-first-ui-sprint-dedie.md](adr/ADR-028-frontend-svelte-ux-first-ui-sprint-dedie.md)

---

## ADR-029 — Settings lecture seule dans l'application desktop

**Date :** 2026-03-13
**Statut :** Accepté

**Contexte :** Sprint 15 introduit la vue Settings dans l'application desktop (STORY-149). La question est de savoir si l'édition de `apollia.toml` doit être intégrée dans l'app ou déléguée à un éditeur externe.

**Décision :** La vue Settings est lecture seule. L'édition est déléguée à l'éditeur natif du système via `open::that(config_path)`.

**Alternatives considérées :**
- Édition in-app avec `toml_edit` : Préserve les commentaires mais ajoute complexité, risque de bugs sur les cas edge du format TOML.
- Édition partielle (quelques champs) : Surface de bugs, incohérence UX (certains champs éditables, d'autres non).

**Conséquences :** L'utilisateur doit utiliser un éditeur externe. Redémarrage nécessaire pour appliquer les changements. Aucun risque de corruption du fichier TOML. Complexité frontend minimale.

[Détail → docs/adr/ADR-029-settings-lecture-seule.md](adr/ADR-029-settings-lecture-seule.md)

---

## ADR-032 — Agent Install, Bundle Format & Package System

**Date :** 2026-03-17 (install) / 2026-04-17 (bundle) / 2026-04-24 (packages)
**Statut :** Accepté

**Contexte :** Les agents Python étaient 100% éphémères (seul composant non-persisté dans `~/.apollia/`). Le lancement v0.1.0 requiert un format de distribution auto-descriptif pour les 4 assistants (avec modules `lib/`). Sprint 43 : installer un groupe d'agents liés (director + workers) requiert N commandes séparées sans concept de package.

**Décision :** (1) **Install :** copie du bundle dans `~/.apollia/agents/<name>/`, persistance SQLite `agents.db`, auto-reload au boot. Commandes : `agent install/uninstall/enable/disable/update`. (2) **Bundle format :** dossier standardisé avec `manifest.toml` + `agent.py` (obligatoires), `lib/` (modules), `assets/` (read-only). `manifest.toml` contient métadonnées statiques (name, version, tools_required, permissions). Modules via `from lib import helpers` (jamais `from shared`). Chargement PyO3 : prepend `<install_path>/` à `sys.path`, nettoyé après. (3) **Package system :** dossier avec `agent.toml` décrivant N agents + triggers déclarés — une commande installe l'ensemble. Tables SQLite `installed_packages` + `package_agents`.

**Alternatives considérées :** Référence par chemin absolu (fichier peut être supprimé), Python wheel (trop lourd), registry centralisé type npm/cargo (viole Principe #1 + #2), config inline dans manifest Python pour packages (mélange logique / déploiement).

**Conséquences :** Format auto-descriptif indexable par marketplace futur. Rétrocompatibilité totale (agents `.py` unitaires inchangés). SHA256 bundle pour détection d'updates. Pas d'encryption du bundle v0.1.0.

**Principes impactés :** Principe #1 — Local-first, Principe #2 — Zéro dépendance, Principe #3 — Contrat minimal (2 fichiers obligatoires), Principe #4 — Fail fast (bundle invalide → rejet install).

[Détail → docs/adr/ADR-032-agent-install-persistence.md](adr/ADR-032-agent-install-persistence.md)

---

## ADR-033 — Config opérateur SQLite, HMAC-SHA256 webhooks, hot reload sans restart

**Date :** 2026-03-08 (HMAC + hot reload) / 2026-03-20 (config SQLite)
**Statut :** Accepté

**Contexte :** Sprint 9 : authentification des webhooks entrants + hot reload des triggers sans downtime. Sprint 17 : `apollia.toml` mélange config structurelle (ports, LLM) et opérationnelle (triggers, pipelines, notifications) — un non-développeur ne peut pas configurer sans éditer du TOML.

**Décision :** (1) **SQLite opérationnel :** triggers/pipelines/notifications migrent dans SQLite (une DB par sous-système). TOML = config structurelle uniquement. Pattern : API handler → SQLite write → `Handle.reload()` synchrone. (2) **HMAC-SHA256 webhooks :** header `X-Apollia-Signature: sha256=<hex>` (standard GitHub), comparaison via `constant_time_eq` (timing attacks éliminées). Ordre réponse : 503 → 404 → 401 → 200. (3) **Hot reload :** `TriggerEngineHandle::reload()` — timeout 2s par `JoinHandle<()>`, full-replace, compteurs SQLite préservés, `TriggersReloaded` sur EventBus. Erreur au reload → 422, triggers actuels inchangés.

**Alternatives considérées :** TOML source de vérité avec hot-reload amélioré (ne résout pas le problème opérateur), EventBus pour notifier (complexité sans feedback synchrone), Watch file SQLite (fragile avec WAL), Token Bearer webhooks (n'authentifie pas le body), SIGHUP (incompatible Windows, pas de retour erreur).

**Conséquences :** CRUD depuis l'API REST et le desktop avec validation interactive (422). ADR-029 (Settings lecture seule) reste valide pour le TOML structurel. `Arc<Mutex<>>` pour les repositories (rusqlite non-Sync, mutations rares).

**Principes impactés :** Principe #1 — Local-first, Principe #2 — Zéro dépendance, Principe #4 — Fail fast (422 au write time), Principe #8 — CLI humaine.

[Détail → docs/adr/ADR-033-config-operateur-sqlite.md](adr/ADR-033-config-operateur-sqlite.md)

---

## ADR-034 — Chat hybride : sessions, streaming, HITL inline

**Date :** 2026-03-20
**Statut :** Accepté

**Contexte :** Les agents Apollia OS sont exclusivement programmés (triggers, pipelines, tasks fire-and-forget). Il manque un mode interactif conversationnel. Le TaskRouter existant est stateless et fire-and-forget — incompatible avec les sémantiques du chat (sessions longues, état mutable, streaming token-by-token, HITL inline avec AlwaysAccept).

**Décision :** Chemin d'exécution séparé du TaskRouter. Nouvel acteur `ChatSessionManager` (position 13 Supervisor) avec deux modes : Chat Libre (BuiltInChatAgent Rust, boucle ReAct, streaming token-by-token) et Chat Agent (AIPBridge.call_run() direct, réponse en bloc). POST + SSE (pas de WebSocket). Tous les outils requièrent approbation HITL en mode chat (Accept/Refuse/AlwaysAccept per-session). Persistance dans `chat.db` SQLite séparé.

**Alternatives considérées :** WebSocket (rejetée — aucune infra existante, POST+SSE suffit), Session = single long-running task (rejetée — incompatible modèle stateless, semaphore bloquant), Chat via TaskRouter avec extensions (rejetée — dénaturerait le TaskRouter, Principe #5 violé).

**Conséquences :** `ChatSessionManager` acteur dédié. `BuiltInChatAgent` en Rust (Chat Libre sans Python). 12 nouveaux RuntimeEvent variants `Chat*`. `chat.db` SQLite supplémentaire. HITL plus restrictif qu'en mode background. TaskRouter inchangé (zéro régression). Concurrence Chat vs Tasks pour agent Python non thread-safe à surveiller.

**Principes impactés :** Principe #5 (un acteur, une responsabilité) respecté, Principe #7 (garde-fous) renforcé (HITL systématique + StepBudget par échange).

[Détail → docs/adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md](adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md)

---

## ADR-035 — Per-step observation en mode Orchestré

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** En mode Orchestré ORIA, le runtime pilote l'exécution des steps (pas l'agent Python). Les outputs des steps ne sont ni injectés dans le contexte des steps suivants, ni auto-enregistrés en mémoire épisodique. Le Principe #6 (mémoire à initiative de l'agent) ne s'applique pas car l'agent ne contrôle pas l'exécution.

**Décision :** Injection delta légère : après chaque step, le runtime injecte les outputs précédents dans le contexte du step suivant (StepContext) et auto-enregistre une entrée mémoire épisodique (importance 0.6). Principe #6 relâché uniquement en mode Orchestré.

**Alternatives considérées :** Re-observation complète per-step via Observer (rejetée — extra LLM call par step, trop coûteux), Plan-once execute-blindly (rejetée — steps sans contexte des résultats précédents).

**Conséquences :** Nouveau StepContext struct. Memory writes fire-and-forget. Trace épisodique auto-construite. Principe #6 documenté comme relâché en mode Orchestré uniquement.

**Principes impactés :** Principe #6 — Mémoire à initiative de l'agent (relâché en Orchestré), Principe #5 — Un acteur, une responsabilité (ActorLoop enrichi).

[Détail → docs/adr/ADR-035-per-step-observation-orchestrated.md](adr/ADR-035-per-step-observation-orchestrated.md)

---

## ADR-036 — Stratégie de cache de plans ORIA

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** En mode Orchestré, ORIA appelle le LLM pour générer un ExecutionPlan à chaque tâche. Les tâches identiques produisent le même plan, gaspillant des appels LLM.

**Décision :** Cache SQLite `plan_cache.db` avec clé SHA-256 de `{agent_name}:{agent_version}:{sorted_tools}:{normalized_task_text}`. TTL 7 jours, max 1000 entrées, LRU. Cache vérifié avant Reasoner::plan(). Cache hit émet RuntimeEvent::PlanCacheHit.

**Alternatives considérées :** In-memory LRU (rejetée — perdu au restart), Pas de cache (rejetée — gaspillage LLM).

**Conséquences :** Réduction coût LLM pour tâches répétitives. SQLite DB supplémentaire. Risque de staleness (mitigé par agent_version dans la clé + TTL).

**Principes impactés :** Principe #1 — Local-first (cache local SQLite), Principe #4 — Fail fast (cache miss = fallback transparent).

[Détail → docs/adr/ADR-036-plan-cache-strategy.md](adr/ADR-036-plan-cache-strategy.md)

---

## ADR-037 — Packaging Python SDK

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** Les développeurs d'agents écrivent du Python en important depuis un fichier unique `apollia_base.py` sans type hints, sans mocks, sans IDE autocomplete.

**Décision :** Package `apollia-sdk` séparé dans `sdk/` à la racine. Installable via `pip install -e ./sdk`. Zéro dépendance Rust à l'installation. Type stubs PEP 561. Base classes (ReAct, Conversational, Orchestrated), utilitaires, mocks de test, scaffolding CLI.

**Alternatives considérées :** SDK bundlé dans le binaire Rust via PyO3 (rejetée — nécessite build Rust pour développer en Python), Garder apollia_base.py unique (rejetée — ne scale pas, pas d'IDE support).

**Conséquences :** Autocomplete IDE, validation mypy, tests MockContext, scaffolding `apollia new`. Stubs à maintenir en sync avec PyO3.

**Principes impactés :** Principe #3 — Contrat minimal (SDK expose uniquement ce dont les agents ont besoin), Principe #2 — Zéro dépendance externe (SDK pur Python).

[Détail → docs/adr/ADR-037-python-sdk-packaging.md](adr/ADR-037-python-sdk-packaging.md)

---

## ADR-038 — Mémoire utilisateur globale

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** Les sessions chat sont isolées — pas de mémoire cross-session. Le système ne connaît pas le nom, les préférences ou l'expertise de l'utilisateur.

**Décision :** Namespace mémoire spécial `__user__` dans SemanticMemory. 3 catégories (preferences, habits, context). Injection non-déterministe dans le system prompt ("for reference, use as you see fit"). Sources : onboarding (0.9), chat_inference (0.5), user_explicit (0.95), agent_observation (0.5).

**Alternatives considérées :** Contexte per-session uniquement (rejetée — amnésie cross-session), Moteur de règles déterministe (rejetée — viole Principe #6, rend les agents heuristiques).

**Conséquences :** Continuité cross-session. User peut voir/éditer/valider ses mémoires. Risque de mémoires incorrectes (mitigé par confidence basse + feedback loop).

**Principes impactés :** Principe #6 — Mémoire à initiative de l'agent (étendu au niveau utilisateur, disponible mais jamais imposé), Principe #1 — Local-first (données utilisateur en SQLite local).

[Détail → docs/adr/ADR-038-global-user-memory.md](adr/ADR-038-global-user-memory.md)

---

## ADR-039 — Conversation memory management

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** Les conversations chat grandissent indéfiniment et finiront par dépasser la fenêtre de contexte du LLM.

**Décision :** Sliding window de 20 derniers messages + résumé LLM des messages hors fenêtre. Résumé stocké dans `chat_sessions.summary`. Recalculé quand la fenêtre glisse. Contexte = system prompt + user memory + summary + window + message courant.

**Alternatives considérées :** Garder tous les messages et tronquer au débordement (rejetée — perte brutale de contexte), Résumé hiérarchique multi-niveaux (rejetée — sur-engineered pour le besoin actuel).

**Conséquences :** Taille de contexte bornée et prévisible. Contexte clé préservé dans le résumé. Extra LLM call lors du shift de fenêtre (~tous les 20 messages).

**Principes impactés :** Principe #8 — CLI humaine, API machine (résumé généré machine pour injection machine).

[Détail → docs/adr/ADR-039-conversation-memory-management.md](adr/ADR-039-conversation-memory-management.md)

---

## ADR-040 — Onboarding comme agent conversationnel

**Date :** 2026-03-23
**Statut :** Accepté

**Contexte :** Apollia OS a besoin d'un flux d'onboarding pour collecter le contexte utilisateur initial. La plupart des applications utilisent des wizards déterministes avec des étapes numérotées, ce qui contredit la philosophie agentique.

**Décision :** L'onboarding est un ConversationalAgent standard (SDK Sprint 21), déployé via une session chat. Le system prompt guide 5 domaines (identité, préférences, outils, domaine, agents) mais l'agent DÉCIDE l'ordre et la profondeur. Chaque insight est persisté immédiatement via ctx.memory.remember(). Pas de schéma rigide, pas d'étapes numérotées.

**Alternatives considérées :** Wizard déterministe à étapes (rejetée — mécanique, ne showcase pas les capacités agents), Apprentissage passif uniquement (rejetée — prend trop de sessions, mauvaise première expérience).

**Conséquences :** Démonstration first-class des capacités agentiques. Interaction naturelle et adaptative. Couverture potentiellement incomplète si l'utilisateur quitte tôt (mitigé par persistance immédiate + re-trigger `apollia-os onboard --topic`).

**Principes impactés :** Principe #3 — Contrat minimal (agent onboarding = même contrat SDK), Principe #6 — Mémoire à initiative de l'agent (l'agent décide quoi retenir).

[Détail → docs/adr/ADR-040-onboarding-conversational-agent.md](adr/ADR-040-onboarding-conversational-agent.md)

> **Évolution v2.2.0 (mai 2026) — onboarding hybride.** Le parcours actuel est un compromis entre wizard et agent : les choix techniques structurés (profil opérateur/builder, configuration LLM/STT, téléchargement de modèles) ont été ré-encadrés dans des écrans dédiés (`OnboardingWelcome`, `OnboardingProfileSelector`, `OnboardingAiSetup`) qui précèdent la conversation calibrage de l'agent (`OnboardingChatStep`). Motivation : un formulaire avec validation immédiate bat un parsing LLM sur des décisions sans ambiguïté (clés API, sélection de modèle GGUF, choix de profil). Le principe #6 reste tenu — l'agent reste seul maître des décisions sémantiques (rôle, supervision, souveraineté) qu'il persiste lui-même. Spec frontend complète : [Onboarding-System](Onboarding-System).

---

## ADR-041 — Moteur STT embarqué : whisper-rs V1, trait SttBackend, roadmap candle-whisper/Voxtral

**Date :** 2026-03-25
**Statut :** Accepté

**Contexte :** Apollia OS doit transcrire la parole en texte localement (hotkey globale → dictée vocale). Le moteur STT doit respecter les mêmes contraintes que le LLM (ADR-020) : local-first, zéro dépendance opérationnelle, fail fast. STT et LLM sont des pipelines distincts → crate dédiée (Principe #5).

**Décision :** Nouvelle crate `apollia-stt` avec trait `SttBackend` object-safe (Send + Sync, API synchrone, `spawn_blocking` côté appelant). Implémentation V1 via `whisper-rs` 0.16 (whisper.cpp FFI, compilation statique CMake). Feature flags `stt-whisper-cpp` (défaut) / `stt-metal` / `stt-cuda` identiques au pattern ADR-020. Modèle GGML (~900 Mo) comme fichier externe dans `~/.apollia/models/`.

**Alternatives considérées :** candle-whisper pure Rust (rejetée V1 — benchmarks Metal inférieurs, RTF ~0.50x vs ~0.30x whisper.cpp, prévu V2 Q3-Q4 2026), Service STT cloud Whisper API/Google/Deepgram (rejetée — viole Principes #1 et #2), Voxtral Mistral (rejetée V1 — pas encore disponible dans l'écosystème Rust, prévu V3 2027).

**Conséquences :**
- Pipeline STT 100% local, < 2s sur M1 Metal, trait abstrait pour migration V2/V3 sans refactoring
- CMake requis au build-time (documenté INSTALL.md), ~5-15 Mo supplémentaires dans le binaire
- Conflit ggml potentiel futur si llama.cpp intégré directement (mitigé par le trait abstrait)

**Principes impactés :** Principe #1 — Local-first (renforcé), Principe #2 — Zéro dépendance (respecté), Principe #4 — Fail fast (respecté), Principe #5 — Un acteur, une responsabilité (crate dédiée)

[Détail → docs/adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md](adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md)

---


## ADR-043 — Décomposition atomique des outils natifs

**Date :** 2026-03-29
**Statut :** Accepté

**Contexte :** Les 3 outils natifs actuels (bash_executor, file_io, python_executor) ont une scope trop large. `file_io` combine read+write+list, créant des schémas JSON ambigus pour les LLM (~15% d'erreurs de validation). Pas d'outils de recherche (grep/glob), pas d'outil HTTP dédié, pas d'outil mémoire malgré le Memory Engine FTS5+BM25 existant. Le marché (Claude Code, Cursor, Cline) a convergé vers 7-10 outils file atomiques.

**Décision :** Décomposer file_io en 4 outils atomiques (file_read, file_write, file_edit, file_list) + ajouter 2 outils de recherche (file_glob, file_grep) + http_fetch + memory_search. Déprécier file_io (code conservé mais non-enregistré, warning au resolve). Total : 10 outils actifs. Règle : un outil = une action sémantique = un schéma JSON sans ambiguïté.

**Alternatives considérées :** Garder file_io avec meilleure description (rejetée — le problème est structurel, pas dans la doc), Ajouter des exemples dans les descriptions (rejetée — approche cosmétique), Supprimer complètement file_io (rejetée — breaking change, préféré dépréciation progressive).

**Conséquences :** Réduction des erreurs de validation (~15% → ~2%). Surface d'API complète (recherche, HTTP, mémoire). Plus d'outils à maintenir (10 vs 3), mais chaque outil est plus simple. Migration nécessaire pour agents existants (guidée par warning). À surveiller : adoption par les agents, performance registry, cohérence des descriptions.

**Principes impactés :** Principe #3 — Contrat minimal (renforcé : un outil = une action claire), Principe #4 — Fail fast (renforcé : schémas JSON sans ambiguïté), Principe #1 — Local-first (étendu : memory_search expose la mémoire locale).

[Détail → docs/adr/ADR-043-decomposition-atomique-outils.md](adr/ADR-043-decomposition-atomique-outils.md)

---

## ADR-044 — Client MCP : architecture, transport, lifecycle

**Date :** 2026-03-29
**Statut :** Accepté

**Contexte :** 16 000+ serveurs MCP existent (GitHub, Notion, Slack, PostgreSQL, Brave Search…). Sans client MCP, les agents Apollia sont isolés de cet écosystème. Le transport stdio couvre ~90 % des serveurs communautaires et reste local-first (subprocess, pas de réseau distant). Le Principe #2 interdit tout SDK MCP tiers.

**Décision :** Nouvelle crate `apollia-mcp` avec implémentation native du protocole JSON-RPC 2.0 + MCP. Transport stdio uniquement en V1 (deux tâches Tokio : stdin writer, stdout reader). Configuration via `~/.apollia/mcp.toml` (secrets interpolés depuis les variables d'environnement). Naming `mcp:{server}/{tool}` dans le ToolRegistry. HITL à deux niveaux : serveur (`requires_approval`) et agent (`tools_requiring_approval`). Lazy start des sous-processus serveurs.

**Alternatives considérées :** SDK MCP Rust tiers (rejetée — viole Principe #2, crates expérimentales non maintenues), HTTP/SSE uniquement (rejetée — couvre une minorité des serveurs MCP réels), intégration dans `apollia-tools` (rejetée — mélange de responsabilités, viole Principe #5).

**Conséquences :** Écosystème MCP accessible depuis les agents. Serveurs locaux fonctionnent hors-ligne. HITL assure la conformité local-first. Pas de reconnection automatique en V1 (crash = erreur explicite). HTTP/SSE reporté à V2. Nouvelles dépendances workspace : `toml`, `async-trait`.

**Principes impactés :** Principe #1 — Local-first (respecté : stdio local, secrets env, HITL gate), Principe #2 — Zéro dépendance externe (respecté : implémentation native), Principe #5 — Un acteur, une responsabilité (respecté : McpClientManager acteur dédié), Principe #8 — CLI humaine, API machine (respecté : `/mcp/servers` REST).

[Détail → docs/adr/ADR-044-client-mcp.md](adr/ADR-044-client-mcp.md)

---

## ADR-045 — Page Intégrations : wizard générique piloté par les metadata MCP Registry

**Date :** 2026-08-01
**Statut :** Accepté

**Contexte :** Le client MCP (ADR-044) est opérationnel, mais la configuration des serveurs nécessite d'éditer `~/.apollia/mcp.toml` manuellement — bloquant pour les opérateurs non-techniques. Le Sprint 27 ajoute une page Intégrations dans le desktop Apollia. Elle doit permettre la découverte, la configuration, et la gestion sécurisée des secrets (tokens API) pour 16 000+ serveurs MCP sans qu'aucune modification du code soit requise pour chaque nouveau connecteur.

**Décision :** Wizard générique unique (`ConnectorWizard`) piloté dynamiquement par les metadata du MCP Registry officiel (`registry.modelcontextprotocol.io/v0.1/servers`). Aucun composant par connecteur. Les 6 connecteurs les plus courants (Notion, Slack, GitHub, Linear, PostgreSQL, Filesystem) bénéficient d'enrichissements builtin (labels UX, liens doc, valeurs par défaut). Les secrets sont stockés dans le keychain OS via la crate `keyring` et référencés dans `mcp.toml` par le préfixe `APOLLIA_SECRET:<service>/<key>`. Cache local du registry (TTL 24h) pour le mode offline.

**Alternatives considérées :** Wizard par connecteur (rejetée — maintenance infinie, équipe solo, 16 000+ serveurs impossibles à couvrir), Éditeur TOML assisté (rejetée — inaccessible aux opérateurs non-techniques, pas de découverte, secrets en clair).

**Conséquences :** Ajout d'un connecteur ne nécessite aucune modification du code frontend. Secrets isolés dans le keychain OS. Mode offline garanti par le cache. Limites V1 : fallback fichier chiffré local si D-Bus absent (Linux headless), UX dépendante de la qualité des metadata du registry, validation sémantique des paramètres impossible (détectée uniquement par le test de connexion). À surveiller : qualité des metadata du registry, évolution du schéma API, adoption du secret store sur Linux headless.

**Principes impactés :** Principe #1 — Local-first (respecté : secrets dans keychain OS, cache local, mode offline), Principe #2 — Zéro dépendance externe (respecté en mode offline, réseau optionnel), Principe #4 — Fail fast (respecté : test de connexion avant confirmation), Principe #8 — CLI humaine, API machine (respecté : wizard délègue à l'API REST existante).

[Détail → docs/adr/ADR-045-page-integrations-wizard-generique.md](adr/ADR-045-page-integrations-wizard-generique.md)

---

## ADR-046 — Transport HTTP/SSE pour les serveurs MCP distants

**Date :** 2026-03-30
**Statut :** Accepté

**Contexte :** Le Sprint 27 a livré le catalogue MCP et le wizard, mais ~70% des serveurs du registry utilisent des transports distants (streamable-http, SSE) au lieu de packages npm (stdio subprocess). Le backend MCP ne supporte que stdio — la majorité du catalogue est non-installable, y compris les serveurs officiels (Notion, Brave).

**Décision :** Nous adoptons un trait `McpTransport` abstrait dans `apollia-mcp` avec trois implémentations : `StdioTransport` (refactoring existant), `StreamableHttpTransport` (nouveau), `SseTransport` (nouveau). Le transport est sélectionné dynamiquement depuis `McpServerConfig.transport`.

**Alternatives considérées :** Proxy local stdio-to-HTTP (rejetée — processus intermédiaire, latence, complexité lifecycle), HTTP uniquement sans SSE (rejetée — exclut les serveurs SSE-only comme fallback Notion).

**Conséquences :** 100% des serveurs du registry deviennent installables. Refactoring majeur de session.rs (~500 LOC). Le wizard supporte les serveurs distants nativement.

**Principes impactés :** Principe #1 — Local-first (respecté : données locales, appels distants explicites), Principe #5 — Un acteur, une responsabilité (respecté : chaque transport est une implémentation indépendante).

[Détail → docs/adr/ADR-046-transport-http-sse-mcp.md](adr/ADR-046-transport-http-sse-mcp.md)

---

## ADR-047 — Multi-LLM Backend Registry : SQLite + binding par agent

**Date :** 2026-03-31
**Statut :** Accepté

**Contexte :** Apollia OS ne supporte qu'un seul backend LLM, configuré statiquement dans `[llm]` de `apollia.toml`. Impossible d'avoir un agent code-reviewer sur `lm5-code` et un agent mail sur `mistral-small` simultanément. La config LLM est la seule entité runtime encore dans un fichier TOML — incohérent avec SQLite pour les agents, triggers, et MCP.

**Décision :** Table SQLite `llm_backends` dans `system.db` avec n backends enregistrés (un seul `is_default`). `AgentManifest` gagne un champ optionnel `llm_backend: Option<String>`. `LlmRouter` devient multi-backend avec routing `agent_id → backend_name`, fallback sur le défaut. Suppression de `[llm]` dans `apollia.toml`. V1 : tous les backends `enabled` chargés au boot.

**Alternatives considérées :** Variable d'environnement par agent (rejetée — viole Principe #3, impossible à gérer depuis le desktop), fichier de config par agent (rejetée — prolifération de fichiers, même problème d'éditabilité), LLM directement dans AgentManifest (rejetée — API keys dans le Python, catastrophe sécurité).

**Conséquences :** Multi-LLM simultané natif. Config LLM éditable depuis le desktop sans redémarrage. Cohérence SQLite-first. Compromis : LlmRouter refactorisé (risque régression), backends locaux lourds tous chargés au boot (lazy load reporté à V2), AgentManifest étendu (breaking change optionnel).

**Principes impactés :** Principe #1 — Local-first (respecté : secrets via env vars/keyring), Principe #3 — Contrat minimal (respecté : champ optionnel), Principe #4 — Fail fast (warning immédiat si backend nommé introuvable).

[Détail → docs/adr/ADR-047-multi-llm-backend-registry.md](adr/ADR-047-multi-llm-backend-registry.md)

---

## ADR-048 — Worker Agents : expertise de domaine compilée dans le code Python

**Date :** 2026-03-31
**Statut :** Accepté

**Contexte :** MCP (Sprint 26) livre 16K+ outils tiers. Mais pour les tâches de domaine complexes (Excel, CSV, PDF...), l'expertise de séquençage — guardrails, patterns d'erreur, imports corrects — se dégrade significativement sur les modèles 7-14B utilisés par les utilisateurs finaux d'Apollia. La fenêtre de contexte limitée (4K-8K tokens) et la fidélité moindre aux instructions longues rendent l'injection Markdown (style "skills" de Claude) inefficace sur ces modèles.

**Décision :** Nous adoptons le pattern Worker Agent : des agents Python built-in dont l'expertise est compilée dans le code (`SYSTEM_PROMPT` constant, imports, guardrails, patterns d'erreur), pas injectée en contexte LLM. Chaque Worker Agent déclare `packages: list[str]` dans son manifest (installés au `INITIALIZING` via `setup_venv`), étend `WorkerAgent(BaseReActAgent)` du SDK Python, et expose `supports_a2a: True`. Le champ `packages` est ajouté à `AgentManifest` dans `apollia-core`.

**Alternatives considérées :** Skills Markdown (rejetée : dégradation sur modèles 7-14B, guardrails contournables, dépend de l'intelligence du modèle), Outils MCP spécialisés (rejetée : atomiques par nature, ne peuvent pas encoder la séquence et les guardrails domaine).

**Conséquences :** Worker Agents model-agnostic (7B à frontier). Guardrails non-contournables. Fail-fast sur packages manquants. Composable via A2A. Compromis : effort développement par domaine, bibliothèques pip à maintenir, temps venv INITIALIZING.

**Principes impactés :** Principe #3 — Contrat minimal (WorkerAgent est une convention, pas une obligation), Principe #4 — Fail fast (packages → setup_venv au INITIALIZING → Degraded immédiat si absent).

[Détail complet → docs/adr/ADR-048-worker-agents-expertise-domaine.md](adr/ADR-048-worker-agents-expertise-domaine.md)

---

## ADR-049 — Routing A2A inter-agents : discovery + invocation

**Date :** 2026-04-01
**Statut :** Accepté

**Contexte :** Sprint 29 (ADR-048) a posé la fondation Worker Agent (`supports_a2a: True` + `skills` dans les manifests) mais sans routing effectif. Sprint 30 implémente ce routing — cinq questions architecturales doivent être formalisées avant l'implémentation : gestion des conflits de skills, mode d'invocation, format du résultat, trust model mémoire, profondeur de récursivité.

**Décision :** (1) Conflit de skills → erreur `AmbiguousSkill` à la résolution, premier enregistré gagne en cas de coexistence transitoire. (2) Invocation synchrone pour V1, timeout configurable (défaut 120 s). (3) Résultat encapsulé dans `A2aDelegateResult { task_id, output: serde_json::Value }`, aligné sur `AIPResult`. (4) Trust model explicite : le Worker reçoit uniquement le payload transmis par le Director, aucune injection de mémoire automatique. (5) Récursivité non limitée en V1, garde-fous de profondeur planifiés pour Sprint 32. Type alias `A2aDelegateFn` pour contourner la contrainte `#[pyclass]` sans paramètre générique.

**Alternatives considérées :** Résolution par nom d'agent (rejetée — couplage fort), routing via EventBus seul (rejetée — pas de request/response natif), invocation asynchrone V1 (rejetée — complexité injustifiée), injection automatique de mémoire vers le Worker (rejetée — viole ADR-007).

**Conséquences :** Director Agent peut déléguer via `ctx.delegate(skill_id, payload)` sans couplage. Ambiguïté de skills détectée explicitement. Trust model préserve l'isolation des namespaces mémoire. Risque théorique de récursion infinie mitigé par `StepBudget` et timeout A2A.

**Principes impactés :** Principe #5 — Un acteur, une responsabilité (SkillIndex dans AgentRegistry, pas un acteur séparé), Principe #6 — Mémoire à initiative de l'agent (renforcé aux délégations inter-agents), Principe #7 — Garde-fous non-négociables (timeout A2A appliqué par le runtime).

[Détail complet → docs/adr/ADR-049-a2a-routing-inter-agents.md](adr/ADR-049-a2a-routing-inter-agents.md)

---

## ADR-050 — Distribution des Worker Agents : bundled vs communautaire, registre local et Git

**Date :** 2026-04-01
**Statut :** Accepté

**Contexte :** Les Sprints 29-31 livrent quatre Worker Agents sans packaging ni séparation formalisée. Avant d'implémenter le packaging bundled (STORY-410) et le registre communautaire (STORY-411), six questions doivent être tranchées : quels agents sont bundled, format du registre, commande d'installation, validation à l'installation, séparation physique `bundled/` vs `community/`, auto-installation au premier boot.

**Décision :** (1) Agents bundled = excel-worker, csv-data-worker, pdf-worker, code-worker (4 agents couvrant les cas PME généraux, maintenus par Apollia). (2) Registre communautaire V1 = répertoire local `agents/community/<agent-name>/` avec `agent.py` + `manifest.json` + `README.md` ; V2 = repo Git public avec `registry.json` d'index. (3) Installation via `apollia-os agent install <path|git-url>` — synchrone, interactif, confirmation requise. (4) Validation en 4 étapes à l'installation : manifest conforme, scan `dangerous_tools_allowed`, résolution packages pip, smoke test optionnel. (5) Séparation stricte `agents/bundled/` (Apollia) vs `agents/community/` (tiers). (6) Auto-installation des bundled au premier boot via `agents/bundled/registry.json`; venvs pip différés au premier `INITIALIZING`.

**Alternatives considérées :** Agents bundled embarqués dans le binaire (binaire trop lourd), endpoint distant pour les bundled (viole Principe #2), registre centralisé hébergé (infrastructure + point de défaillance), validation lazy au premier `agent start` (viole Principe #4), venv installé au boot (dégrade le démarrage).

**Conséquences :** Runtime fonctionnel hors ligne. Séparation physique lisible. Validation à l'installation non-contournable. `sql-worker` et `git-worker` (Sprint 32) servent de template communautaire. V2 compatible V1 sans migration.

**Principes impactés :** Principe #1 — Local-first (bundled inclus dans le repo, zéro réseau obligatoire), Principe #2 — Zéro dépendance externe (packages pip sont des dépendances de l'agent, pas du runtime), Principe #4 — Fail fast (validation complète à l'installation), Principe #7 — Garde-fous non-négociables (scan `dangerous_tools_allowed` non-contournable en mode interactif).

[Détail complet → docs/adr/ADR-050-distribution-worker-agents.md](adr/ADR-050-distribution-worker-agents.md)

---

## ADR-051 — Authentification API REST TCP : token statique + restriction loopback

**Date :** 2026-04-03
**Statut :** Accepté

**Contexte :** L'API REST TCP `:7771` est ouverte sans aucune authentification depuis le Sprint 5. Toute application locale peut appeler des endpoints destructifs sans preuve d'identité. Avant la beta publique, ce vecteur doit être fermé sans infrastructure externe (Principe #1).

**Décision :** Token statique 32 octets hexadécimaux (256 bits, `rand::rngs::OsRng`) stocké dans `~/.apollia/api-token` avec permissions `0600`. Toutes les requêtes TCP doivent porter `Authorization: Bearer <token>`. Comparaison à temps constant via `subtle::ConstantTimeEq` (pas de timing attack). TCP `:7771` bindé sur `127.0.0.1` par défaut. Socket Unix non authentifiée (permissions filesystem suffisent). Config `api.require_token = true` (défaut), `api.bind = "127.0.0.1"`. Rotation manuelle via `apollia-os config rotate-token`.

**Alternatives considérées :** OAuth2/JWT (trop complexe pour beta locale mono-utilisateur), mTLS (overhead infrastructure injustifié), aucune authentification (vecteur XSS via localhost inacceptable en beta publique), restriction par PID/UID (APIs OS non portables).

**Conséquences :** Sécurité locale suffisante sans infrastructure externe. Backward-compatible (socket Unix non affectée). Les clients TCP existants doivent ajouter le header `Authorization`. Pas de rotation automatique — token compromis persiste jusqu'à intervention manuelle.

**Principes impactés :** Principe #1 — Local-first (token stocké en local, aucun endpoint distant), Principe #4 — Fail fast (permissions incorrectes → erreur au démarrage, requête sans token → 401 immédiat), Principe #8 — CLI humaine, API machine (`apollia-os config show-token` disponible).

[Détail complet → docs/adr/ADR-051-api-auth.md](adr/ADR-051-api-auth.md)

---


## ADR-053 — Pipeline fan-out et boucles conditionnelles

**Date :** 2026-04-03
**Statut :** SUPERSEDED — crate `apollia-pipelines` retirée du workspace v0.1.0

**Contexte :** Le Pipeline Engine (ADR-025) supporte les DAG linéaires. Deux topologies manquent pour les cas PME avancés : fan-out sur tableau (un step produit une liste, chaque élément traité en parallèle) et boucles conditionnelles (plusieurs passes jusqu'à convergence).

**Décision :** (1) Fan-out via `tokio::JoinSet` — step déclare `fan_out = true`, output interprété comme tableau JSON, sous-steps éphémères créés à l'exécution, concurrence bornée par `pipelines.max_fan_out_concurrency` (défaut : 8) ; (2) Boucles via `loop_until` (condition JSONPath) + `max_iterations` obligatoire (garde-fou non-contournable — Principe #7), absence de `max_iterations` = erreur au démarrage ; (3) Nouveau `StepRunStatus::Cancelled { reason: String }` — distinct de `Failed`, enregistré dans l'audit trail ; (4) Step timeout configurable via `timeout_secs` (défaut depuis `pipelines.default_step_timeout_secs`), dépassement → `Cancelled { reason: "timeout" }`.

**Alternatives considérées :** Fan-out séquentiel (bat le but), framework workflow externe (viole Principe #2), boucles infinies avec signal d'arrêt (contournable par l'agent), expansion des nœuds dans le DAG statique (casse le cycle detector).

**Conséquences :** Parallélisme natif Tokio sans thread pool externe. Boucles retry-until-convergence supportées. Sous-steps éphémères non stockés dans le graphe statique. Risque de deadlock fan-out avec `depends_on` circulaire — limitation V1 documentée.

**Principes impactés :** Principe #5 — Un acteur, une responsabilité (sous-steps éphémères hors graphe statique), Principe #7 — Garde-fous non-négociables (`max_iterations` obligatoire, `max_fan_out_concurrency` borné).

[Détail complet → docs/adr/ADR-053-pipeline-fanout-loops.md](adr/ADR-053-pipeline-fanout-loops.md)

---

## ADR-054 — Consolidation mémoire épisodique : report justifié post-v1

**Date :** 2026-04-03
**Statut :** Accepté

**Contexte :** La mémoire épisodique croît sans mécanisme de consolidation depuis le Sprint 3. La littérature (MemGPT, Letta) propose des consolidations automatiques — mais ces approches introduisent du coût LLM non maîtrisé, un comportement imprévisible, et un risque de perte de données pour la beta.

**Décision :** La consolidation automatique de la mémoire épisodique est reportée à post-v1. Garde-fou unique acceptable : troncature `STEP_MEMORY_OUTPUT_MAX_CHARS = 200` (Principe #7, configurable via `oria.step_memory_max_chars`) — borne la taille des épisodes sans modifier leur sémantique. La consolidation sera opt-in, contrôlée par l'agent via `ctx.memory.consolidate()` — jamais déclenchée automatiquement par le runtime (Principe #6). Design préliminaire post-v1 : `[memory.consolidation]` avec `enabled = false`, `interval`, `min_episodes`.

**Alternatives considérées :** Consolidation automatique par LLM (coût caché, viole Principe #6), consolidation par règles (heuristiques fragiles), limite dure FIFO (détruit les épisodes importants), consolidation manuelle déjà possible via `ctx.memory.record()`.

**Conséquences :** Comportement prévisible. Zéro coût LLM caché. La base épisodique croît linéairement — pour des agents long-running, peut atteindre plusieurs centaines de Mo après 1 an. À mesurer en beta.

**Principes impactés :** Principe #6 — Mémoire à initiative de l'agent (renforcé), Principe #7 — Garde-fous non-négociables (troncature `STEP_MEMORY_OUTPUT_MAX_CHARS` seul garde-fou automatique acceptable).

[Détail complet → docs/adr/ADR-054-memory-episodic-consolidation.md](adr/ADR-054-memory-episodic-consolidation.md)

---

## ADR-055 — Community Registry : distribution Git-based peer-to-peer

**Date :** 2026-04-03
**Statut :** Accepté

**Contexte :** ADR-050 a défini la V1 du registry communautaire (installation par path local). La V2 — résolution d'une URL Git → clonage → validation → installation — est implémentée dans STORY-450. Cette ADR formalise l'architecture du registre distribué.

**Décision :** (1) Format : chaque agent communautaire est un repo Git autonome avec `agent.py` + `manifest.json` + `requirements.txt` + `tests/test_smoke.py`. (2) Découverte optionnelle : un `registry.json` dans un repo Git public (ex. `apollia-os/community-registry`) indexe les agents disponibles — optionnel, pas requis pour l'installation directe. (3) Validation 4 étapes ADR-050 inchangées (manifest, `dangerous_tools_allowed`, packages pip, smoke test). (4) Pas de signature cryptographique en V2 — confiance sur URL Git présentée à l'utilisateur ; GPG encouragé mais non requis. (5) Commandes : `agent install <git-url>`, `agent search <keyword>`, `agent list --source community`, `agent update <name>`. Fallback `gitoxide` si Git absent (Windows).

**Alternatives considérées :** Registry HTTP centralisé (point de défaillance, infrastructure, viole Principe #1), npm-style (complexité), PyPI (confusion packages pip / agent), aucun registre distant (bloque l'écosystème communautaire), registry embarqué dans le binaire (agents évoluent plus vite que le runtime).

**Conséquences :** Distribution P2P — pas de serveur central requis. Compatible V1 (path local toujours valide). Découvrabilité limitée sans index. Pas de vérification d'intégrité post-clonage en V2. Repo d'index `apollia-os/community-registry` à modérer avant beta publique.

**Principes impactés :** Principe #1 — Local-first (clonage local, index optionnel), Principe #2 — Zéro dépendance externe (pas de serveur Apollia requis, fallback `gitoxide`), Principe #4 — Fail fast (validation complète à l'installation).

[Détail complet → docs/adr/ADR-055-community-registry.md](adr/ADR-055-community-registry.md)

---

## ADR-056 — Workspace Context, ContextProvider Trait, Memory Namespace & ContextBootstrap

**Date :** 2026-04-04 (workspace + trait) / 2026-04-15 (namespace + bootstrap)
**Statut :** Accepté

**Contexte :** (1) Sprint 35 : l'agent ignore le projet dans lequel il opère — il re-découvre branche git, APOLLIA.md, arborescence à chaque session. (2) Le contexte workspace doit être extensible (providers Rust, Python, scripts). (3) Sprint 39 : `dev-assistant` sur deux projets partage le même namespace mémoire — contamination inter-projets. (4) Sprint 40 : pattern bootstrap copié-collé dans 3 agents sans détection de péremption.

**Décision :** (1) Crate `apollia-workspace` avec `WorkspaceAssembler` (timeout 2s, TTL 30s), `GitContextCollector` (subprocess `git`, pas de `libgit2`), `ApolliamdFinder`, `DirectoryTreeBuilder`. (2) Trait `ContextProvider` dans `apollia-core` — 3 niveaux : Rust natif, duck-typing Python, script stdin/stdout JSON. Distingué de la mémoire (Principe #6) : le Context = situation courante, pas accumulation. (3) Namespace effectif = `"{project_id}:{manifest_namespace}"` si `project_id` est `Some(_)`, sinon namespace tel quel. Transparent pour l'agent Python. (4) `ContextBootstrap` : protocole SDK (`sdk/apollia/bootstrap.py`) avec 2 méthodes abstraites (`is_stale()`, `run_bootstrap()`) + 4 méthodes infrastructure. Opt-in, jamais injecté par le runtime.

**Alternatives considérées :** git2 crate (dépendance dynamique libgit2, Principe #2), WorkspaceAssembler unique (non extensible), namespace déclaré par l'agent (project_id inconnu à l'écriture du manifest), injection automatique bootstrap par le runtime (viole Principe #6).

**Conséquences :** Isolation mémoire complète entre projets. Suppression du pattern bootstrap copié-collé. Économie de tokens (bootstrap payé une fois). Timeout 2s garantit aucun blocage. Données orphelines en mémoire si projet supprimé sans purge.

**Principes impactés :** Principe #2 — Zéro dépendance (pas de libgit2), Principe #3 — Contrat minimal (ContextBootstrap est couche SDK), Principe #6 — Mémoire à initiative de l'agent (renforcé).

[Détail → docs/adr/ADR-056-workspace-context-assembly.md](adr/ADR-056-workspace-context-assembly.md)

---

## ADR-057 — Stratégie de prompt caching

**Date :** 2026-04-04 — **Statut :** Accepté

Activation du prompt caching Anthropic (`cache_control: ephemeral`) sur les sections stables du system prompt (workspace context, APOLLIA.md, tool descriptions). TTL 5 min côté Anthropic. Breakpoints de cache sur les N-1 premiers messages de l'historique en mode chat. Économies estimées : 70-85% sur les re-runs de longues sessions. Cache hits trackés via `LlmCallCompleted.cache_read_tokens`.

[Détail → docs/adr/ADR-057-prompt-caching-strategy.md](adr/ADR-057-prompt-caching-strategy.md)

---

## ADR-058 — Gestion de la fenêtre de contexte

**Date :** 2026-04-04 — **Statut :** Accepté

`ContextWindowManager` estime les tokens du contexte avant chaque appel LLM et tronque si nécessaire : outils supprimés en premier, historique tronqué par le sliding window (ADR-039), workspace context tronqué en dernier. Limite configurable par backend (`context_window_tokens`). Jamais de crash sur dépassement — dégradation gracieuse avec `tracing::warn!`.

[Détail → docs/adr/ADR-058-context-window-management.md](adr/ADR-058-context-window-management.md)

---

## ADR-059 — Exécution concurrente des outils

**Date :** 2026-04-04 — **Statut :** Accepté

Quand le LLM retourne plusieurs `tool_use` dans une même réponse, les appels sont exécutés en parallèle via `tokio::JoinSet`. Chaque outil s'exécute dans son propre spawn. `StepBudget.tool_calls_remaining` décrémenté atomiquement. Limite `max_concurrent_tools = 8` (configurable). Outil qui échoue → `tool_result` avec `is_error: true`, boucle ReAct continue.

[Détail → docs/adr/ADR-059-concurrent-tool-execution.md](adr/ADR-059-concurrent-tool-execution.md)

---

## ADR-061 — Permission Engine 3 layers

**Date :** 2026-04-04 — **Statut :** Accepté

Trois scopes de permissions stockés dans `~/.apollia/governance.db` : Session (in-memory, durée du runtime), Project (persistant, bound au project_id), Global (persistant, tous projets). Le HITL propose 3 boutons : Approuver cette fois (session), Toujours pour ce projet, Toujours. `ToolRegistry` est scope-aware : avant chaque exécution, consulte governance.db. Révocation via `apollia-os permissions revoke`.

[Détail → docs/adr/ADR-061-permission-engine-3-layers.md](adr/ADR-061-permission-engine-3-layers.md)

---

## ADR-062 — Mode serveur MCP

**Date :** 2026-04-04 — **Statut :** Accepté

Apollia OS peut exposer ses propres outils et agents comme un serveur MCP (en plus de consommer des serveurs externes). Transport stdio. Le runtime expose un `McpServerHandler` via `apollia-mcp`. Permet à Claude Code, Cursor et autres clients MCP d'utiliser les outils Apollia nativement.

[Détail → docs/adr/ADR-062-mcp-server-mode.md](adr/ADR-062-mcp-server-mode.md)

---

## ADR-063 — Feedback binaire RLHF

**Date :** 2026-04-04 — **Statut :** Accepté

Bouton 👍/👎 sur chaque réponse agent dans le desktop. Feedback stocké localement dans `feedback.db`. Pas de télémétrie cloud (Principe #1). Les données sont optionnellement exportables pour fine-tuning. API `POST /api/v1/feedback` pour les intégrations CLI.

[Détail → docs/adr/ADR-063-binary-feedback-rlhf.md](adr/ADR-063-binary-feedback-rlhf.md)

---

## ADR-064 — OAuth2 PKCE + keyring

**Date :** 2026-04-04 — **Statut :** Accepté

Authentification OAuth2 avec PKCE dans `apollia-auth`. Tokens stockés dans le keychain OS (`keyring` crate). Refresh automatique. `apollia-os auth login/logout/status`. Pas de stockage de refresh token en clair. Compatible avec les providers tiers qui utilisent OAuth2 (GitHub, Notion, etc.) via la page Intégrations (ADR-045).

[Détail → docs/adr/ADR-064-oauth2-pkce-keyring.md](adr/ADR-064-oauth2-pkce-keyring.md)

---

## ADR-065 — Auto-updater et distribution binaire

**Date :** 2026-04-04 — **Statut :** Accepté

Auto-updater Tauri v2 (`tauri-plugin-updater`) pour le desktop. CLI : `apollia-os upgrade` via curl du binaire signé depuis GitHub Releases. Signatures Ed25519. Canal stable (défaut) + beta opt-in. Pas de mise à jour automatique sans confirmation (Principe #1 — aucune donnée ne sort sans action explicite).

[Détail → docs/adr/ADR-065-auto-updater-distribution.md](adr/ADR-065-auto-updater-distribution.md)

---

## ADR-066 — Format export/import mémoire

**Date :** 2026-04-04 — **Statut :** Accepté

Export : `apollia-os memory export --format json > backup.json` (tableau JSON d'entrées mémoire sérialisées avec namespace, content, confidence, timestamps). Import : `apollia-os memory import backup.json` avec mode `merge` (défaut) ou `replace`. Pas de format binaire propriétaire — JSON lisible à l'œil nu. Inclus dans la stratégie de backup `~/.apollia/`.

[Détail → docs/adr/ADR-066-memory-export-import-format.md](adr/ADR-066-memory-export-import-format.md)

---

## ADR-069 — Autonomie filesystem : friction graduée et journal réversible

**Date :** 2026-04-15 — **Statut :** Accepté

Avant toute écriture ou suppression filesystem hors sandbox, l'agent évalue l'impact (scope : workspace / home / global). Trois niveaux de friction : immédiat (workspace propre), HITL (home), bloqué (global). Journal des opérations filesystem dans `audit.db` avec `undo_payload` — `apollia-os task undo <id>` restaure l'état précédent pour les opérations réversibles.

[Détail → docs/adr/ADR-069-autonomie-filesystem-friction-graduee-journal-reversible.md](adr/ADR-069-autonomie-filesystem-friction-graduee-journal-reversible.md)

---

## ADR-072 — Outils web natifs : web_search + web_read

**Date :** 2026-04-15 — **Statut :** Accepté

Architecture 2-étages : `web_search` (trait `SearchBackend` pluggable, DuckDuckGo HTML scraping par défaut, Brave Search API opt-in via feature flag + clé API) + `web_read` (fetch HTTP + extraction lisible via `dom_smoothie`, SSRF-guarded par liste d'adresses privées bloquées). Activation opt-in dans `apollia.toml`. Résultats bornés (max 5 résultats search, 8K chars web_read).

[Détail → docs/adr/ADR-072-web-tools-architecture.md](adr/ADR-072-web-tools-architecture.md)

---

## ADR-073 — Code signing macOS

**Date :** 2026-04-17 — **Statut :** Accepté

Binaire et `.dmg` signés avec un certificat Apple Developer (Developer ID Application). Notarisation Apple via `xcrun notarytool`. Intégré dans le workflow GitHub Actions `release.yml`. PyO3 requiert que les bibliothèques Python liées soient également signées — script de resign inclus. Sans signature, Gatekeeper bloque l'exécution sur macOS 10.15+.

[Détail → docs/adr/ADR-073-macos-code-signing.md](adr/ADR-073-macos-code-signing.md)

---

## ADR-075 — Chargement multi-fichier GGUF

**Date :** 2026-04-17 — **Statut :** Accepté

`llama-cpp-2` supporte les modèles GGUF fragmentés (`model-00001-of-00004.gguf`). `LlamaModel::load_from_file()` accepte le premier fragment, llama.cpp résout les autres automatiquement. Le `ModelHubDownloader` télécharge tous les fragments d'un seul appel (ADR-080). Validation SHA256 par fragment. Stockés dans `~/.apollia/models/<name>/`.

[Détail → docs/adr/ADR-075-gguf-multi-file-loading.md](adr/ADR-075-gguf-multi-file-loading.md)

---

## ADR-076 — Internationalisation frontend (svelte-i18n, FR/EN)

**Date :** 2026-03-16 / spec complète Sprint 42 — **Statut :** Accepté

`svelte-i18n` v4 avec fichiers JSON plats (`en.json`, `fr.json`) organisés en 13 namespaces. ~1700 clés. Détection locale système au premier lancement via `getLocaleFromNavigator()`. Persistance dans `localStorage`. Script `audit-i18n.mjs` vérifie la parité FR/EN en CI. Convention capitalisation : première lettre majuscule uniquement pour les phrases, tout en minuscule pour les labels.

[Détail → docs/adr/ADR-076-i18n-frontend.md](adr/ADR-076-i18n-frontend.md)

---

## ADR-077 — Design tokens v2

**Date :** 2026-04-03 — **Statut :** Accepté

Refonte du système de tokens : variables CSS HSL custom properties (`--background`, `--foreground`, `--primary` etc.) + fichier TypeScript `tokens.ts` pour les composants Svelte. Mode clair : fond crème chaud (`--background: 38 28% 90%`). Mode sombre : charcoal chaud (`--background: 28 8% 9%`). Bleu primaire `#3435f5`. Système d'élévation 5 niveaux. Rim light accents. Glass morphism. Spec complète dans `docs/wiki/DESIGN-SYSTEM.md`.

[Détail → docs/adr/ADR-077-design-tokens-v2.md](adr/ADR-077-design-tokens-v2.md)

---

## ADR-078 — Meta LLM Orchestrator

**Date :** 2026-04-20 — **Statut :** Accepté

`MetaOrchestrator` Rust qui orchestre des appels LLM secondaires pour des tâches d'analyse interne : classification de tâche (Direct vs Orchestré), génération de coaching `ApolliaCoach`, détection d'étapes suivantes `NextStepsAnalyzer`, parsing d'automatisation `ParseAutomation`. Ces appels utilisent les backends cloud (Anthropic par défaut) et sont comptabilisés séparément dans `llm_calls.db`. Opt-in via `meta_orchestrator.enabled = true`.

[Détail → docs/adr/ADR-078-meta-llm-orchestrator.md](adr/ADR-078-meta-llm-orchestrator.md)

---

## ADR-079 — LLM config DB-first, TOML sync

**Date :** 2026-04-20 — **Statut :** Accepté

La configuration LLM backend (models, endpoints, API keys env vars) est migrée de `apollia.toml` vers `~/.apollia/system.db`. `apollia.toml` ne contient plus de section `[llm]`. Au boot, si des sections `[[llm.backends]]` existent dans le TOML, elles sont importées dans SQLite et la section est supprimée (migration one-shot). L'app desktop peut configurer les backends LLM sans éditer de fichier.

[Détail → docs/adr/ADR-079-llm-db-first-toml-sync.md](adr/ADR-079-llm-db-first-toml-sync.md)

---

## ADR-080 — Model Hub : intégration Hugging Face

**Date :** 2026-04-22 — **Statut :** Accepté

`ModelHubDownloader` dans `apollia-llm` : requêtes à l'API HF (liste des fichiers GGUF d'un repo, téléchargement avec barre de progression). Authentification optionnelle via `HF_TOKEN`. Validation SHA256 après téléchargement. `apollia-os model download <repo_id>` ou via le desktop (page LLM). Filtrage automatique des fichiers `.gguf` par quantisation (Q4_K_M par défaut).

[Détail → docs/adr/ADR-080-model-hub-hf-integration.md](adr/ADR-080-model-hub-hf-integration.md)

---

## ADR-082 — Tool Governance Architecture

**Date :** 2026-04-25 — **Statut :** Accepté

Unification de la gouvernance des outils dans `~/.apollia/governance.db`. `ToolRegistry` est scope-aware : lit les permissions avant chaque exécution. Trois scopes HITL (`session`, `project`, `global`) exposés via 3 boutons dans l'UI d'approbation. `apollia-os permissions list/revoke/audit` pour la CLI. Les permissions globales survivent aux restarts. Les permissions session sont perdues au shutdown.

[Détail → docs/adr/ADR-082-tool-governance-architecture.md](adr/ADR-082-tool-governance-architecture.md)

---

## ADR-083 — Trust model des agents Python (v0.1.0)

**Date :** 2026-04-29 — **Statut :** Accepté

Les agents Python sont traités comme du code utilisateur de confiance, exécutés avec les droits du processus runtime (donc de l'utilisateur courant). Pas de sandbox process-per-agent en v0.1.0 — la cible builders avancés audite son propre code. SEC-02 (sandbox OS) reste roadmap v1.0. Bandeau onboarding obligatoire : *« Apollia OS exécute du code Python avec vos droits utilisateur — n'installez que des agents que vous avez audités. »* Marketing v0.1.0 ne doit jamais sous-entendre une isolation forte.

[Détail → docs/adr/ADR-083-trust-model-python-agents.md](adr/ADR-083-trust-model-python-agents.md)

---

## ADR-084 — Windows hors scope v0.1.0 et v1.0

**Date :** 2026-04-29 — **Statut :** Accepté

Windows n'est pas supporté en v0.1.0 ni en v1.0. Le binaire ne build pas sur Windows (Unix socket axum, notify-rust D-Bus/NSUserNotificationCenter, llama.cpp Metal/CUDA, sandbox tools POSIX). Documentation publique, site vitrine et annonce ne mentionnent ni « Windows » ni « cross-platform ». Réévaluation possible v1.x si demande communautaire significative — décision tracée dans un ADR ultérieur.

[Détail → docs/adr/ADR-084-windows-hors-scope-v1.md](adr/ADR-084-windows-hors-scope-v1.md)

---

## ADR-085 — Pipeline engine TOML supprimé de v0.1.0

**Date :** 2026-04-29 — **Statut :** Accepté

Le crate `apollia-pipelines` (TOML déclaratif, topologies DAG natives, HITL — ADR-025/ADR-053) est retiré de la v0.1.0. Le composant fonctionnait mais doublait la composition dynamique via A2A (ADR-066), plus alignée avec l'état de l'art 2026 et les besoins early adopters. Code, docs publiques, diagrammes, templates et corpus help associés sont supprimés. ADR-025 et ADR-053 sont conservés pour traçabilité historique. Rebuild prévu en v1.0 sur une spec n8n-like (workflow visuel + step library) si la demande "workflow fixe versionnable" devient prioritaire.

---

## ADR-086 — Permissions agent-driven : `governance.db` comme source unique

**Date :** 2026-04-29 — **Statut :** Accepté

Abandon d'une story prévoyant un *derivation engine* Rust qui mappait automatiquement le profil onboarding (`user.constraints.sovereignty`, `user.agents.hitl`, `user.tech.integrations`) vers des règles `governance.db`. Trois raisons : (1) violation du principe #6 — la mémoire utilisateur devenait un effet runtime invisible ; (2) incompatibilité technique réelle — `RuleAction` ne supporte ni `Approval` ni wildcard `tool_name="*"`, le derivation engine aurait imposé une réécriture du moteur de permissions ; (3) conflit avec la value proposition — un mapping déterministe est l'inverse de la philosophie ReAct (cf. ADR-085). L'onboarding-agent reçoit deux nouveaux outils natifs (`permission_rule_add`, `permission_rule_list`) et **propose** explicitement les règles via HITL. La `SafeList` du TOML est ingérée dans `governance.db` au boot avec `created_by="config-import"`. `governance.db` devient l'unique source de décision runtime ; le champ `created_by` discrimine l'auteur (`onboarding-agent`, `user-hitl`, `user-settings`, `config-import`).

[Détail → docs/adr/ADR-086-permissions-agent-driven-source-unique.md](adr/ADR-086-permissions-agent-driven-source-unique.md)

## ADR-087 — Profil utilisateur canonique avec schéma déclaratif

**Date :** 2026-05-11 — **Statut :** Accepté

Refonte de la mémoire utilisateur globale (namespace `__user__`) en un profil canonique unique, livrée comme V1 propre (pas d'utilisateurs en production ⇒ aucun code legacy, aucune rétrocompat, aucune migration conservée). Constat : le backend exposait 4 catégories (dont `Profile` morte), 4 sources, un score `confidence`, un badge `validated` — pour ~4 entrées réellement écrites en production. L'UI redoublait cette complexité (`UserMemoryDashboard.svelte` avec chips + confidence bars + badges) en parallèle d'une page `settings/Profile.svelte` form-based déjà en place. Décision : (1) schéma déclaratif central `PROFILE_SCHEMA` Rust (~15 champs canoniques, 4 sections d'affichage, flag `sensitive`) ; (2) clés plates en stockage `__user__.semantic_memories` ; (3) API SDK `ctx.profile.{name|role|get|has|all|set|update}` — unique surface ; (4) `WrittenBy { Onboarding | User | Agent(name) }` remplace `UserMemorySource` ; (5) UI unifiée — `Paramètres → Profil` est l'unique surface d'édition, tab `user_memory` supprimé de la page Mémoire. **Supprimés** : `remember_user`, fallback `recall("user.X")` vers `__user__`, `UserMemoryCategory`/`UserMemorySource`/`UserMemoryEntry`, méthodes legacy du repo, IPC `validate_user_memory`, commandes legacy `get_user_memory_profile`/`update_user_memory_entry`/`delete_user_memory_entry`/`clear_user_memory`/`search_user_memory`, routes HTTP `/api/v1/user/*`, types `apollia_core::user::*`, fichier `commands/user.rs`. ADR-038 amendé (non superseded).

[Détail → docs/adr/ADR-087-user-profile-redesign.md](adr/ADR-087-user-profile-redesign.md)

---

## ADR-088 — Architecture hybride : connecteurs natifs + MCP officiels

**Date :** 2026-05-12 — **Statut :** Proposé

Pour matérialiser la promesse local-first ("vos données restent sur votre machine, l'agent s'y connecte vraiment"), la v0.1.0 mixe **connecteurs natifs Rust + OAuth** pour Google Workspace et Microsoft 365 (deux SaaS sans MCP officiel maintenu par l'éditeur) et **MCP officiels intégrés au catalogue** pour les autres (Notion, Slack, GitHub, Linear, Atlassian Rovo, Stripe, Figma, Sentry, Cloudflare). Frontière économique : les SaaS qui publient leur propre MCP transfèrent la maintenance externe ; ceux qui ne le font pas (Google, Microsoft) nécessitent du custom maison où les concurrents (Dust, Claude) facturent leur valeur perçue. Salesforce/HubSpot reportés post-v0.1.0.

[Détail → docs/adr/ADR-088-architecture-hybride-connecteurs-natifs-mcp.md](adr/ADR-088-architecture-hybride-connecteurs-natifs-mcp.md)

## ADR-089 — Client MCP OAuth 2.1 conforme (RFC 9728 + 8707 + CIMD)

**Date :** 2026-05-12 — **Statut :** Proposé

Extension de `apollia-auth` avec un module `mcp_oauth` qui implémente le flow OAuth 2.1 spec MCP 2025-11-25 : RFC 9728 PRM discovery, RFC 8414 + OIDC AS metadata, RFC 8707 Resource Indicators (MUST), CIMD prioritaire avec hébergement statique sur `https://apollia.fr/.well-known/mcp-client-metadata`, DCR (RFC 7591) en fallback, PKCE S256 obligatoire, singleflight refresh via DashMap pour éviter les rate-limit cascade. N'importe quel MCP server HTTP officiel se connecte sans configuration.

[Détail → docs/adr/ADR-089-mcp-oauth-21-rfc-9728-rfc-8707-cimd.md](adr/ADR-089-mcp-oauth-21-rfc-9728-rfc-8707-cimd.md)

## ADR-090 — Abstraction `Connector` trait dans `apollia-connectors`

**Date :** 2026-05-12 — **Statut :** Proposé

Nouveau crate `apollia-connectors` avec un trait `Connector` minimal (4 méthodes : `id`, `manifest`, `operations`, `check`) qui rend l'ajout d'un futur connecteur (Salesforce, HubSpot, Asana en v0.2+) mécanique : un module + une impl du trait + déclaration `OperationSpec` + enregistrement build-time dans `ConnectorRegistry`. HTTP client centralisé avec retry exponential + 401-refresh-once + 429-Retry-After. Plugin dynamique (.so/WASM) explicitement rejeté en v0.1.0 ; build-time only.

[Détail → docs/adr/ADR-090-connector-trait-apollia-connectors.md](adr/ADR-090-connector-trait-apollia-connectors.md)

## ADR-091 — Catalogue MCP : statique → registry → marketplace + override user-side

**Date :** 2026-05-12 — **Statut :** Proposé

v0.1.0 livre un catalogue **statique enrichi de 18 entrées** dans `crates/apollia-desktop/src/mcp/enrichments.json`, avec un champ `cost_model` obligatoire (`free` / `freemium` / `paid` — aucune entrée `paid` en v1). Un mécanisme **override user-side** via `~/.apollia/mcp-overrides.json` (clés `add` / `disable` / `override` JSON deep-merge) permet aux power users de patcher le catalogue sans attendre une release Apollia. Roadmap : v0.3 = registry remote dynamique, v0.4+ = marketplace communautaire signé. Schéma stable cross-paliers.

[Détail → docs/adr/ADR-091-catalogue-mcp-statique-registry-marketplace.md](adr/ADR-091-catalogue-mcp-statique-registry-marketplace.md)

## ADR-092 — Spec exposition `resources` MCP côté agent ReAct

**Date :** 2026-05-12 — **Statut :** Proposé

Les `resources` MCP sont exposées par **deux voies complémentaires, jamais auto-injectées** (conformément au principe #6 — mémoire à initiative de l'agent) : (1) **voie agent** via deux tools implicites `mcp_resources.list` + `mcp_resources.read` que l'agent ReAct appelle de sa propre initiative ; (2) **voie utilisateur** via le sélecteur @-mention du desktop — l'utilisateur épingle explicitement une resource, elle devient un message system prefix au tour suivant. Notifications `resources/updated` invalident le cache mais ne déclenchent aucune ré-injection automatique.

[Détail → docs/adr/ADR-092-exposition-resources-mcp-cote-agent-react.md](adr/ADR-092-exposition-resources-mcp-cote-agent-react.md)

## ADR-093 — `sampling` MCP avec HITL pré-approval

**Date :** 2026-05-12 — **Statut :** Proposé

`sampling/createMessage` (serveur MCP → client demandant un appel LLM) est routé via `apollia-llm::LlmRouter` **avec HITL pré-approval obligatoire** : le prompt complet + l'identifiant du serveur source apparaissent dans l'inbox sous `HITLCard` (composant existant réutilisé sans modification). L'utilisateur approuve ou refuse avant exécution. **Rate limiting** : 100 sampling/heure par serveur source par défaut, configurable — empêche le DoS budget d'un serveur malveillant. Aligné avec les recommandations spec MCP §sampling.

[Détail → docs/adr/ADR-093-sampling-mcp-hitl-pre-approval.md](adr/ADR-093-sampling-mcp-hitl-pre-approval.md)

## ADR-094 — Linux keyring fallback strategy

**Date :** 2026-05-12 — **Statut :** Proposé (Option A provisoirement retenue)

Sur Linux headless (server, container, distros minimales sans Secret Service daemon), le crate `keyring` Rust échoue à l'initialisation. Décision provisoire : **Option A — `age` symétrique avec passphrase utilisateur** (`X25519` / `scrypt`, lib `rage`). Active via `APOLLIA_TOKEN_STORAGE=file` + `APOLLIA_TOKEN_PASSPHRASE`. Pro : zéro dépendance système, fonctionne identiquement partout, crypto auditée. Con : exige une passphrase à l'init (prompt-once + cache acteur prévu). Option B (system-keyring-with-prompt D-Bus) rejetée pour non-respect du principe #4 fail-fast (échoue silencieusement sur certaines distros). Implémentation différée à M1.5.

[Détail → docs/adr/ADR-094-linux-keyring-fallback-strategy.md](adr/ADR-094-linux-keyring-fallback-strategy.md)

## ADR-095 — Orchestration MCP HTTP OAuth de bout en bout

**Date :** 2026-05-15 — **Statut :** Implémenté (en attente de validation utilisateur)

L'ADR-089 a posé les primitives OAuth MCP (`parse_www_authenticate`, `McpDiscoveryClient`, CIMD const, PKCE, callback) sans jamais les câbler entre elles, sans persistance MCP-server-scoped, sans resolver dynamique côté transport, sans IPC, et sans UI wizard adaptée. Conséquence : 8 MCPs HTTP du catalogue non-fonctionnels, 1 (Figma) avec écran vide. Décision : implémenter un **orchestrateur générique** dans `apollia-auth` (`negotiate_token` + `ensure_fresh_token`) qui enchaîne RFC 9728 → 8414 → CIMD/DCR → PKCE → exchange RFC 8707 sans aucun code spécifique provider, persiste via `SecretStorage` existant sous clé `mcp_oauth:{server_name}`, expose un placeholder dynamique `${APOLLIA_OAUTH}` au resolver de `apollia-mcp::config`, unifie le router callback loopback (`/callback` + `/oauth/callback` même listener), et étend `test_mcp_connection` avec un enum `Success | OauthRequired | Error` qui dirige le wizard vers 3 modes UI auto-détectés (NoAuth / StaticToken / OAuth avec scope selector). Singleflight via `tokio::sync::Mutex` par `server_name` pour les refreshes concurrents. Un compte par MCP server en v0.1.0, multi-comptes reporté. Plan 4.2j en 6 phases.

[Détail → docs/adr/ADR-095-mcp-oauth-orchestrator-end-to-end.md](adr/ADR-095-mcp-oauth-orchestrator-end-to-end.md)

## ADR-098 — Apollia AgentKit : decorator-first, agent unifié

**Date :** 2026-05-19 — **Statut :** Accepté

Refonte du SDK Python : suppression complète de la hiérarchie `BaseReActAgent` (676 LOC) / `WorkerAgent` (125 LOC) / `ConversationalAgent` (126 LOC) / `OrchestratedAgent` (103 LOC) — soit 1 030 LOC d'héritage défensif (`getattr`, `try/except`, doubles dispatchers). Une seule classe décorée `@agent(name, version, …)` + décorateurs additifs de méthode `@skill`, `@on_message`, `@orchestrated`. ReAct devient une utility runtime (`ctx.react(...)`) plutôt qu'une classe parente. Constat business : un worker multi-skill passe de ~1 100 LOC à ~150 LOC sur le portage cible. Composition libre (un agent = N skills + on_message + orchestrated dans la même classe), introspection statique au load (alimente `apollia inspect` ADR-110), tests Python = `pytest` standard. Breaking change total sans shim — assumé. Alternatives rejetées : conserver `BaseReActAgent` comme parent unique (ne résout rien), mixins composables (MRO piégeux), DSL YAML (trahit principe #3).

[Détail → docs/adr/ADR-098-apollia-agentkit-decorator-first.md](adr/ADR-098-apollia-agentkit-decorator-first.md)

## ADR-099 — Signature inference comme schéma I/O

**Date :** 2026-05-19 — **Statut :** Accepté

La signature Python des handlers décorés (`@skill`, `@on_message`, `@orchestrated`) devient la **source unique** du schéma I/O des agents. Plus de `def manifest()` côté agent, plus de TypedDict obligatoire, plus de validation `payload.get(...)` manuelle. Le SDK introspecte `inspect.signature` + `typing.get_type_hints` au moment du `@agent`, génère le JSON Schema input/output, valide les payloads entrants côté boundary, et populate `__apollia_manifest__`. Types supportés sans config : `str`, `int`, `float`, `bool`, `bytes`, `list[T]`, `dict[str, T]`, `T | None`, `Literal[...]`, `Enum`, `datetime`, `Path`. Fallback `@dataclass` / `TypedDict` pour cas complexes (toujours stdlib). Docstring Google style → descriptions par champ. Mesure : ~700 LOC de validation payload supprimées sur les agents bundled. Alternatives rejetées : Pydantic (viole principe #2), JSON Schema dans docstring (fragile à parser), manifest TOML source de vérité (statu quo, mismatchs persistants).

[Détail → docs/adr/ADR-099-sdk-signature-inference-schema.md](adr/ADR-099-sdk-signature-inference-schema.md)

## ADR-100 — Exceptions typées au boundary, AIPResult interne

**Date :** 2026-05-19 — **Statut :** Accepté

L'agent ne construit plus `AIPResult.completed/failed/input_required` (~340 occurrences dans le repo, ~210 LOC boilerplate × 5 workers = ~1 050 LOC dupliquées). Il **lève des exceptions typées** depuis `apollia.errors` : `DomainError(code, message, details)`, `PayloadError(field, message)`, `NeedHumanInput(prompt, context)`, `BudgetError`, `PermissionError`. Le SDK boundary (`_internal/dispatch.py`) trap ces exceptions au dispatch et formate l'`AIPResult` à la sortie. Retour normal de handler = `dict` métier ou `None` → enveloppé en `AIPResult.completed(data=...)`. Exceptions non-typées (ex. `KeyError`) → `DomainError("UNHANDLED", ...)` avec traceback loggé par `ctx.logger`. L'agent ne manipule plus jamais `AIPResult`. Cohérence avec FastAPI `HTTPException`. Alternatives rejetées : Result `Ok/Err` (verbeux en Python), sous-classer `AIPResult` (ne supprime aucune duplication), décorateur `@with_result` (déplace la magie).

[Détail → docs/adr/ADR-100-sdk-exceptions-au-boundary.md](adr/ADR-100-sdk-exceptions-au-boundary.md)

## ADR-101 — `ctx` exhaustif et typé via `Protocol`

**Date :** 2026-05-19 — **Statut :** Accepté

Le `ctx` injecté dans les agents Python est restructuré en **14 services nestés typés via `Protocol` PEP 544** : `ctx.llm`, `ctx.react`, `ctx.memory`, `ctx.profile`, `ctx.tools`, `ctx.a2a`, `ctx.datasources`, `ctx.templates`, `ctx.secrets`, `ctx.events`, `ctx.logger`, `ctx.budget`, `ctx.notify`, `ctx.stt`, `ctx.workspace`. Constat : la surface actuelle est plate (~25 méthodes au même niveau, dont `ctx.send`/`ctx.receive`/`ctx.a2a_invoke`/`ctx.delegate` en parallèle de `ctx.llm` nested), divergente entre stubs SDK et runtime Rust, et truffée de `getattr(ctx, "emit_X", lambda *a: None)` défensifs (cf. `react.py:_emit_safe`). Le Protocol devient l'**unique source de vérité** — le runtime Rust DOIT exposer les 14 services exactement, divergence = fail au load. `mypy --strict` passe sur un agent moyen post-migration. Mock testing trivial (structural typing). Alternatives rejetées : dict-like `ctx["llm"]` (zéro IDE), ABC héritage (force mock à hériter), `ctx` plat élargi (devient ingérable au-delà de 40 entrées).

[Détail → docs/adr/ADR-101-sdk-ctx-protocol-exhaustif.md](adr/ADR-101-sdk-ctx-protocol-exhaustif.md)

## ADR-102 — API A2A unifiée (`ctx.a2a`)

**Date :** 2026-05-19 — **Statut :** Accepté

Suppression des 3 APIs A2A concurrentes actuelles (`ctx.send`/`receive` mailbox, `ctx.delegate`, `ctx.a2a_invoke`/`a2a_discover`/`a2a_list_skills` éparpillés racine). Remplacement par un seul service `ctx.a2a` exposant 4 méthodes : `invoke(skill_id, **kwargs)`, `discover(skill_id) -> SkillDescriptor`, `list_skills(agent=None)`, `skill_as_tool(skill_id) -> ToolDescriptor`. `invoke()` se comporte en idiome caller — lève `DomainError("A2A_<CODE>", ...)` côté caller si le skill cible a échoué (cohérent avec ADR-100), retourne le dict métier sur succès. `skill_as_tool()` ouvre le pattern director ReAct ↔ workers (un director découvre dynamiquement les skills disponibles et les présente comme tools au LLM via `ctx.react(tools=[...])`). Alternatives rejetées : conserver 3 APIs en deprecation (pérennise confusion), callable unique `ctx.a2a(skill_id)` (perte de discover/list), objets typés `A2AClient(target_agent)` (re-couple director↔worker).

[Détail → docs/adr/ADR-102-sdk-a2a-api-unifiee.md](adr/ADR-102-sdk-a2a-api-unifiee.md)

## ADR-103 — Datasources YAML et templates Jinja2 accessibles au runtime

**Date :** 2026-05-19 — **Statut :** Accepté

Les datasources (YAML versionnés) et templates (Jinja2) existent dans le packaging des agents mais sont **invisibles au runtime Python** — les agents les lisent via `ctx.tools.invoke("file_read", ...)` puis parsent manuellement (avec `import yaml` qui contredit principe #2). Les templates Jinja2 sont cosmétiques (aucun agent ne s'en sert). Décision : exposer deux nouveaux services `ctx.datasources.get(name)` (cache LRU, parsing `serde_yaml` côté Rust) et `ctx.templates.render(name, **vars)` (rendu `minijinja` v2 ajouté au workspace, sandboxé). Gating manifest obligatoire via `@agent(datasources=("topics", ...), templates=("digest", ...))`. Fail-fast au load (YAML invalide ou datasource déclarée manquante = refus de démarrer). Boucle le pilier #3 / #1 du business model agent forge (livrables prestation = code + templates + datasources + règles + README). Alternatives rejetées : status quo `file_read` (ne résout rien), PyYAML+jinja2 Python (viole #2), chargement bloquant global (mémoire+gating cassé).

[Détail → docs/adr/ADR-103-sdk-datasources-templates-runtime.md](adr/ADR-103-sdk-datasources-templates-runtime.md)

## ADR-104 — API secrets read-only via gating manifest

**Date :** 2026-05-19 — **Statut :** Accepté

Nouveau service `ctx.secrets.get(key)` lecture-seule branché sur `ToolCredentialStore` (AES-256-GCM existant, ADR-082). Gating manifest obligatoire : `@agent(secrets=("brave_api_key", "openweather_api_key", ...))` — l'agent ne voit QUE les clés explicitement déclarées. Synchrone (<1ms keyring lookup). Pas d'écriture (config = `apollia tools config <key>=<value>` humain). Le store gagne un namespace : `tool:<id>:<key>` pour builtin Rust (existant), `agent:<id>:<key>` pour Python (nouveau). UI desktop "Settings → Outils" affiche les deux sections. **Tokens OAuth (Gmail/Calendar/Drive) explicitement reportés v1.1** — l'agent ne reçoit JAMAIS un `access_token` brut en v1.0 ; il passe par les connecteurs natifs (`ctx.tools.invoke("gmail.list", ...)`) qui refresh en interne (ADR-090). Restriction volontaire alignée avec trust model ADR-083. Alternatives rejetées : tous secrets sans gating (casse trust model), secrets dans manifest TOML (mélange code/config).

[Détail → docs/adr/ADR-104-sdk-secrets-read-only-gating.md](adr/ADR-104-sdk-secrets-read-only-gating.md)

## ADR-105 — Events publics typés (`ctx.events`)

**Date :** 2026-05-19 — **Statut :** Accepté

Sortie du pattern défensif actuel `getattr(ctx, "emit_thought", lambda *a: None)` (cf. `sdk/apollia/agents/react.py:187` `_emit_safe`). Service `ctx.events` typé via `Protocol`, exposant 8 méthodes explicites : `emit_token(delta)`, `emit_thought(text, step)`, `emit_action(name, args, step)`, `emit_observation(result, step)`, `emit_retry(step, reason, count)`, `emit_action_parse_error(step, raw, fatal)`, `emit_progress(message, ratio)`, `emit_warning(code, message, details)`. Tous synchrones non-async (mpsc::send côté Rust). No-op gracieux si non-branché (testing) — `NullEventsService` injecté. Ajoute 4 events absents en v0.4.0 (`action`, `observation`, `progress`, `warning`) — directement utiles au builder mode "plus transparent que Claude.ai" (cf. mémoire `project_sprint42_frontend`). Pas d'event custom inventable côté agent (passer par `ctx.logger.info`). Liste fermée à 8 events ; ajout = mineur SemVer SDK. Alternatives rejetées : conserver getattr défensif (zéro typage), pub/sub topic strings (abstraction inutile), single `emit(kind, **data)` générique (zéro autocomplete).

[Détail → docs/adr/ADR-105-sdk-events-types-publics.md](adr/ADR-105-sdk-events-types-publics.md)

## ADR-106 — Logging structuré via `ctx.logger`

**Date :** 2026-05-19 — **Statut :** Accepté

Remplacement de `ctx.log(level: str, message: str)` (level string propice aux typos, pas de structured fields, blob string opaque côté tracing Rust) par `ctx.logger`, un `logging.Logger` stdlib pré-configuré au nom hiérarchique `apollia.agent.<agent_name>`. Handler custom `ApolliaTracingHandler` (`logging.Handler`) qui convertit chaque `LogRecord` en `tracing::event!` côté Rust via PyO3, en préservant les `extra` fields stdlib comme champs tracing structurés. Naming hiérarchique permet le filtering CLI (`apollia logs --agent veille-ia`) et la config log-level par agent via `~/.apollia/config.toml`. Champs auto-ajoutés à chaque log : `agent_id`, `task_id`, `step_id`. `print()` agent capturé et redirigé vers `ctx.logger.info` avec préfixe `[stdout]` — migration sans perte des agents legacy. Stdlib only (principe #2). Alternatives rejetées : conserver `ctx.log` + `extra` (signature custom à enseigner), wrapper opinionné `ctx.logger.info(message, **fields)` (réinvente stdlib mal), OpenTelemetry Python (viole #2).

[Détail → docs/adr/ADR-106-sdk-logger-structure.md](adr/ADR-106-sdk-logger-structure.md)

## ADR-107 — `@agent` instancie et expose `agent` au module

**Date :** 2026-05-19 — **Statut :** Accepté

Le décorateur `@agent(...)` instancie automatiquement la classe décorée et expose l'instance comme attribut `agent` du module (`module.agent = cls()`). L'auteur n'écrit plus `agent = MyClass()` à la fin du fichier (boilerplate présent dans 100 % des agents bundled, source de bugs silencieux quand oublié). Le contrat runtime `getattr(module, "agent")` du bridge PyO3 (ADR-014) est **strictement préservé** — seul le mécanisme de production de l'attribut change. Règles : (1) une seule classe `@agent` par module (RuntimeError si double), (2) `__init__` sans arguments obligatoires (sinon fail-fast au load), (3) le décorateur retourne la classe (pas l'instance), permettant `worker_for_test = MyWorker()` en test sans collision, (4) imports absolus toujours obligatoires (cf. mémoire `feedback_apollia_python_imports`). Alternatives rejetées : statu quo explicite (boilerplate redondant), retourner l'instance (casse isinstance), macro `apollia.run_as_main()` (pire que ligne actuelle).

[Détail → docs/adr/ADR-107-sdk-auto-module-instance.md](adr/ADR-107-sdk-auto-module-instance.md)

## ADR-108 — Suppression de la mailbox A2A `ctx.send/receive`

**Date :** 2026-05-19 — **Statut :** Accepté

Suppression sans remplacement des méthodes `ctx.send(to_agent, message)` et `ctx.receive(timeout)` (fire-and-forget mailbox introduite sprint 22). Audit : aucun agent bundled ne les utilise (`grep -r "ctx\.send\|ctx\.receive" agents/` = 0 résultats), aucun test SDK, aucune doc book/wiki, sémantique floue (push vs poll ? TTL ? persistance ?), conflit conceptuel avec `ctx.a2a.invoke` (ADR-102). `ctx.a2a.invoke` couvre 100 % des cas synchrones inter-agents. Usages asynchrones fire-and-forget reportés à v2.0 sous forme d'un vrai event bus spec'd (si demande émerge — aucun signal aujourd'hui). Suppression bridge PyO3 (`apollia-aip/src/context.rs`), suppression stub (`sdk/apollia/stubs/context.py:155-179`), suppression acteur mailbox côté runtime si présent. Pas de shim, pas de deprecation window. Alternatives rejetées : conserver+documenter (zone confusion pérennisée), renommer en `ctx.a2a.notify` (réintroduit notion mailbox non-spec'd), alias deprecated (sémantique différente d'invoke).

[Détail → docs/adr/ADR-108-sdk-mailbox-a2a-suppression.md](adr/ADR-108-sdk-mailbox-a2a-suppression.md)

## ADR-109 — `AIPResult` devient interne au SDK

**Date :** 2026-05-19 — **Statut :** Accepté

`AIPResult` n'est plus injecté magiquement dans `run.__globals__` par le bridge Rust (const `AIP_TYPES_PY` ~70 LOC dans `crates/apollia-aip/src/bridge.rs:41-110` + injection `~30 LOC` à chaque `call_run()` ligne 295-310, supprimées). Le SDK Python construit le résultat **côté Python** (`sdk/apollia/_internal/aip_result.py` + `_internal/dispatch.py`) à partir du return value du handler (= `AIPResult.completed(data)`) ou de l'exception trappée (`DomainError` → `failed`, `NeedHumanInput` → `input_required`, autres → `failed("UNHANDLED")` + log traceback via `ctx.logger`). Le shape JSON reste strictement aligné avec `apollia_core::AIPResult` côté Rust (round-trip testé). `AIPResult` n'est pas importable depuis `apollia.*` — l'agent ne le voit jamais. Cohérent avec ADR-100 (exceptions au boundary). Élimine les ~340 occurrences `AIPResult.X` et les `# noqa` associés. Alternatives rejetées : alias importable (maintient deux façons), conserver injection sans usage agent (code mort dans bridge), `AIPResult` sealed/Final documenté "interne" (demi-mesure).

[Détail → docs/adr/ADR-109-sdk-aip-result-interne.md](adr/ADR-109-sdk-aip-result-interne.md)

## ADR-110 — Commande `apollia inspect <agent.py>`

**Date :** 2026-05-19 — **Statut :** Accepté

Nouvelle commande CLI `apollia inspect <chemin_agent>` qui charge le module Python en isolation (sans démarrer bridge Rust ni runtime), introspecte `agent.__apollia_manifest__` (généré par `@agent` ADR-098/107), et affiche un rapport complet : manifest, skills (id + description + JSON Schema input/output issu de l'inférence ADR-099), packages requis, datasources/templates/secrets déclarés vs configurés/présents (ADR-103/104), permissions tools croisées avec catalogue, warnings et erreurs. Validation systématique : unicité skill_id, signatures inférables, YAML datasources parsable, templates Jinja2 présents, secrets configurés dans le store local. Codes retour : `0` OK, `1` inspection error, `2` arg/file error. Output `--json` pour pipelines/IDE/Tauri. Feedback < 1s (vs cycle ~5-10s "install + start + invoke"). Use cases : pre-commit hook, CI (`.github/workflows`), dev quotidien builder, UI `Install Package Dialog`. Matérialisation directe du principe #4 fail-fast au niveau ergonomique. Alternatives rejetées : validation au boot runtime seulement (statu quo), outil externe `apollia-lint` séparé (duplique), UI desktop only (ne sert pas CI).

[Détail → docs/adr/ADR-110-apollia-inspect-cli.md](adr/ADR-110-apollia-inspect-cli.md)

## ADR-111 — Vision API typée + memory export/import

**Date :** 2026-05-19 — **Statut :** Accepté

Deux capabilities runtime existantes non exposées proprement côté SDK Python. (1) **Vision** : TypedDicts publics `LlmMessage`, `MessageContent`, `TextContent`, `ImageContent` ajoutés dans `apollia.types.llm` ; helpers `text(s)`, `image_from_path(path)`, `image_from_bytes(data, mime)`, `image_from_url(url)`. Routing automatique côté `apollia-llm` vers Anthropic vision / OpenAI gpt-4o / Vertex Gemini selon le provider. **Local llama-cpp text-only** (cf. mémoire `project_local_llm_engine`) — `ctx.llm.complete()` lève `DomainError("VISION_UNSUPPORTED")` au boundary si `ImageContent` détecté en mode local. (2) **Memory export/import** : `ctx.memory.export() -> dict` JSON-sérialisable + `ctx.memory.import_data(data)`, bouclant ADR-066 jamais branchée côté SDK. Round-trip testé. Cas d'usage : tests d'agent avec fixtures, checkpoint long-running, migration agent v1→v2. Format strictement aligné ADR-066 v1 (schema_version: 1). Alternatives rejetées : Pydantic vision (viole #2), dataclasses vision (sérialisation manuelle JSON), reporter memory v1.1 (laisse ADR-066 pendant), `import_data` sans `export` (asymétrique).

[Détail → docs/adr/ADR-111-sdk-vision-typage-memory-io.md](adr/ADR-111-sdk-vision-typage-memory-io.md)

## ADR-112 — Suppression `LlmProxy.stream()` legacy, renommage `stream_complete` → `stream`

**Date :** 2026-05-19 — **Statut :** Accepté

Trois nettoyages ciblés sur `ctx.llm` et la boucle ReAct. (1) **Suppression** de `LlmProxy.stream()` legacy (buffered, retourne `list[str]`, plus utilisé par aucun agent bundled, docstring `stubs/llm.py:101` recommande déjà `stream_complete`). (2) **Renommage** de `stream_complete()` → `stream()` (la version moderne async iterator devient le nom canonique, cohérent avec LangChain / OpenAI SDK). Surface finale `ctx.llm` : `complete()`, `stream()`, `embed()` — 3 méthodes nommées idéalement. (3) **Suppression** de l'auto-rewrite des actions shorthand dans la boucle ReAct (`{"action": "tool_name"}` ré-écrit silencieusement en `{"action": "tool_call", "tool": "tool_name", "args": {}}`). Le shorthand devient une `ActionParseError` claire, émise via `ctx.events.emit_action_parse_error(step, raw, fatal=True)` (ADR-105). Bugs de prompt mis à nu (l'auteur voit qu'il doit renforcer son prompt au lieu que le SDK masque). ~50 LOC totales supprimées. Cohérence frameworks modernes. Alternatives rejetées : garder les deux comme aliases (confusion pérennisée), conserver rewrite avec warning (ignoré par défaut), config `strict_action_parsing` (option = friction).

[Détail → docs/adr/ADR-112-sdk-stream-cleanup-rename.md](adr/ADR-112-sdk-stream-cleanup-rename.md)

## ADR-113 : Architecture multi-runner sidecar pour l'inférence LLM/STT

**Date :** 2026-05-25 — **Statut :** Proposé

Refactor architectural majeur post-launch v0.1.0. Le runtime `apollia-os` ne link plus `llama-cpp-2` directement, mais spawn un process enfant `apollia-runner-{cuda,rocm,vulkan,metal,cpu}` qui contient le binding GPU compilé. Communication daemon ↔ runner par HTTP/JSON sur loopback TCP. Détection GPU automatique au boot du daemon. Résout 3 douleurs : UX download confuse pour non-techs (1 installer par OS au lieu de 3-5), maintenance multipliée (1 rebuild par CVE au lieu de 5), impossibilité d'évoluer (ajouter Intel oneAPI ou Apple ANE sans casser l'install). Pattern validé en production par Ollama, LM Studio. Compromis acceptés : bundle +200 MB par installer, latence IPC +50-100 µs par appel (négligeable vs 100 ms+ d'inférence), 6-8 semaines d'engineering, complexité ops (2 process à monitorer). Alternatives rejetées : Multi-binary launcher (pas de crash isolation, code dead), Multi-installer status quo (UX cassée). Plan 6 phases sprint-by-sprint, rétrocompatibilité totale côté agents Python (SDK `ctx.llm.*` inchangé) et CLI users. Rollback strategy : revert merge, re-tag, re-publish (SDK n'aura pas bougé entre temps).

[Détail → docs/adr/ADR-113-multi-runner-sidecar-architecture.md](adr/ADR-113-multi-runner-sidecar-architecture.md)

## DEC-2026-05-20 — Optimisation tool descriptors pour LLM mid-market/petits

**Date :** 2026-05-20 — **Statut :** Accepté

Optimisation du SDK et du bridge pour que les LLM mid-market (Mistral Small, Haiku, Llama 70B) construisent un payload valide au premier appel de skill, sans dépendre du system prompt pour expliciter chaque structure. Bug observé en runtime sur chart-worker : un LLM tente d'appeler `chart.bar` avec `{type: "bar", options: {...}}` au lieu du format attendu `{series: [{name, data}], ...}` parce que `list[dict[str, Any]]` produit un schéma trop vague. Trois leviers actifs : (1) **`Annotated[T, "description"]`** propagé dans `input_schema.properties[param].description` (SDK introspecte au load time, cf. `sdk/apollia/_internal/inference.py`) ; (2) **`@skill(examples=[...])`** propagé au tool descriptor LLM-facing via le manifest (bridge Rust `crates/apollia-aip/src/bridge.rs`) — payloads-modèles minimaux mais réalistes ; (3) **TypedDict canoniques** pour remplacer `list[dict[str, Any]]` opaque — déclarés dans un `schemas.py` séparé sans `from __future__ import annotations` (PEP 563 casse `TypedDict.__required_keys__`). Fix SDK associé : unwrap `NotRequired[T]`/`Required[T]` (PEP 655) dans `_typeddict_schema`. Migration livrée : 25/25 skills des workers chart/pdf/xlsx/docx/archive + workers veille-ia (entity-extraction, synthesis). Aucun changement d'API publique (`Annotated` est stdlib, `examples=` optionnel, TypedDict est stdlib), backward compatible. Pas d'ADR formel : optimisation d'implémentation des contrats existants (ADR-099 — signature = schéma), pas une décision architecturale structurante.

Référence : commits `bed9e212`, `48f6cd83`, `ee88ad44`, `26e0a719`, `566b79a1` + release doc `docs/internal/release/AGENTKIT-REBUILD-2026-05-19.md` section "Post-rebuild — Optimisations LLM tool descriptors".

---

*Ce log est maintenu à jour à chaque décision architecturale significative.*
*Format inspiré de [Architecture Decision Records (ADR)](https://adr.github.io/) par Michael Nygard.*
