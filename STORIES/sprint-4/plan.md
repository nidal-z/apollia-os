# Sprint 4 — Plan

**Sprint Goal :** `apollia-os run hello-agent "Bonjour"` — un agent Python s'execute dans le runtime Rust via PyO3, avec StepBudget enforce et dispatch complet TaskRouter → ExecutionCoordinator → ORIA → AIPBridge → Python.
**Duree estimee :** 36h / budget 32-40h (4 semaines)
**Dates :** semaines 11-14

---

## Stories du sprint (ordre d'implementation)

| Priorite | ID | Story | Crate | Taille | Estime | Depend de |
|---|---|---|---|---|---|---|
| 1 | STORY-024 | Chargement module Python via PyO3 | apollia-aip | L | 6h | Sprint 3 ✅ |
| 2 | STORY-025 | Validation AIP duck typing (manifest + run async) | apollia-aip | M | 3h | STORY-024 |
| 3 | STORY-026 | Bridge Tokio ↔ asyncio via pyo3-async-runtimes | apollia-aip | L | 6h | STORY-024 |
| 4 | STORY-029 | Observer + ContextBundle + classify() | apollia-oria | M | 3h | Sprint 3 ✅ (memoire) |
| 5 | STORY-027 | ToolProxy Python → outils Rust | apollia-aip | M | 3h | STORY-026 |
| 6 | STORY-028 | MemoryInterface Python → apollia-memory | apollia-aip | M | 3h | STORY-026 |
| 7 | STORY-030 | ORIA Mode Direct + StepBudget enforcement | apollia-oria | L | 6h | STORY-026, 027, 028, 029 |
| 8 | STORY-031 | ExecutionCoordinator + semaphore concurrence | apollia-runtime | M | 3h | STORY-030 |
| 9 | STORY-032 | TaskRouter dispatch | apollia-runtime | M | 3h | STORY-031 |

**Jalons intermediaires :**
- Apres STORY-026 (semaine 12) : "agent Python charge, valide, et callable en async depuis Rust" — premier jalon demo-able
- Apres STORY-030 (semaine 13) : "agent execute avec StepBudget enforce" — ORIA Direct fonctionne
- Apres STORY-032 (semaine 14) : Sprint Goal atteint — chaine complete

**Note :** STORY-029 (Observer) est parallelisable avec STORY-025/026 car elle ne depend que de apollia-memory (Sprint 3 ✅) et apollia-core. Elle peut etre implementee pendant que le bridge PyO3 se stabilise.

---

## Dependances verifiees

| Dependance | Statut | Story dependante |
|---|---|---|
| `pyo3` 0.22 + `auto-initialize` dans workspace deps | ✅ | STORY-024, 025, 026 |
| `pyo3-async-runtimes` 0.22 + `tokio-runtime` dans workspace deps | ✅ | STORY-026 |
| `apollia-aip/Cargo.toml` avec toutes les deps (core, tools, memory, pyo3) | ✅ | STORY-024 a 028 |
| `apollia-oria/Cargo.toml` avec deps (core, tools, memory) | ✅ | STORY-029, 030 |
| `ToolRegistryHandle` acteur Tokio (STORY-011) | ✅ Sprint 2 | STORY-027 (ToolProxy) |
| `AuditTrailHandle` fire-and-forget (STORY-016) | ✅ Sprint 2 | STORY-027 (audit des appels) |
| `MemoryManager` namespace isolation (STORY-021) | ✅ Sprint 3 | STORY-028 (MemoryInterface) |
| `EpisodicMemory`, `SemanticMemory`, search FTS5 (STORY-018, 019, 020) | ✅ Sprint 3 | STORY-028, 029 |
| `AgentRegistryHandle` (STORY-008) | ✅ Sprint 1 | STORY-031, 032 |
| `EventBusSender` broadcast (STORY-006) | ✅ Sprint 1 | STORY-031, 032 |
| `StepBudgetConfig` dans apollia-core (STORY-004) | ✅ Sprint 0 | STORY-030 |
| `AIPTask`, `AIPResult`, `AgentManifest` (STORY-002) | ✅ Sprint 0 | Toutes |
| `ProcessState` + `can_transition_to()` (STORY-003, 007) | ✅ Sprint 0-1 | STORY-032 |
| Python 3.10+ installe sur la machine de dev (macOS) | ✅ Prerequis | STORY-024 a 028 |

---

## Risques identifies

### Risque #1 — Compatibilite PyO3 0.22 + pyo3-async-runtimes 0.22 sur macOS (ELEVE)
- **Contexte :** PyO3 0.22 a introduit des changements d'API (Bound vs GIL Refs). `pyo3-async-runtimes` doit etre a une version compatible. La compilation sur macOS peut necessiter des flags specifiques (`PYO3_PYTHON`, `MACOSX_DEPLOYMENT_TARGET`).
- **Impact :** STORY-024 bloquee si la compilation echoue.
- **Mitigation :** Tester `cargo build -p apollia-aip` des le debut du sprint. Creer un ADR si un downgrade de version est necessaire. Verifier que `python3-config --ldflags` fonctionne.

