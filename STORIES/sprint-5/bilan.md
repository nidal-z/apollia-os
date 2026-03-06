# Sprint 5 — Bilan

**Sprint Goal :** Runtime operable sans modifier le code — `apollia-os start` demarre le runtime, `apollia-os run hello-agent "Bonjour"` execute un agent, `apollia-os status` affiche l'etat, `apollia-os stop` arrete proprement — **atteint (chaine complete implementee)** ✅
**Demo :** `cargo test --workspace` passe (289 tests) + `cargo clippy --workspace -- -D warnings` : zero warning + `cargo fmt --check` propre

---

## Stories livrees

| ID | Story | Crate | Taille estimee | Temps reel | Derive |
|---|---|---|---|---|---|
| STORY-033 | APIServer axum Unix socket + TCP | apollia-runtime | L (6h) | ~6h | 0 |
| STORY-034 | Routes REST tasks (POST/GET/DELETE) | apollia-runtime | M (3h) | ~3h | 0 |
| STORY-035 | Routes REST agents (POST/GET/DELETE) | apollia-runtime | M (3h) | ~3h | 0 |
| STORY-036 | SSE streaming pour taches | apollia-runtime | M (3h) | ~3h | 0 |
| STORY-037 | CLI commandes niveau 1 (start/stop/status/run) | apollia-cli | L (8h) | ~8h | 0 |
| STORY-038 | CLI commandes niveau 2 (agent/task/tools/memory/audit) | apollia-cli | L (6h) | ~6h | 0 |
| STORY-039 | Supervisor demarrage ordonne + watchdog | apollia-runtime | L (6h) | ~6h | 0 |
| STORY-040 | Graceful shutdown SIGTERM/drain 30s | apollia-runtime | M (3h) | ~3h | 0 |

**Total estime :** 38h / budget 32-40h — sprint dans les clous. Aucune story reportee.

---

## Ce qui a bien marche

- **hyper-util pour Unix socket (ADR-017) :** Le risque #1 (premiere utilisation d'axum, dual listener) a ete mitigue par une boucle accept manuelle via hyper-util pour le Unix socket, tandis que `axum::serve()` gere le TCP. Pattern asymetrique mais fonctionnel. Sera simplifie quand axum 0.8 supportera les Unix listeners nativement.
- **CLI bootstrap sans Supervisor (ADR-018) :** La strategie d'implementer `start` avec un bootstrap sequentiel inline avant STORY-039 a permis de debloquer le Sprint Goal rapidement. Le code a ensuite ete remplace proprement par le Supervisor.
- **AppState generique `<B: ExecutionBackend>` :** Le pattern generique introduit dans le Sprint 4 (ADR-015/016) s'est propage naturellement a l'APIServer et au Supervisor. Chaque composant est testable avec un mock backend, sans Python reel ni acteurs Tokio.
- **Pattern `TakeWhileInclusive` custom pour SSE :** Un combinateur de stream custom qui inclut l'evenement terminal (TaskCompleted/TaskCanceled) avant de fermer le flux SSE. Elegant et testable unitairement.
- **Supervisor demarrage sequentiel simple :** Boucle sequentielle avec `tokio::time::timeout` par acteur, rollback en cas d'echec APIServer, emission `AllReady` apres demarrage complet. `RestartPolicy` + `RestartTracker` (sliding window) pour le watchdog. Pattern simple et robuste.
- **ShutdownController generique avec drain :** `wait_for_shutdown_signal()` gere SIGTERM + SIGINT + double Ctrl+C (force exit). Drain via EventBus subscriber avec countdown des taches actives. Arret des acteurs en ordre inverse du demarrage.
- **RuntimeClient HTTP Unix socket custom :** Client leger sans `reqwest` — hyper + tokio::net::UnixStream directement. Evite une dependance lourde pour un usage simple (CLI -> APIServer).
- **`--json` systematique sur toutes les commandes CLI :** Pattern `noun-verb` (ADR-008) respecte. Toutes les commandes supportent `--json` pour l'usage machine. TTY auto-detecte pour le formatage humain.

---

## Ce qui a pose probleme

- **axum 0.7.9 path params `:id` vs `{id}` :** La syntaxe des path parameters a change entre axum 0.7 (`:id`) et 0.8 (`{id}`). Documentation parfois ambigue. Resolu rapidement mais source de confusion initiale.
- **Fichiers source longs dans apollia-runtime :** `shutdown.rs` (829 loc), `router.rs` (649 loc), `supervisor.rs` (623 loc), `server.rs` (544 loc), `routes_agents.rs` (530 loc) depassent la limite recommandee de 300 lignes. Les tests inline representent ~50% du fichier. Acceptable pour le MVP mais a surveiller.
- **`main.rs` de apollia-cli trop long (539 loc) :** Toutes les sous-commandes clap sont definies dans un seul fichier. Refactor possible en extrayant les definitions clap dans des modules separes.
- **DT-025 (test e2e complet) toujours ouverte :** La chaine complete TaskRouter -> Coordinator -> ORIA -> AIPBridge -> Python n'est pas exercee par un test d'integration end-to-end. Reporte au Sprint 6 (STORY-045).

---

## Stories reportees

Aucune.

---

## Decisions architecturales prises

| ADR | Decision | Story |
|---|---|---|
| ADR-017 | hyper-util explicite pour Unix socket serving (boucle accept manuelle) | STORY-033 |
| ADR-018 | CLI bootstrap sans Supervisor — demarrage sequentiel inline dans `start` | STORY-037 |

