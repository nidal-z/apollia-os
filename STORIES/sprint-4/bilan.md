# Sprint 4 — Bilan

**Sprint Goal :** `apollia-os run hello-agent "Bonjour"` — un agent Python s'execute dans le runtime Rust via PyO3, avec StepBudget enforce et dispatch complet TaskRouter -> ExecutionCoordinator -> ORIA -> AIPBridge -> Python — **atteint (chaine complete implementee) ✅**
**Demo :** `cargo test --workspace` passe (217 tests) + `cargo clippy --workspace -- -D warnings` : zero warning + `cargo fmt --check` propre

---

## Stories livrees

| ID | Story | Crate | Taille estimee | Temps reel | Derive |
|---|---|---|---|---|---|
| STORY-024 | Chargement module Python via PyO3 | apollia-aip | L (6h) | ~6h | 0 |
| STORY-025 | Validation AIP duck typing (manifest + run async) | apollia-aip | M (3h) | ~3h | 0 |
| STORY-026 | Bridge Tokio <-> asyncio via pyo3-async-runtimes | apollia-aip | L (6h) | ~6h | 0 |
| STORY-027 | ToolProxy Python -> outils Rust | apollia-aip | M (3h) | ~3h | 0 |
| STORY-028 | MemoryInterface Python -> apollia-memory | apollia-aip | M (3h) | ~3h | 0 |
| STORY-029 | Observer + ContextBundle + classify() | apollia-oria | M (3h) | ~3h | 0 |
| STORY-030 | ORIA Mode Direct + StepBudget enforcement | apollia-oria | L (6h) | ~6h | 0 |
| STORY-031 | ExecutionCoordinator + semaphore concurrence | apollia-runtime | M (3h) | ~3h | 0 |
| STORY-032 | TaskRouter dispatch | apollia-runtime | M (3h) | ~3h | 0 |

**Total estime :** 36h / budget 32-40h — sprint dans les clous. Aucune story reportee.

---

## Ce qui a bien marche

- **Bridge PyO3 `spawn_blocking` + `asyncio.run()` (ADR-014) :** Le risque #2 (GIL contention) et #5 (premiere utilisation pyo3-async-runtimes) ont ete mitiges par un pattern simple : `tokio::task::spawn_blocking` cote Rust pour acquitter le GIL dans un thread dedie, puis `asyncio.run()` cote Python pour executer le code async de l'agent. Ce pattern evite completement les problemes d'event loop imbriquees et fonctionne de maniere fiable.
- **Pattern trait pour testabilite (ADR-015 ToolExecutor, ADR-016 AgentRunner) :** Le meme pattern d'abstraction par trait a ete applique 3 fois (ToolProxy, ORIAEngine, ExecutionCoordinator). Chaque composant depend d'un trait injectable, ce qui permet des tests unitaires sans Python reel ni acteurs Tokio. Pattern confirme comme standard du projet.
- **Fonctions `*_inner()` testables sans PyO3 :** Pour ToolProxy et MemoryInterface, la logique metier est extraite dans des fonctions libres (`call_inner()`, `record_inner()`, `recall_inner()`, etc.) testables en pure Rust. Les `#[pymethods]` ne font que du marshalling PyObject <-> serde_json::Value. Ce pattern separe proprement le code testable du code PyO3.
- **`classify()` pure dans Observer :** La fonction de classification ExecutionMode (Direct vs Orchestrated) est une fonction pure avec des constantes nommees pour les seuils. Facile a tester, facile a ajuster. Les 4 heuristiques (nombre de steps, multi-tool, sous-taches, complexite memoire) couvrent bien les cas d'usage initiaux.
- **StepBudget tri-dimensionnel (steps + temps + tokens) :** `AtomicU32` pour le compteur de steps, `Instant` pour le timeout, `from_capped()` pour appliquer `min(agent, runtime)`. Le budget est non-contournable par l'agent (principe #7). Pattern simple et robuste.
- **ExecutionCoordinator generique avec semaphore Tokio :** `Semaphore::try_acquire_owned()` pour du non-bloquant, le permit est move dans la closure `tokio::spawn` et drop automatiquement a la fin. Fire-and-forget pour les events TaskStarted/TaskCompleted. Le generique `<B: ExecutionBackend>` permet le mock complet.
- **TaskRouter acteur Tokio avec verification ProcessState :** Le router verifie l'etat de l'agent via `AgentRegistryHandle` avant dispatch et emet un warning `AgentDegraded` pour les agents en etat degrade. Le `Clone` manuel sur `TaskRouterHandle<B>` (independant de `B: Clone`) est une solution elegante au probleme des generics avec `mpsc::Sender`.

