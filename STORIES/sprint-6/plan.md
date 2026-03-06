# Sprint 6 — Plan

**Sprint Goal :** Demo client reelle — agent devis-generator operationnel de bout en bout, tout local, zero cloud.
**Duree estimee :** 24h / budget 16-20h + buffer (2 semaines)
**Dates :** semaines 18-19

---

## Stories du sprint (ordre d'implementation)

| Priorite | ID | Story | Crate | Taille | Estime | Depend de |
|---|---|---|---|---|---|---|
| 1 | STORY-041 | ResilienceLayer circuit breaker par outil | apollia-oria | L | 6h | Sprint 4 (ORIA Engine) |
| 2 | STORY-042 | Retry policy avec backoff exponentiel + jitter | apollia-oria | M | 3h | STORY-041 |
| 3 | STORY-044 | Agent devis-generator complet | agents/ + apollia-runtime | L | 6h | STORY-041, STORY-042 |
| 4 | STORY-045 | Tests d'integration end-to-end | tests/ + multi-crates | L | 6h | STORY-044 |
| 5 | STORY-046 | README + documentation installation | docs/ | M | 3h | STORY-044 |

**Story reportee au Sprint 7 :**

| ID | Story | Taille | Raison |
|---|---|---|---|
| STORY-043 | ORIA Mode Orchestre + Reasoner LLM | XL (10h) | Non necessaire pour la demo MVP. L'agent devis-generator tourne en Mode Direct. Le Mode Orchestre sera utile quand des cas multi-etapes reels apparaitront. |

**Jalons intermediaires :**
- Apres STORY-042 (semaine 18) : "ResilienceLayer + RetryPolicy operationnels, circuit breaker testable" — premier jalon
- Apres STORY-044 (semaine 19) : "Agent devis-generator tourne en Mode Direct avec resilience" — Sprint Goal MVP atteint
- Apres STORY-046 (semaine 19) : Sprint Goal complet — demo + docs

---

## Dependances verifiees

| Dependance | Statut | Story dependante |
|---|---|---|
| `ORIAEngine` + `StepBudget` (STORY-030) | Sprint 4 | STORY-041, 042 |
| `ToolProxy` Python (STORY-027) | Sprint 4 | STORY-044 |
| `MemoryInterface` Python (STORY-028) | Sprint 4 | STORY-044 |
| `AIPBridge` call_run/call_on_start/call_on_stop (STORY-026) | Sprint 4 | STORY-044 |
| `AIPLoader` + `ValidatedAgent` (STORY-024, 025) | Sprint 4 | STORY-044 |
| `Supervisor` + `APIServer` + `CLI` (Sprint 5) | Sprint 5 | STORY-044, 045 |
| `EventBus` broadcast (STORY-006) | Sprint 1 | STORY-041 (ToolCircuitBroken events) |
| `ToolRegistryHandle` (STORY-011) | Sprint 2 | STORY-041 |

---

## Risques identifies

### Risque #1 — Premier agent Python reel (STORY-044) exercant toute la chaine (ELEVE)
- **Contexte :** C'est la premiere fois que la chaine complete CLI -> Supervisor -> TaskRouter -> Coordinator -> ORIA -> AIPBridge -> Python est exercee. Des bugs d'integration sont probables.
- **Impact :** STORY-044 pourrait reveler des bugs dans les Sprints precedents, augmentant le temps reel.
- **Mitigation :** Commencer par un agent minimal (hello-world) avant le devis-generator complet. DT-031 (manifest_from_path MVP) doit etre resolu en prerequis.

### Risque #2 — DT-031 (manifest_from_path MVP) bloque STORY-044 (MOYEN)
- **Contexte :** `manifest_from_path()` dans routes_agents.rs retourne un manifest placeholder sans charger le module Python reel. Il faut integrer `AIPLoader` pour que `agent start` fonctionne reellement.
- **Impact :** STORY-044 ne peut pas fonctionner sans cette correction.
- **Mitigation :** Resoudre DT-031 comme prerequis de STORY-044 (integre dans la story).

### Risque #3 — Tests e2e necessitent Python + PyO3 (MOYEN)
- **Contexte :** STORY-045 exercera la chaine complete incluant PyO3. Les tests e2e necessitent un environnement Python configure (PYO3_PYTHON, venv).
- **Impact :** Les tests e2e pourraient etre fragiles sur macOS (ADR-013).
- **Mitigation :** Utiliser `#[cfg(feature = "python-tests")]` pour les tests necessitant Python reel. Fournir un script de setup.

---

## Crates impactees

| Crate | Stories | Etat avant sprint | Etat apres sprint |
|---|---|---|---|
| `apollia-oria` | STORY-041, 042 | Observer + ORIAEngine (Mode Direct) + StepBudget | + ResilienceLayer + RetryPolicy |
| `apollia-aip` | STORY-044 | Bridge + Loader + Validator + ToolProxy + MemoryInterface | Integration agent reel validee |
| `apollia-runtime` | STORY-044, 045 | Complet (Supervisor, API, CLI) | DT-031 resolu + tests e2e |
| `apollia-core` | STORY-041 | Stable | + RuntimeEvent::ToolCircuitBroken/Restored si manquants |
| `apollia-cli` | STORY-044 | CLI complete | Integration `run` avec agent Python reel |
| `agents/` | STORY-044 | Inexistant | hello_agent.py + devis_agent.py |
| `docs/` | STORY-046 | Architecture docs | + README.md + INSTALL.md |

---

## Nouvelles dependances potentielles

| Crate | Usage | Decision |
|---|---|---|
| `rand` 0.8 | Jitter aleatoire pour retry backoff (STORY-042) | Ajout workspace |
| Aucune dep Python | Les agents Python n'ont pas de deps Rust supplementaires | — |

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint : agent devis-generator execute une tache de bout en bout via `apollia-os run devis-generator "..."`
- [ ] `cargo test --workspace` passe (0 test echoue)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] `sprint-index.md` mis a jour (toutes les stories du sprint)
- [ ] `sprint-6/bilan.md` redige
- [ ] Au moins 1 ADR cree si deviation architecturale

---

## Ordre d'implementation detail

```
semaine 18
  jour 1-3 : STORY-041 — ResilienceLayer circuit breaker par outil (6h)
             CircuitBreaker struct + CircuitState machine d'etat
             ResilienceLayer struct avec HashMap<String, CircuitBreaker>
             ErrorClass (Transient/Permanent/BudgetExceeded/SandboxViolation)
             Integration avec ToolProxy (increment failure_count)
  jour 4-5 : STORY-042 — Retry policy avec backoff exponentiel + jitter (3h)
             RetryPolicy struct + execute() avec boucle retry
             Backoff exponentiel + jitter aleatoire
             Integration dans ResilienceLayer.execute()

semaine 19
  jour 1-3 : STORY-044 — Agent devis-generator complet (6h)
             Resoudre DT-031 (manifest_from_path)
             hello_agent.py minimal + test chaine complete
             devis_agent.py avec ToolProxy + MemoryInterface
  jour 4   : STORY-045 — Tests d'integration end-to-end (6h)
             Test chaine complete avec hello_agent.py
             Test resilience (circuit breaker declenche)
             Feature flag python-tests
  jour 5   : STORY-046 — README + documentation installation (3h)
             README.md principal + guide d'installation
             Bilan sprint
```