### Risque #2 — GIL contention entre Tokio et Python async (ELEVE)
- **Contexte :** Le GIL Python doit etre acquis pour tout appel PyO3. Si un agent Python fait un long calcul synchrone, le GIL bloque tous les autres agents Python.
- **Impact :** Performance degradee avec plusieurs agents concurrents.
- **Mitigation :** Sprint 4 se concentre sur un agent a la fois (max_concurrent_tasks=1 par defaut). Le GIL doit etre release pendant les awaits Python. Documenter le pattern `py.allow_threads()` dans les stories. Multi-agent = optimisation Sprint 6+.

### Risque #3 — Serialisation AIPTask/AIPResult entre Rust et Python (MOYEN)
- **Contexte :** Les types AIPTask et AIPResult doivent etre convertis en dicts Python (et inversement). PyO3 ne serialise pas automatiquement les structs Rust complexes vers Python.
- **Impact :** STORY-026 AC-5 pourrait etre plus complexe que prevu.
- **Mitigation :** Utiliser `serde_json::to_value()` cote Rust puis `pythonize()` (crate `pythonize` ou conversion manuelle dict). Evaluer si `pythonize` doit etre ajoute aux workspace deps ou si la conversion manuelle suffit.

### Risque #4 — ORIA Engine couplage avec AIPBridge non encore testable (MOYEN)
- **Contexte :** STORY-030 (ORIAEngine) depend du bridge Python (STORY-026) mais les tests unitaires ne devraient pas necessiter un interpreteur Python complet.
- **Impact :** Tests de STORY-030 plus complexes a ecrire.
- **Mitigation :** Extraire un trait `AgentBridge` avec `call_run()` pour permettre le mocking dans les tests ORIA. Seuls les tests d'integration end-to-end (STORY-032+) necessitent Python reel.

### Risque #5 — Premiere utilisation de pyo3-async-runtimes (MOYEN)
- **Contexte :** C'est la premiere fois qu'on bridge Tokio ↔ asyncio. La documentation de `pyo3-async-runtimes` est limitee.
- **Impact :** STORY-026 pourrait prendre plus de 6h.
- **Mitigation :** Commencer par un PoC minimal (appel async Python depuis Rust) avant d'implementer le bridge complet. Prevoir 2h de buffer dans l'estimation L.

---

## Crates impactees

| Crate | Stories | Etat avant sprint | Etat apres sprint |
|---|---|---|---|
| `apollia-aip` | STORY-024, 025, 026, 027, 028 | Squelette vide (lib.rs doc only) | Bridge PyO3 complet : loader, validator, bridge async, ToolProxy, MemoryInterface |
| `apollia-oria` | STORY-029, 030 | Squelette vide (lib.rs doc only) | Observer + classify(), ORIAEngine Mode Direct, StepBudget runtime |
| `apollia-runtime` | STORY-031, 032 | EventBus + AgentRegistry | + ExecutionCoordinator + TaskRouter |
| `apollia-core` | (aucune nouvelle story) | Stable | Potentiellement un trait `AgentBridge` si necessaire |

---

## Nouvelles dependances potentielles

| Crate | Usage | Decision |
|---|---|---|
| `pythonize` | Conversion Rust serde ↔ Python dict via PyO3 | A evaluer dans STORY-026. Si adopte → ajouter au workspace + ADR |

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint : `apollia-os run hello-agent "Bonjour"` execute un agent Python et retourne un AIPResult
- [ ] `cargo test --workspace` passe (0 test echoue)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] `sprint-index.md` mis a jour (toutes les stories ✅)
- [ ] `sprint-4/bilan.md` redige
- [ ] Au moins 1 ADR cree (Risque #1 ou #3 si deviation)

---

## Ordre d'implementation detail

```
semaine 11
  jour 1-2 : STORY-024 — Chargement module Python via PyO3 (6h)
             PoC: cargo build -p apollia-aip compile avec PyO3
             Impl: load_agent_module() + AIPLoaderError
  jour 3-4 : STORY-025 — Validation AIP duck typing (3h)
             validate_agent() + ValidatedAgent + AIPValidationError
  jour 5   : STORY-029 — Observer + ContextBundle + classify() (debut, 1h)

semaine 12
  jour 1-3 : STORY-026 — Bridge Tokio ↔ asyncio (6h) <- jalon "bridge fonctionne"
             AIPBridge + call_run/on_start/on_stop
             Risque #5 mitigation ici
  jour 4   : STORY-029 — Observer (fin, 2h total)
             observe() + classify() + ContextBundle
  jour 5   : STORY-027 — ToolProxy Python → outils Rust (debut, 1h)

semaine 13
  jour 1   : STORY-027 — ToolProxy (fin, 3h total)
             #[pyclass] ToolProxy + call() + audit
  jour 2   : STORY-028 — MemoryInterface Python → apollia-memory (3h)
             #[pyclass] MemoryInterface + record/remember/recall/search/forget
  jour 3-5 : STORY-030 — ORIA Mode Direct + StepBudget (6h) <- jalon "ORIA fonctionne"
             StepBudget runtime + ORIAEngine.execute_direct()

semaine 14
  jour 1-2 : STORY-031 — ExecutionCoordinator + semaphore (3h)
  jour 3-4 : STORY-032 — TaskRouter dispatch (3h) <- Sprint Goal ATTEINT
  jour 5   : Buffer / dette technique / bilan sprint
```