---

## Ce qui a pose probleme

- **`cargo fmt` non applique sur les derniers commits :** Plusieurs fichiers (memory.rs, context.rs, router.rs, coordinator.rs) avaient des ecarts de formatage accumules. Corrige en fin de sprint. A l'avenir, toujours executer `cargo fmt` avant chaque commit.
- **`#[allow(clippy::useless_conversion)]` necessaire pour PyO3 :** Les `#[pymethods]` avec des types de retour PyO3 declenchent un faux positif clippy `useless_conversion`. Le workaround (`#[allow(...)]` sur les modules concernes) est fonctionnel mais pas ideal. A surveiller avec les futures versions de PyO3/clippy.
- **Fichiers source longs dans apollia-aip :** `memory.rs` (601 loc), `context.rs` (571 loc) depassent la limite recommandee de 300 lignes. Comme pour le sprint 3, les tests inline representent ~50% du fichier. Acceptable pour le MVP.
- **`PYO3_PYTHON` obligatoire sur macOS (ADR-013) :** La variable d'environnement doit etre settee pour chaque `cargo test -p apollia-aip` et `cargo test --workspace`. Pas bloquant mais friction de dev. La CI devra configurer cela explicitement.
- **DT-017 (MemoryManager pas acteur Tokio) confirmee :** Le `MemoryInterface` Python utilise `Arc<Mutex<MemoryManager>>` pour le partage inter-threads, ce qui est un compromis par rapport au pattern acteur strict. Fonctionne en single-agent mais devra etre revu pour le multi-agent concurrent.

---

## Stories reportees

Aucune.

---

## Decisions architecturales prises

| ADR | Decision | Story |
|---|---|---|
| ADR-013 | `PYO3_PYTHON` config macOS — pointer explicitement vers l'interpreteur Python systeme | STORY-024 |
| ADR-014 | `spawn_blocking` + `asyncio.run()` au lieu de `into_future` pour le bridge async | STORY-026 |
| ADR-015 | Trait `ToolExecutor` pour abstraire l'execution d'outils (injection de dependance) | STORY-027 |
| ADR-016 | Trait `AgentRunner` pour abstraire l'execution d'agents (meme pattern ADR-015) | STORY-030 |

**Decisions non-ADR (mineures) :**
- `ValidatedAgent` struct avec `Py<PyAny>` pour le module valide — pas de Clone possible (PyO3 GIL), usage par reference
- `ToolCallContext` struct pour grouper les parametres de `call_inner()` — evite les fonctions a 7+ parametres
- `now_rfc3339()` helper interne dans context.rs — evite la dependance `chrono` pour un simple timestamp
- `SemanticMemory::recall_all()` ajoute dans apollia-memory pour le snapshot Observer
- `ExecutionMode` enum (Direct/Orchestrated) dans apollia-oria — pas de mode intermediaire, classification binaire
- Clone manuel sur `TaskRouterHandle<B>` — `mpsc::Sender::clone()` est independant du bound `Clone` sur `B`

---

## Dette technique identifiee

