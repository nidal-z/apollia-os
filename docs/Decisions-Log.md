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

**Conséquences :** Abandon de 8 mois de code SaaS. Conservation de la valeur architecturale. Nouveau modèle économique (freelance vs. SaaS).

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

*Ce log est maintenu à jour à chaque décision architecturale significative.*
*Format inspiré de [Architecture Decision Records (ADR)](https://adr.github.io/) par Michael Nygard.*
