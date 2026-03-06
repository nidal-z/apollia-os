# Sprint 3 — Bilan

**Sprint Goal :** `memory.search("devis Dupont")` retourne 3 resultats classes BM25 depuis SQLite FTS5 avec tokenizer `unicode61` — **atteint ✅**
**Demo :** `cargo test -p apollia-memory` passe (56 tests) + `cargo test --workspace` propre (156 tests total)

---

## Stories livrees

| ID | Story | Taille estimee | Temps reel | Derive |
|---|---|---|---|---|
| STORY-017 | Schema SQLite complet + migrations versionnees | M (3h) | ~3h | 0 |
| STORY-018 | EpisodicMemory backend (record/history/TTL) | M (3h) | ~3h | 0 |
| STORY-019 | SemanticMemory backend (remember/recall/forget) | M (3h) | ~3h | 0 |
| STORY-020 | FTS5 search avec tokenizer unicode61 + BM25 | M (3h) | ~3h | 0 |
| STORY-021 | MemoryManager namespace isolation | M (3h) | ~3h | 0 |
| STORY-022 | ProceduralMemory backend | S (2h) | ~2h | 0 |
| STORY-023 | CLI `apollia-os memory inspect` preview | S (2h) | ~2h | 0 |

**Total estime :** 19h / budget 18-24h — sprint dans les clous. Aucune story reportee.

---

## Ce qui a bien marche

- **Pattern MemoryStore partage entre backends :** Le choix d'un `MemoryStore` centralise (ouverture unique de la connexion SQLite + WAL + schema) reutilise par les 3 backends (episodic, semantic, procedural) a evite toute duplication et simplifie l'architecture. Chaque backend recoit un `&MemoryStore` et se concentre sur sa logique metier.
- **FTS5 bundled confirme sans friction :** Le risque #1 du plan (FTS5 non disponible dans rusqlite bundled) ne s'est pas materialise. `rusqlite` avec `features = ["bundled"]` inclut FTS5 et le tokenizer `unicode61` par defaut. Le test d'initialisation dans STORY-017 a valide cela des le debut du sprint.
- **BM25 ranking fonctionnel immediatement :** La fonction `bm25()` de FTS5 a fonctionne sans configuration particuliere. Le classement par pertinence est correct pour le cas d'usage cible (recherche textuelle PME francaise).
- **MemoryManager avec lazy store opening :** Ouverture des stores SQLite a la demande (pas au demarrage) + cache `HashMap<namespace, MemoryStore>`. Pattern simple et efficace, zero allocation inutile.
- **Namespace isolation propre :** Chaque agent a sa propre base SQLite (`{data_dir}/{namespace}.db`). L'isolation est physique (fichier separe), pas logique (filtre SQL). Cross-namespace en lecture seule via `shared_memory_namespaces` du manifest.
- **ProceduralMemory avec compteur d'usage :** `use_count` incremente a chaque `recall()`, permettant au runtime de privilegier les workflows les plus utilises. Pattern simple qui ajoute de la valeur sans complexite.
- **56 tests dans apollia-memory :** Couverture exhaustive des 3 backends + search + manager + store. Tous les AC positifs et negatifs couverts.
- **STORY-023 CLI preview livree proprement :** `MemoryStats` expose dans `store.rs` avec `MemoryStore::stats()` pub, reutilisable par d'autres consommateurs. La CLI lit directement SQLite sans besoin du runtime.

---

## Ce qui a pose probleme

- **Aucun probleme majeur sur ce sprint.** Les risques identifies dans le plan etaient bien calibres et aucun ne s'est materialise en bloqueur.
- **Fichiers source un peu longs :** `semantic.rs` (545 lignes) et `search.rs` (477 lignes) depassent la limite recommandee de 300 lignes. Cela inclut les tests inline (~50% du fichier). Acceptable pour le MVP mais a surveiller si ces modules grossissent.
- **Pas de test d'integration cross-crate :** Contrairement au Sprint 1 (EventBus + Registry) et Sprint 2 (ToolResolver), le Sprint 3 n'a pas de test d'integration exercant le Memory Engine depuis l'exterieur d'`apollia-memory`. Le test viendra naturellement avec le bridge PyO3 (Sprint 4).

