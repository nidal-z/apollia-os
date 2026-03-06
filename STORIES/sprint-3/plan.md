# Sprint 3 — Plan

**Sprint Goal :** `memory.search("devis Dupont")` retourne 3 resultats classes BM25 depuis SQLite FTS5 avec tokenizer `unicode61`.
**Duree estimee :** 19h / budget 18-24h (3 semaines)
**Dates :** semaines 8-10

---

## Stories du sprint (ordre d'implementation)

| Priorite | ID | Story | Taille | Estime | Depend de |
|---|---|---|---|---|---|
| 1 | STORY-017 | Schema SQLite complet + migrations versionnees | M | 3h | STORY-016 ✅ (rusqlite pattern) |
| 2 | STORY-018 | EpisodicMemory backend (record/history/TTL) | M | 3h | STORY-017 |
| 3 | STORY-019 | SemanticMemory backend (remember/recall/forget) | M | 3h | STORY-017 |
| 4 | STORY-020 | FTS5 search avec tokenizer unicode61 + BM25 | M | 3h | STORY-017, STORY-018, STORY-019 |
| 5 | STORY-021 | MemoryManager namespace isolation | M | 3h | STORY-018, STORY-019, STORY-020 |
| 6 | STORY-022 | ProceduralMemory backend | S | 2h | STORY-017 |
| 7 | STORY-023 | CLI `apollia-os memory inspect` preview | S | 2h | STORY-021 |

**Sprint Goal atteint apres :** STORY-017 + 018 + 019 + 020 + 021 = 15h (fin semaine 9)
**Stories complementaires :** STORY-022, STORY-023 = 4h (semaine 10)

---

## Dependances verifiees

| Dependance | Status | Story dependante |
|---|---|---|
| `rusqlite` bundled + WAL pattern (STORY-016) | ✅ Sprint 2 | STORY-017 (reutilisation du pattern) |
| `AgentManifest.memory_namespace` (STORY-002) | ✅ Sprint 0 | STORY-021 (namespace isolation) |
| `AgentManifest.shared_memory_namespaces` (STORY-002) | ✅ Sprint 0 | STORY-021 (cross-namespace read) |
| `uuid` dans workspace deps | ✅ | STORY-017, 018 (generation d'IDs) |
| `apollia-cli` crate squelette | ✅ Sprint 0 | STORY-023 (clap commande) |

**Note STORY-017 :** Le pattern SQLite + WAL est deja valide dans `apollia-tools/src/audit.rs` (STORY-016). STORY-017 reutilise le meme pattern (`rusqlite::Connection`, WAL mode) mais dans `apollia-memory` avec un schema different (3 tables + FTS5).

**Note STORY-023 :** Preview CLI uniquement — sous-commande `memory inspect <namespace>` qui lit directement la DB. Pas besoin du runtime complet (pas de gRPC/API), lecture directe SQLite.

---

## Risques identifies

### Risque #1 — FTS5 non disponible dans rusqlite bundled (MOYEN)
- **Contexte :** FTS5 est une extension SQLite. Le feature flag `bundled` de rusqlite compile SQLite from source, mais FTS5 doit etre active explicitement.
- **Impact :** STORY-020 echoue si FTS5 n'est pas compile.
- **Mitigation :** Verifier que `rusqlite` avec `features = ["bundled"]` inclut FTS5 par defaut (c'est le cas depuis rusqlite 0.29+). Ajouter un test d'initialisation `CREATE VIRTUAL TABLE ... USING fts5(...)` dans STORY-017 pour detecter le probleme tot.

### Risque #2 — unicode61 tokenizer et accentuation francaise (FAIBLE)
- **Contexte :** Le tokenizer `unicode61` est critique pour la cible PME francaise ("reunion" doit matcher "reunion").
- **Impact :** Si `unicode61` n'est pas supporte, la recherche FTS5 sera degradee.
- **Mitigation :** `unicode61` est le tokenizer par defaut de FTS5 depuis SQLite 3.7.13. Avec `bundled`, la version SQLite est recente. Test explicite dans STORY-020.

### Risque #3 — Performance BM25 ranking sur gros volumes (FAIBLE)
- **Contexte :** BM25 est calcule par FTS5 via `rank` ou `bm25()`.
- **Impact :** Negligeable pour MVP (< 10K entries par namespace).
- **Mitigation :** Pas d'optimisation prematuree. Documenter comme dette technique si > 100ms sur 10K entries.

### Risque #4 — Schema migration versioning first-time (MOYEN)
- **Contexte :** Premier systeme de migration du projet. Pas de framework type `refinery` ou `sqlx-migrate`.
- **Impact :** STORY-017 doit definir un pattern de migration simple et reproductible.
- **Mitigation :** Pattern minimaliste : table `_schema_version(version INTEGER)` + fonctions `migrate_to_vN()` chainables. Pas de framework externe — principe #2.

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint et demo-able : `memory.search("devis Dupont")` retourne des resultats BM25
- [ ] `cargo test --workspace` passe (0 test echoue)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] `sprint-index.md` mis a jour (toutes les stories ✅)
- [ ] `sprint-3/bilan.md` redige

---

## Ordre d'implementation detail

```
semaine 8
  jour 1-2 : STORY-017 — Schema SQLite + migrations (3h)
  jour 3-4 : STORY-018 — EpisodicMemory backend (3h)
  jour 5   : STORY-019 — SemanticMemory backend (debut)

semaine 9
  jour 1   : STORY-019 — SemanticMemory backend (fin, 3h total)
  jour 2-3 : STORY-020 — FTS5 search + BM25 (3h) <- Sprint Goal ATTEINT apres integration
  jour 4-5 : STORY-021 — MemoryManager namespace isolation (3h)

semaine 10
  jour 1   : STORY-022 — ProceduralMemory backend (2h)
  jour 2   : STORY-023 — CLI memory inspect preview (2h)
  jour 3   : Buffer / dette technique / bilan sprint
```