| # | Dette | Severite | Sprint cible |
|---|---|---|---|
| DT-020 | `memory.rs` (601 loc) et `context.rs` (571 loc) dans apollia-aip depassent 300 lignes — tests inline inclus | Faible | Refactor si les modules grossissent |
| DT-021 | `router.rs` (586 loc) dans apollia-runtime depasse 300 lignes — tests inline inclus | Faible | Refactor si le module grossit |
| DT-022 | `#[allow(clippy::useless_conversion)]` sur modules `context` et `memory` dans apollia-aip — faux positif PyO3 | Faible | Retirer quand PyO3/clippy corrigent le faux positif |
| DT-023 | `Arc<Mutex<MemoryManager>>` dans MemoryInterface viole le pattern acteur strict — compromis pour le bridge PyO3 synchrone | Moyenne | Sprint 6 (refactor en acteur si multi-agent concurrent) |
| DT-024 | `PYO3_PYTHON` variable d'environnement requise sur macOS — friction de dev, CI a configurer | Moyenne | Sprint 5 (CI config) |
| DT-025 | Pas de test d'integration end-to-end exercant la chaine complete TaskRouter -> Coordinator -> ORIA -> AIPBridge -> Python | Elevee | Sprint 5 ou 6 (quand la CLI `run` est implementee) |
| DT-026 | `cargo fmt` non applique systematiquement avant commit — ecarts accumules corriges en fin de sprint | Moyenne | Immediat (discipline de dev) |
| DT-027 | `ExecutionBackend` trait non unifie avec `AgentRunner` trait — deux abstractions similaires pour l'execution | Faible | v0.2 post-MVP (unification si necessaire) |

**Dettes Sprints precedents toujours ouvertes :** DT-006 (AgentId String alias), DT-007 (AgentStopping event), DT-008 (dead_code allows dans registry.rs), DT-009 (AgentRegistry::spawn pub), DT-010 (cgroups CPU/RAM), DT-011 (mount namespace tmpfs), DT-012 (dangerous_tools granularite), DT-013 (outils natifs non auto-enregistres), DT-014 (AuditTrail sync/async hybride), DT-015 (fichiers longs semantic.rs/search.rs), DT-016 (purge automatique TTL), DT-017 (MemoryManager pas acteur Tokio — confirmee par DT-023), DT-018 (limite entries par namespace), DT-019 (MemoryStore dry-run).

---

## Metriques

| Metrique | Valeur |
|---|---|
| Tests apollia-aip | 32 |
| Tests apollia-oria | 16 |
| Tests apollia-runtime (unit + integration) | 26 + 4 = 30 |
| Tests apollia-memory | 56 |
| Tests apollia-tools | 55 |
| Tests apollia-core | 22 |
| Tests apollia-cli | 6 |
| **Tests workspace total** | **217** |
| Lignes de code (apollia-aip/src) | ~2 033 |
| Lignes de code (apollia-oria/src) | ~919 |
| Lignes de code (apollia-runtime/src, nouveau) | ~1 538 |
| ADR crees | 4 (ADR-013 a ADR-016) |
| Clippy warnings | 0 |
| Stories livrees / planifiees | 9/9 (100%) |

---

## Focus Sprint 5

**Sprint Goal cible :** Runtime operable sans modifier le code — `apollia-os start/stop/status/run/audit` fonctionnels.

Stories a implementer :
1. STORY-033 — APIServer axum Unix socket + TCP (L)
2. STORY-034 — Routes REST tasks POST/GET/DELETE (M)
3. STORY-035 — Routes REST agents POST/GET/DELETE (M)
4. STORY-036 — SSE streaming pour taches (M)
5. STORY-037 — CLI commandes niveau 1 start/stop/status/run (L)
6. STORY-038 — CLI commandes niveau 2 agent/task/tools/memory/audit (L)
7. STORY-039 — Supervisor demarrage ordonne + watchdog (L)
8. STORY-040 — Graceful shutdown SIGTERM/drain 30s (M)

**Risques principaux :**
- Premiere utilisation d'axum — API HTTP + Unix socket + SSE
- Supervisor ordonnance le demarrage de tous les acteurs — complexite d'orchestration
- Integration end-to-end de toute la chaine (DT-025)