---

## Stories reportees

Aucune.

---

## Decisions architecturales prises

Aucun ADR cree pendant ce sprint. Les choix architecturaux etaient alignes avec les decisions existantes :
- Pattern SQLite + WAL reutilise de STORY-016 (AuditTrail)
- FTS5 avec `unicode61` comme prevu dans la spec Memory Engine
- Isolation physique par namespace (un fichier SQLite par agent) — choix naturel du principe #1 (local-first)

**Decisions non-ADR (mineures) :**
- `MemoryStore::stats()` expose comme API publique pour la CLI — pas de trait intermediaire, acces direct SQLite
- `MemoryManager` utilise `HashMap<String, MemoryStore>` en cache interne — pas d'acteur Tokio pour le manager lui-meme (les stores sont synchrones via rusqlite)
- `AccessLevel::ReadWrite` / `ReadOnly` pour le controle cross-namespace — enum simple, pas de systeme de permissions complexe

---

## Dette technique identifiee

| # | Dette | Severite | Sprint cible |
|---|---|---|---|
| DT-015 | `semantic.rs` (545 loc) et `search.rs` (477 loc) depassent 300 lignes — les tests inline gonflent les fichiers | Faible | Refactor si les modules grossissent |
| DT-016 | Pas de purge automatique des entries expirees (episodic TTL, semantic TTL) — purge manuelle uniquement via `purge_expired()` | Moyenne | Sprint 5 (Supervisor periodic task) |
| DT-017 | MemoryManager n'est pas un acteur Tokio — acces synchrone via rusqlite. Pas de probleme tant que les appels viennent d'un seul thread, mais incompatible avec le pattern acteur strict si plusieurs agents accedent en parallele | Moyenne | Sprint 4 (quand le bridge PyO3 integre la memoire) |
| DT-018 | Pas de limite sur le nombre d'entries par namespace — un agent peut remplir le disque sans garde-fou | Faible | Sprint 6 (hardening) |
| DT-019 | `MemoryStore::open()` cree le fichier et le schema immediatement — pas de mode "dry-run" ou verification avant creation | Faible | v0.2 post-MVP |

**Dettes Sprints precedents toujours ouvertes :** DT-006 (AgentId String alias), DT-007 (AgentStopping event), DT-008 (dead_code allows dans registry.rs — 3 occurrences confirmees), DT-009 (AgentRegistry::spawn pub), DT-010 (cgroups CPU/RAM), DT-011 (mount namespace tmpfs), DT-012 (dangerous_tools granularite), DT-013 (outils natifs non auto-enregistres), DT-014 (AuditTrail sync/async hybride).

---

## Metriques

| Metrique | Valeur |
|---|---|
| Tests apollia-memory | 56 |
| Tests workspace total | 156 |
| Lignes de code (apollia-memory/src) | ~2 656 |
| Lignes de code (CLI memory command) | 233 |
| Clippy warnings | 0 |
| Stories livrees / planifiees | 7/7 (100%) |

---

## Focus Sprint 4

**Sprint Goal cible :** `apollia-os run hello-agent "Bonjour"` — un agent Python s'execute dans le runtime Rust.

Stories a implementer dans l'ordre :
1. STORY-024 — Chargement module Python via PyO3 (L)
2. STORY-025 — Validation AIP duck typing manifest + run async (M)
3. STORY-026 — Bridge Tokio <-> asyncio via pyo3-async-runtimes (L)
4. STORY-027 — ToolProxy Python -> outils Rust (M)
5. STORY-028 — MemoryInterface Python -> apollia-memory (M)
6. STORY-029 — Observer + ContextBundle + classify() (M)
7. STORY-030 — ORIA Mode Direct + StepBudget enforcement (L)
8. STORY-031 — ExecutionCoordinator + semaphore concurrence (M)
9. STORY-032 — TaskRouter dispatch (M)

**Note :** Sprint ambitieux (3L + 6M = ~33h). Sera probablement etale sur 4 semaines (semaines 11-14). Le bridge PyO3 (STORY-024/026) est le risque technique principal — premiere integration Rust/Python du projet.
