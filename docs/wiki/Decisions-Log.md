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

## ADR-005 — Sandbox sans Docker (Linux namespaces)

**Date :** 2026-03
**Statut :** Accepté

**Contexte :** Docker est une dépendance lourde interdite par le principe #2. Linux namespaces fournissent l'isolation nécessaire sans Docker.

**Décision :** MVP avec `subprocess` + `unshare(1)` (PID + mount namespaces). Roadmap v0.2 → `nsjail`. Roadmap v1.0 → gVisor optionnel.

**Alternatives considérées :**
- Docker obligatoire : Viole le principe #2, bloque les déploiements sans Docker daemon.
- Firecracker microVM : Complexité excessive pour un MVP.
- WebAssembly : Écosystème Python Wasm immature.
- Rien : Inacceptable pour un runtime de production.

**Conséquences :** User namespaces doivent être activés sur l'OS hôte (standard sur Linux moderne).

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

**Conséquences :** Les commandes de niveau 1 (`start`, `stop`, `status`) sont des exceptions justifiées par leur fréquence.

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

## ADR-012 — Mode DevSandbox sur macOS : pas de sandbox réel en développement

**Date :** 2026-03-06
**Statut :** Accepté

**Contexte :** `unshare(1)` n'existe pas sur macOS. `sandbox-exec` (Seatbelt/SBPL), l'alternative native macOS, est deprecated depuis macOS 10.15 et basé sur une API privée non documentée. Docker viole le Principe #2.

**Décision :** Deux modes compilés via `#[cfg(target_os = "linux")]` : `SandboxMode::LinuxNamespaces` en production (Linux), `SandboxMode::Dev` sur macOS avec `tracing::warn!` à chaque invocation. La CI tourne sur Linux et valide le chemin sandbox réel.

**Alternatives considérées :**
- `sandbox-exec` macOS : API deprecated depuis macOS 10.15, syntaxe SBPL propriétaire, retrait possible sans préavis. Rejetée — dette technique garantie.
- Docker en dev : Viole Principe #2, commercial pour orgs > 250 personnes. Rejeté.
- Warning uniquement au démarrage : Trop discret — un dev peut oublier l'absence de sandbox. Rejeté.

**Conséquences :** Pas d'isolation réelle sur macOS dev (acceptable : code de confiance du développeur). Zero dépendance ajoutée. Parity prod garantie par CI Linux. Le warning par invocation rend l'absence de sandbox impossible à ignorer.

[Détail → docs/adr/ADR-012-sandbox-devmode-macos.md](adr/ADR-012-sandbox-devmode-macos.md)

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

**Contexte :** STORY-037 (CLI niveau 1) depend de STORY-039 (Supervisor) non encore implementee. La commande `start` doit demarrer le runtime en foreground.

**Decision :** Bootstrap sequentiel inline dans la commande `start` (EventBus -> AgentRegistry -> TaskRouter -> APIServer). Endpoint `POST /api/v1/shutdown` emet `RuntimeEvent::ShutdownRequested` via EventBus. Sera remplace par le Supervisor (STORY-039).

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

## ADR-020 — apollia-llm : moteur d'inférence embarqué, modèles fichiers externes, feature flags

**Date :** 2026-03-08
**Statut :** Accepté

