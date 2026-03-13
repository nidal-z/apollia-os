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

*Ce log est maintenu à jour à chaque décision architecturale significative.*
*Format inspiré de [Architecture Decision Records (ADR)](https://adr.github.io/) par Michael Nygard.*
