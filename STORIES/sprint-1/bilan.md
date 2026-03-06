# Sprint 1 — Bilan

**Sprint Goal :** EventBus + AgentRegistry fonctionnels, validés par un test d'intégration qui prouve les transitions ProcessState via broadcast Tokio — **atteint ✅**
**Demo :** `cargo test -p apollia-runtime` passe (17 tests : 13 unitaires runtime + 4 integration) + `cargo test --workspace` propre

---

## Stories livrées

| ID | Story | Taille estimée | Statut |
|---|---|---|---|
| STORY-006 | EventBus broadcast Tokio + RuntimeEvent catalogue | M | ✅ |
| STORY-007 | AgentRegistry acteur Tokio (Register/Unregister/UpdateState) | M | ✅ |
| STORY-008 | AgentRegistryHandle API publique async | S | ✅ |
| STORY-009 | Test d'integration EventBus ↔ AgentRegistry | M | ✅ |

Charge estimee : 11h / budget 16-20h — sprint sous-charge, aucune story reportee.

---

## Ce qui a bien marche

- **Pattern acteur Tokio strict** : `mpsc::channel` + struct privee + Handle `Clone` — zéro `Arc<Mutex<T>>` cross-acteurs. Principe #5 respecte a 100%.
- **Separation `apollia-core` / `apollia-runtime`** : `RuntimeEvent` et `can_transition_to()` dans `apollia-core`, zero logique metier dupliquee dans le runtime.
- **Test d'integration bien isole** : `tests/integration_registry.rs` utilise uniquement les APIs publiques — aucun acces aux internals de l'acteur.
- **Lagged consumer gere proprement** : `RecvError::Lagged` loggue un warning `tracing::warn!`, jamais de panic. Principe #4 respecte.
- **STORY-007 + STORY-008 naturellement groupees** : l'API publique du Handle est triviale une fois l'acteur defini — bonne granularite de story.
- **`broadcast::channel(1024)` simple et efficace** : pas de sur-ingenierie, la taille du buffer sera reajustee empiriquement si besoin.

---

## Ce qui a pose probleme

- **Statut STORY-008 non mis a jour** : le fichier `story-008` est reste a "A faire" alors que la story a ete livree dans le meme commit que STORY-007. Meme probleme que Sprint 0 (DT-004). Action : mettre a jour le statut dans le fichier story immediatement apres implementation.
- **Deviation sur le compte d'evenements (STORY-009 AC-1)** : la spec prevoyait 7 evenements pour le cycle de vie complet, la realite est 6. `RuntimeEvent` n'a pas de variante `AgentStopping` — la transition `Active → Stopping` n'emet aucun evenement. Bug dans le template de story, pas dans l'architecture. Corrige directement dans la story sans ADR.
- **`AgentRegistry` rendu `pub`** : initialement `pub(crate)`, doit etre `pub` pour etre importe depuis `tests/`. Acceptable (struct sans champs publics, seul `spawn()` accessible), mais note comme ecart par rapport a la spec.

---

## Stories reportees

Aucune.

---

## Decisions architecturales prises

- **ADR-011** : `AgentId` et `TaskId` sont des type aliases `String` dans `apollia-core/src/events.rs`. Choix pragmatique Sprint 1 — migration possible vers newtypes a Sprint 3+ si la type-safety devient un besoin reel.

---

## Dette technique identifiee

| # | Dette | Severite | Sprint cible |
|---|---|---|---|
| DT-006 | `AgentId` / `TaskId` sont des `String` aliases — pas de type-safety entre les deux | Faible | Sprint 3+ (si besoin) |
| DT-007 | `RuntimeEvent` n'a pas de variante `AgentStopping` — impossible de distinguer "en cours d'arret" de "arrete" dans les consumers | Faible | Sprint 5 (Supervisor) |
| DT-008 | `#[allow(dead_code)]` sur `RegistryMessage` et `AgentRegistry` struct — sera retire quand le Supervisor les utilisera | Faible | Sprint 5 (STORY-039) |
| DT-009 | `AgentRegistry::spawn()` est `pub` uniquement pour les tests d'integration — idealement `pub(crate)` + test helper | Faible | Sprint 5 |
| DT-004 | Statut des stories non mis a jour en cours de sprint (recidive Sprint 0) — process a ameliorer | Moyen | Process continu |

---

## Focus Sprint 2

**Sprint Goal cible :** `bash_executor.run("echo hello")` → stdout trace dans SQLite.

Stories a implementer dans l'ordre :
1. STORY-010 — ToolDescriptor, ToolKind, SandboxProfile types (S)
2. STORY-011 — ToolRegistry catalogue en memoire (M)
3. STORY-012 — ToolResolver validation a INITIALIZING (M)
4. STORY-013 — bash_executor avec Linux namespaces (unshare) (L)
5. STORY-014 — python_executor avec virtualenv isole (L)
6. STORY-015 — file_io avec validation path traversal (M)
7. STORY-016 — Audit trail SQLite (tool_invocations) (M)

Charge estimee : 6×2h + 2×6h + 2×3h = S×2 + L×2 + M×4 = 4h + 12h + 12h ≈ 28h — sprint charge, probablement a decouper ou reduire le scope (STORY-013/014 sont des L).