**Contexte :** Sprint 8 introduit `ctx.llm` pour les agents Python. Trois contraintes encadrent le choix : inférence locale offline (Principe #1), zéro daemon tiers requis (Principe #2), et fail fast si le modèle est absent (Principe #4). Certains utilisateurs préfèrent les backends cloud (Anthropic, OpenAI) — la solution doit couvrir les deux cas sans imposer la compilation du moteur d'inférence à tous.

**Décision :** Crate `apollia-llm` avec deux feature flags Cargo : `cloud` (défaut, clients HTTP purs via `async-openai` + `reqwest`) et `local` (compile `EmbeddedBackend` via `mistral-rs-core` in-process). Le modèle `.gguf` est toujours un fichier externe dans `~/.apollia/models/` — jamais dans le binaire. `LlmRouter` dispatche au runtime selon `apollia.toml`. Backend absent → warning, pas de crash. Aucun backend disponible → `ctx.llm = None`, agent en `DEGRADED`.

**Alternatives considérées :**
- Daemon externe géré par Supervisor (rejetée : viole Principe #2 — suppose llama.cpp/ollama installé, gestion PID complexe, pas de single-binary réel)
- Modèle GGUF embarqué dans le binaire (rejetée : ~2 Go inutilisables, impossible de changer de modèle sans recompiler)

**Conséquences :**
- `feature = "local"` : inférence offline complète, binaire plus lourd (mistral-rs-core).
- `feature = "cloud"` (défaut) : binaire léger, aucun moteur compilé.
- `LlmCallCompleted` émis sur EventBus après chaque appel (tokens, latence, coût estimatif).
- `run_tools()` intègre `StepBudget` — garde-fou Principe #7 respecté dans la boucle ReAct.
- `mistral-rs-core 0.4` à surveiller pour breaking changes.

**Principes impactés :** Principe #1 — Local-first, Principe #2 — Zéro dépendance opérationnelle, Principe #4 — Fail fast, Principe #7 — Garde-fous non-négociables

[Détail complet → docs/adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md](adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md)

---

## ADR-021 — apollia-triggers : configuration TOML-only, authentification HMAC-SHA256 webhooks, hot reload sans restart

**Date :** 2026-03-08
**Statut :** Accepté

**Contexte :** Sprint 9 introduit `apollia-triggers` (cron, interval, file watch, webhooks). Trois décisions structurantes engagent des interfaces difficiles à inverser : (1) format de configuration des triggers, (2) authentification des webhooks entrants, (3) mise à jour des triggers sans redémarrer le runtime.

**Décision :** (1) Configuration TOML-only via `[[triggers]]` dans `apollia.toml` — même source de vérité que les LLM backends. Validation sémantique complète dans `ApolliaConfig::load()` au démarrage (schedule cron via `cron::Schedule::from_str`, secret webhook non vide, path résolvable). Trigger `enabled = false` → source non validée. (2) HMAC-SHA256 avec header `X-Apollia-Signature: sha256=<hex>` (standard GitHub Webhooks) + `constant_time_eq` pour la comparaison — le body est lié cryptographiquement au secret, timing attacks éliminées. Ordre de réponse : 503 → 404 → 401 → 200. (3) Hot reload via `POST /api/v1/triggers/reload` + `apollia-os trigger reload` : timeout 2s sur chaque `JoinHandle<()>` actif avant drop forcé, full-replace des définitions, compteurs SQLite préservés, `TriggersReloaded { count }` sur EventBus. Erreur TOML au reload → 422, triggers actuels inchangés.

**Alternatives considérées :**
- API REST + stockage SQLite (rejetée : double source de vérité TOML/base, pas de fail fast naturel au démarrage)
- Fichier `triggers.toml` séparé avec auto-reload inotify (rejetée : fragmentation config, reload implicite = comportement surprenant, viole ADR-008)
- Token Bearer statique pour les webhooks (rejetée : n'authentifie pas le body — rejoue possible, pas de protection timing attacks)
- Hot reload via SIGHUP (rejetée : pas de retour sur erreur, incompatible Windows roadmap, rompt le pattern REST `POST /api/v1/shutdown`)

**Conséquences :**
- `apollia.toml` est la source de vérité unique pour l'ensemble du runtime (runtime, LLM, agents, triggers).
- Full-replace au reload : sources inchangées stoppées et respawnées (impact minimal en pratique).
- Trois nouvelles dépendances workspace : `cron = "0.12"`, `notify = "6"`, `chrono = "0.4"`.
- Risque compatibilité `hmac 0.12` + `sha2 0.10` : vérifier `digest` commun avant STORY-069.

**Principes impactés :** Principe #1 — Local-first, Principe #4 — Fail fast, Principe #5 — Un acteur une responsabilité, Principe #8 — CLI humaine

[Détail complet → docs/adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md](adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md)

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
**Statut :** Accepté

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

## ADR-033 — Config opérateur SQLite : séparation structurel (TOML) / opérationnel (SQLite)

**Date :** 2026-03-20
**Statut :** Accepté

**Contexte :** `apollia.toml` mélange config structurelle (ports, chemins, LLM) et config opérationnelle (triggers, pipelines, notifications). Un non-développeur ne peut pas configurer ces éléments sans éditer du TOML. Le hot-reload TOML est fragile et sans validation interactive.

**Décision :** Triggers, pipelines et notifications migrent de `apollia.toml` vers SQLite (une DB par sous-système). Le TOML ne contient plus que la config structurelle. Le pattern de modification est : API handler → SQLite write → Handle.reload() synchrone. L'app desktop devient read-write pour la config opérationnelle.

**Alternatives considérées :** TOML reste source de vérité avec hot-reload amélioré (rejetée — ne résout pas le problème opérateur), EventBus pour notifier les acteurs (rejetée — complexité sans feedback synchrone), Watch file SQLite (rejetée — fragile avec WAL).

**Conséquences :** CRUD depuis l'API REST et l'app desktop avec validation interactive (422). ADR-029 (Settings lecture seule) reste valide pour le TOML structurel. ADR-021 (triggers TOML-only) partiellement remplacé. `Arc<Mutex<>>` pour les repositories dans AppState (rusqlite Connection non-Sync, mutations rares).

**Principes impactés :** Principe #1 (Local-first) renforcé, Principe #4 (Fail fast) renforcé, Principe #8 (CLI humaine, API machine) renforcé.

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

## ADR-042 — Remplacement de mistral.rs par llama.cpp (lié statiquement) comme moteur d'inférence GGUF

**Date :** 2026-03-26
**Statut :** Accepté

**Contexte :** mistralrs v0.7 ne supporte que 16 architectures GGUF et ses kernels candle Metal crashent sur les modèles MoE (`indexed_moe_forward not implemented`). Qwen3.5, GLM-4.7, Llama 4 sont inaccessibles. Le streaming est un fallback single-chunk.

**Décision :** Remplacement de `mistralrs` + `mistralrs-core` par `llama-cpp-2` (bindings safe Rust pour llama.cpp, compilation statique). Changement contenu dans `apollia-llm::backends::embedded`. Le trait `CompletionModel`, le `LlmRouter`, le config TOML et l'API publique ne changent pas.

**Alternatives considérées :** Attendre mistral.rs 0.8+ (rejetée — délai inconnu, pas de garantie Metal MoE), llama-server processus externe (rejetée — viole Principe #2), Contribuer les kernels Metal à candle (rejetée — effort 3-6 mois disproportionné).

**Conséquences :**
- 30+ architectures GGUF supportées immédiatement (Qwen3.5, GLM-4.7, Llama 4...)
- Metal MoE natif fonctionnel, streaming token-by-token, taille binaire réduite
- Build chain C++ (cmake) déjà présente via ADR-041 whisper.cpp
- Surveiller conflit symboles ggml entre whisper.cpp et llama.cpp

**Principes impactés :** Principe #1 — Local-first (renforcé), Principe #2 — Zéro dépendance (respecté, statique), Principe #4 — Fail fast (respecté)

[Détail → docs/adr/ADR-042-remplacement-mistralrs-par-llamacpp-statique.md](adr/ADR-042-remplacement-mistralrs-par-llamacpp-statique.md)

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

*Ce log est maintenu à jour à chaque décision architecturale significative.*
*Format inspiré de [Architecture Decision Records (ADR)](https://adr.github.io/) par Michael Nygard.*