**Decisions non-ADR (mineures) :**
- `AppState<B>` avec Clone manuel (pas de derive) car `B: ExecutionBackend` n'est pas necessairement Clone — meme pattern que `TaskRouterHandle<B>`
- `APIServerHandle` utilise `watch::channel` pour le shutdown signal — plus simple que `broadcast` pour un signal one-shot
- `SseTaskEvent` struct intermediaire pour le mapping RuntimeEvent -> SSE event — filtre par task_id cote serveur
- `json_to_aip_input()` wraps `serde_json::Value` en `DataPart` pour la compatibilite AIPInput
- `RuntimeClient` sans `reqwest` — hyper + UnixStream directement pour eviter une dep lourde
- `exit_codes.rs` avec constantes POSIX (0=OK, 1=ERROR, 2=USAGE, 69=UNAVAILABLE, 70=INTERNAL)
- `POST /api/v1/shutdown` endpoint ajoute a l'APIServer — emet `ShutdownRequested` via EventBus
- `manifest_from_path()` MVP sans chargement Python dans routes_agents — simplifie le start agent
- `TaskRouterHandle::active_tasks()` ajoute pour le drain du ShutdownController
- `Supervisor::default_child_specs()` definit 6 acteurs avec `RestartPolicy` individuel

---

## Dette technique identifiee

| # | Dette | Severite | Sprint cible |
|---|---|---|---|
| DT-028 | `shutdown.rs` (829 loc) dans apollia-runtime depasse 300 lignes — tests inline ~50% | Moyenne | Refactor si le module grossit |
| DT-029 | `supervisor.rs` (623 loc), `server.rs` (544 loc), `routes_agents.rs` (530 loc) dans apollia-runtime depassent 300 lignes | Faible | Refactor si les modules grossissent |
| DT-030 | `main.rs` (539 loc) dans apollia-cli — definitions clap + dispatch dans un seul fichier | Moyenne | Sprint 6 (extraire les definitions dans des modules) |
| DT-031 | `manifest_from_path()` MVP dans routes_agents — ne charge pas le module Python reel, retourne un manifest placeholder | Elevee | Sprint 6 (integrer AIPLoader dans le start agent) |
| DT-032 | CLI `start` bootstrap inline coexiste avec Supervisor — code potentiellement duplique | Faible | Sprint 6 (verifier que `start` utilise le Supervisor) |
| DT-033 | Pas de tests d'integration HTTP reels (API routes testees avec mocks, pas de server spawne) | Moyenne | Sprint 6 (STORY-045 e2e) |
| DT-034 | Unix socket path `/tmp/apollia.sock` hardcode dans client.rs et server.rs — devrait etre configurable | Faible | v0.2 (config file) |

**Dettes Sprints precedents toujours ouvertes :** DT-006 (AgentId String alias), DT-007 (AgentStopping event), DT-008 (dead_code allows dans registry.rs), DT-009 (AgentRegistry::spawn pub), DT-010 (cgroups CPU/RAM), DT-011 (mount namespace tmpfs), DT-012 (dangerous_tools granularite), DT-013 (outils natifs non auto-enregistres), DT-014 (AuditTrail sync/async hybride), DT-015 (fichiers longs semantic.rs/search.rs), DT-016 (purge automatique TTL), DT-017 (MemoryManager pas acteur Tokio — confirmee par DT-023), DT-018 (limite entries par namespace), DT-019 (MemoryStore dry-run), DT-020 (memory.rs/context.rs longs dans apollia-aip), DT-021 (router.rs long dans apollia-runtime — confirmee, 649 loc), DT-022 (clippy useless_conversion PyO3), DT-023 (Arc<Mutex<MemoryManager>>), DT-024 (PYO3_PYTHON macOS), DT-025 (test e2e complet — reporte Sprint 6), DT-026 (cargo fmt discipline), DT-027 (ExecutionBackend vs AgentRunner unification).

---

## Metriques

| Metrique | Valeur |
|---|---|
| Tests apollia-aip | 32 |
| Tests apollia-oria | 16 |
| Tests apollia-runtime (unit + integration) | 69 + 4 = 73 |
| Tests apollia-memory | 56 |
| Tests apollia-tools | 55 |
| Tests apollia-core | 22 |
| Tests apollia-cli | 35 |
| **Tests workspace total** | **289** |
| Lignes de code (apollia-runtime/src) | ~5 010 |
| Lignes de code (apollia-cli/src) | ~2 432 |
| ADR crees | 2 (ADR-017, ADR-018) |
| Clippy warnings | 0 |
| Stories livrees / planifiees | 8/8 (100%) |
| Delta tests vs Sprint 4 | +72 (217 -> 289) |

---

## Focus Sprint 6

**Sprint Goal cible :** Demo client reelle — agent devis-generator operationnel, tout local, zero cloud.

Stories a implementer :
1. STORY-041 — ResilienceLayer circuit breaker par outil (L)
2. STORY-042 — Retry policy avec backoff exponentiel + jitter (M)
3. STORY-043 — ORIA Mode Orchestre + Reasoner LLM (XL)
4. STORY-044 — Agent devis-generator complet (L)
5. STORY-045 — Tests d'integration end-to-end (L)
6. STORY-046 — README + documentation installation (M)

**Risques principaux :**
- STORY-043 (ORIA Orchestre) est XL et necessite un appel LLM reel — premiere utilisation d'un LLM dans le runtime
- STORY-044 (agent devis-generator) est le premier agent Python reel exercant toute la chaine
- DT-031 (manifest_from_path MVP) doit etre resolu avant STORY-044
- DT-025 (test e2e) couvert par STORY-045
