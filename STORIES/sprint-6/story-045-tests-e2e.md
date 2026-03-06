# [SPRINT-6][tests] Tests d'integration end-to-end

**ID :** STORY-045
**Sprint :** 6
**Crate cible :** `tests/` (workspace-level) + multi-crates
**Fichier(s) cible(s) :** `tests/integration/test_hello_agent.rs`, `tests/integration/test_resilience.rs`
**Taille :** L
**Depend de :** STORY-044 (Agent devis-generator)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que developpeur,
je veux des tests d'integration end-to-end qui exercent la chaine complete du runtime,
afin de detecter les regressions et valider que toutes les briques fonctionnent ensemble.
```

---

## Contexte technique

DT-025 (test e2e complet) est ouverte depuis le Sprint 4. La chaine TaskRouter → Coordinator → ORIA → AIPBridge → Python n'a jamais ete exercee par un test automatise. STORY-045 couvre ce gap critique.

Les tests e2e necessitent PyO3 et un environnement Python configure. Ils sont gates derriere un feature flag `python-tests` pour ne pas bloquer la CI sur les environnements sans Python.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #4 — Fail fast (les tests valident que les erreurs sont detectees au demarrage)

**Position dans l'architecture :**
```
Test e2e
  └── Supervisor.start() → tous les acteurs
        └── CLI/API → TaskRouter → Coordinator → ORIA → AIPBridge → hello_agent.py
```

---

## Criteres d'Acceptation

### AC-1 — Test chaine complete hello_agent

```
ETANT DONNE un Supervisor demarre avec tous les acteurs
ET hello_agent.py charge via AIPLoader
QUAND une tache est soumise via TaskRouterHandle
ALORS la tache est executee par hello_agent via AIPBridge
ET AIPResult(status=completed) est retourne
ET TaskCompleted event est emis sur l'EventBus
```

### AC-2 — Test agent avec manifest invalide echoue au demarrage

```
ETANT DONNE un fichier Python sans manifest()
QUAND l'agent est charge via AIPLoader
ALORS AIPValidationError est retourne
ET l'agent n'est pas enregistre dans le registry
```

### AC-3 — Test circuit breaker se declenche

```
ETANT DONNE un ResilienceLayer avec failure_threshold=3
QUAND 3 erreurs Transient consecutives surviennent sur un outil
ALORS le CircuitBreaker passe en Open
ET les appels suivants retournent CircuitOpen sans appeler l'outil
ET apres cooldown, le circuit passe en HalfOpen
```

### AC-4 — Test graceful shutdown avec tache en cours

```
ETANT DONNE une tache en cours d'execution
QUAND ShutdownRequested est emis
ALORS la tache se termine (drain jusqu'a 30s)
ET l'agent passe en STOPPED
ET le Supervisor arrete tous les acteurs dans l'ordre inverse
```

### AC-5 — Test StepBudget enforcement en conditions reelles

```
ETANT DONNE un agent avec StepBudget max_steps=2
QUAND l'agent execute une tache qui depasse le budget
ALORS AIPResult(status=failed, error="STEP_BUDGET_EXCEEDED") est retourne
```

### AC-6 — Tests gates derriere feature flag

```
ETANT DONNE un environnement CI sans Python configure
QUAND cargo test --workspace est execute (sans feature python-tests)
ALORS les tests e2e Python sont ignores
ET tous les autres tests passent
```

---

## Specification technique

### Structure des tests

```
tests/
└── integration/
    ├── test_hello_agent.rs      # AC-1, AC-2 — chaine complete avec Python
    ├── test_resilience.rs       # AC-3 — circuit breaker + retry
    ├── test_shutdown_e2e.rs     # AC-4 — graceful shutdown avec agent reel
    └── test_budget_e2e.rs       # AC-5 — StepBudget enforcement
```

### Feature flag

```toml
# Dans Cargo.toml workspace
[features]
python-tests = []

# Dans tests/ crate
#[cfg(feature = "python-tests")]
mod test_hello_agent;
```

### Approche de test

**Tests avec Python (feature python-tests) :**
- Chargent un vrai agent Python via AIPLoader
- Exercent la chaine complete AIPBridge → Python → retour Rust
- Necessitent `PYO3_PYTHON` configure (ADR-013)

**Tests sans Python (toujours actifs) :**
- Utilisent des mock runners (AgentRunner trait)
- Testent la resilience, le circuit breaker, le shutdown
- Ne necessitent aucune dep Python

### Ce que cette story N'implemente PAS

- Les tests de performance / charge — hors scope
- Les tests d'API HTTP (curl-like) — les routes sont deja testees unitairement
- Les benchmarks — hors scope MVP
- Le test de l'agent devis_generator complet (trop fragile, depends on file system) — test manuel

---

## Tests requis

(Les tests sont le livrable principal de cette story)

```rust
// test_hello_agent.rs
#[cfg(feature = "python-tests")]
#[tokio::test]
async fn test_hello_agent_full_chain() { /* AC-1 */ }

#[cfg(feature = "python-tests")]
#[tokio::test]
async fn test_invalid_agent_fails_at_load() { /* AC-2 */ }

// test_resilience.rs
#[tokio::test]
async fn test_circuit_breaker_opens_on_threshold() { /* AC-3 */ }

#[tokio::test]
async fn test_circuit_breaker_half_open_after_cooldown() { /* AC-3 */ }

// test_shutdown_e2e.rs
#[tokio::test]
async fn test_shutdown_drains_active_tasks() { /* AC-4 */ }

// test_budget_e2e.rs
#[tokio::test]
async fn test_budget_exceeded_returns_failed() { /* AC-5 */ }
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test --workspace` passe (sans python-tests)
- [ ] `cargo test --workspace --features python-tests` passe (avec Python)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] Tests deterministes (pas de flaky tests)
- [ ] DT-025 fermee

**Couverture :**
- [ ] Chaine complete exercee au moins 1 fois avec un vrai agent Python
- [ ] Circuit breaker teste en isolation
- [ ] Shutdown avec drain teste
- [ ] StepBudget enforcement teste

**Commit :**
- [ ] `test(workspace): add end-to-end integration tests for full runtime chain`

---

## Liens

- DT-025 : test e2e complet (ouverte depuis Sprint 4)
- Story precedente : STORY-044 (Agent devis-generator)
- Story suivante : STORY-046 (README)
- ADR-013 : PYO3_PYTHON configuration macOS
