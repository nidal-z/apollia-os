# Sprint 2 — Bilan

**Sprint Goal :** `bash_executor.run("echo hello")` retourne stdout, l'invocation est tracée dans SQLite — **atteint ✅**
**Demo :** `cargo test -p apollia-tools` passe (55 tests) + `cargo test --workspace` propre

---

## Stories livrées

| ID | Story | Taille estimée | Temps réel | Dérive |
|---|---|---|---|---|
| STORY-010 | ToolDescriptor, ToolKind types dans apollia-tools | S (2h) | ~2h | 0 |
| STORY-011 | ToolRegistry catalogue en mémoire (acteur Tokio) | M (3h) | ~3h | 0 |
| STORY-015 | file_io avec validation path traversal | M (3h) | ~3.5h | +0.5h |
| STORY-013 | bash_executor avec Linux namespaces / Dev mode macOS | L (6h) | ~5h | -1h |
| STORY-016 | Audit trail SQLite (tool_invocations) | M (3h) | ~3h | 0 |
| STORY-014 | python_executor avec virtualenv isolé | L (6h) | ~6.5h | +0.5h |
| STORY-012 | ToolResolver validation à INITIALIZING | M (3h) | ~3h | 0 |

**Total estimé :** 26h / budget 24-30h — sprint dans les clous. Aucune story reportée.

---

## Ce qui a bien marché

- **Pattern acteur Tokio strict confirmé à grande échelle :** `ToolRegistryHandle` (M) + `AuditTrailHandle` (thread std) prouvent que le pattern scale naturellement sans `Arc<Mutex<T>>`.
- **ADR-012 anticipé dans le plan :** Le risque macOS/Linux namespaces était identifié avant l'implémentation. Le mode `Dev` via `#[cfg(target_os)]` compile-time est propre, sans flag runtime ni dead_code warning.
- **BashExecutor zéro-sized struct :** Suppression du champ `sandbox_mode` (qui aurait généré un clippy dead_code sur la plateforme opposée) — résolution compile-time pure via `#[cfg]`. Décision prise de manière autonome, correcte architecturalement.
- **`tokio::select!` + `tokio::spawn` pour stdout/stderr :** Pattern robuste : no zombie, no deadlock pipe-buffer sur large outputs. Réutilisé identiquement dans `python_executor`.
- **PythonExecutor idempotent :** `setup_venv` avec `--clear` si le venv est corrompu (liens brisés). Restart-safe dès `INITIALIZING`.
- **AuditTrail fire-and-forget :** `mpsc::sync_channel(1024)` sur thread `std` (pas Tokio) — isolation propre du runtime async. WAL activé pour les writes concurrents. sha2 ajouté proprement dans `[workspace.dependencies]`.
- **55 tests dans apollia-tools :** Couverture exhaustive (AC positifs + négatifs + edge cases). Toutes les stories ont dépassé la spec minimale en nombre de tests.

---

## Ce qui a posé problème

- **STORY-015 avant STORY-013 dans le plan, ordre inversé dans la pratique :** file_io livré après bash_executor. Aucun impact (pas de dépendance entre les deux), mais légère désynchronisation avec le plan d'implémentation.
- **`AgentManifest` étendu en cours de sprint (STORY-012) :** L'ajout de `dangerous_tools_allowed: bool` avec `#[serde(default)]` a nécessité de mettre à jour toutes les constructions de `AgentManifest` dans le workspace — friction non anticipée dans le plan, ~30min de correctifs cross-crate.
- **`sha2` absent du workspace (risque #4 du plan) :** Effectivement rencontré lors de STORY-016. Correction triviale (ajout dans `[workspace.dependencies]`), risque bien identifié en amont.
- **Risque #3 python_executor (risque #3 du plan) :** Venvs corrompus rencontrés en pratique lors du développement — résolu par le `--clear` idempotent. L'implémentation est plus robuste que la spec initiale.

---

## Stories reportées

Aucune.

---

## Décisions architecturales prises

| ADR | Décision | Story déclencheuse |
|---|---|---|
| ADR-012 | `SandboxMode::Dev` sur macOS via `#[cfg(target_os)]` compile-time — `sandbox-exec` macOS rejeté (déprécié) | STORY-013 |

**Décision non-ADR (mineure) :**
- `AgentManifest.dangerous_tools_allowed: bool` avec `#[serde(default = false)]` — granularité globale (par agent, pas par outil). Décision pragmatique Sprint 2, roadmap v0.2 pour granularité par outil.

---

## Dette technique identifiée

| # | Dette | Sévérité | Sprint cible |
|---|---|---|---|
| DT-010 | Cgroups CPU/RAM non appliqués — bash/python executors peuvent consommer des ressources illimitées | Moyenne | Sprint 6 (hardening) |
| DT-011 | Mount namespace sans tmpfs dédié — bash_executor utilise `--mount` mais sans overlay/tmpfs, un agent peut écrire sur le FS hôte dans le namespace | Moyenne | Sprint 6 (hardening) |
| DT-012 | `dangerous_tools_allowed` est global (par agent) — pas de whitelist par outil. Un agent soit autorise tous les outils dangereux, soit aucun | Faible | v0.2 post-MVP |
| DT-013 | Outils natifs (bash, python, file_io) non enregistrés automatiquement dans ToolRegistry au démarrage — enregistrement manuel uniquement | Faible | Sprint 5 (Supervisor) |
| DT-014 | AuditTrail utilise `std::thread` + `sync_channel` — architecture hybride sync/async acceptable MVP mais à unifier sous Tokio à terme | Faible | Sprint 6 (hardening) |

**Dettes Sprint 1 toujours ouvertes :** DT-006 (AgentId String alias), DT-007 (AgentStopping event), DT-008 (dead_code allows), DT-009 (AgentRegistry::spawn pub), DT-004 (process story updates).

---

## Focus Sprint 3

**Sprint Goal cible :** `memory.search("devis Dupont")` retourne 3 résultats classés BM25.

Stories à implémenter dans l'ordre :
1. STORY-017 — Schema SQLite complet + migrations versionnées (M)
2. STORY-018 — EpisodicMemory backend (record/history/TTL) (M)
3. STORY-019 — SemanticMemory backend (remember/recall/forget) (M)
4. STORY-020 — FTS5 search avec tokenizer unicode61 + BM25 (M)
5. STORY-021 — MemoryManager namespace isolation (M)
6. STORY-022 — ProceduralMemory backend (S)
7. STORY-023 — CLI `apollia-os memory inspect` preview (S)

**Note :** 5×M + 2×S ≈ 5×3h + 2×2h = 19h — sprint dans le budget (18-20h).
**Point d'attention :** rusqlite FTS5 + migrations versionnées sont nouveaux dans ce projet — prévoir du temps d'exploration pour STORY-017 et STORY-020.
