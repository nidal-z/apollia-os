# [Sprint 4][apollia-oria] ORIA Mode Direct + StepBudget enforcement

**ID :** STORY-030
**Sprint :** 4
**Crate cible :** `apollia-oria`
**Fichier(s) cible(s) :** `crates/apollia-oria/src/budget.rs`, `crates/apollia-oria/src/engine.rs`
**Taille :** L (6h)
**Depend de :** STORY-026 (AIPBridge call_run) ✅, STORY-027 (ToolProxy) ✅, STORY-028 (MemoryInterface) ✅, STORY-029 (Observer classify/ContextBundle) ✅
**Statut :** 🔲 A faire

---

## User Story

En tant que runtime, je veux executer un agent en Mode Direct avec un StepBudget tri-dimensionnel (steps, tool_calls, wall_clock) applique par le runtime et non contournable par l'agent, afin de garantir des executions bornees en production.

## Contexte technique

Le StepBudget est le mecanisme de securite le plus important d'Apollia (Principe #7 : Garde-fous non-negociables). Il borne l'execution d'un agent selon trois dimensions :

1. **max_steps** : nombre maximal d'iterations de la boucle ReAct de l'agent
2. **max_tool_calls** : nombre maximal d'appels outils via ToolProxy
3. **wall_clock_limit** : duree maximale d'execution reelle (timeout absolu)

Les valeurs proviennent de `AgentManifest.step_budget` (suggestion de l'agent), mais sont plafonnees par la configuration runtime (`apollia.toml`). La valeur effective pour chaque dimension est `min(agent_config, runtime_default)`.

En Mode Direct, ORIA appelle `agent.run(task, ctx)` une seule fois. L'agent gere sa propre boucle interne. ORIA supervise via le tracking des appels ToolProxy qui incrementent les compteurs du StepBudget. Le budget est partage (Arc) entre ORIAEngine et ToolProxy.

Le StepBudget est expose en lecture seule a l'agent via `RuntimeContext.step_budget` (steps_left, tool_calls_left, elapsed).

## Criteres d'Acceptation

- **AC-1 :** `execute_direct()` appelle `bridge.call_run()` et retourne `AIPResult` en cas de succes
- **AC-2 :** `StepBudget::is_exhausted()` retourne `true` quand `max_steps` est atteint
- **AC-3 :** `StepBudget::is_exhausted()` retourne `true` quand `max_tool_calls` est atteint
- **AC-4 :** `StepBudget::is_exhausted()` retourne `true` quand `wall_clock_limit` est depasse
- **AC-5 :** `execute_direct()` retourne `ORIAError::BudgetExceeded` si le budget est deja epuise avant l'appel
- **AC-6 :** StepBudget est thread-safe (`Arc` + `AtomicU32`) — partage avec ToolProxy
- **AC-7 :** StepBudget utilise `min(agent_config, runtime_default)` pour chaque dimension

## Specification technique

### budget.rs — StepBudget

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_core::StepBudgetConfig;

/// Budget d'execution tri-dimensionnel applique par le runtime.
///
/// Thread-safe grace a `AtomicU32` pour les compteurs et `Instant` pour le chrono.
/// Partage via `Arc<StepBudget>` entre ORIAEngine et ToolProxy.
pub struct StepBudget {
    /// Nombre maximal de steps autorisees.
    pub max_steps: u32,
    /// Nombre maximal d'appels outils autorisees.
    pub max_tool_calls: u32,
    /// Duree maximale d'execution.
    pub wall_clock_limit: Duration,
    current_steps: AtomicU32,
    current_tool_calls: AtomicU32,
    started_at: Instant,
}

impl StepBudget {
    /// Cree un nouveau StepBudget a partir de la config.
    pub fn new(config: &StepBudgetConfig) -> Self { ... }

    /// Cree un StepBudget avec valeurs effectives = min(agent, runtime) par dimension.
    pub fn from_capped(agent: &StepBudgetConfig, runtime: &StepBudgetConfig) -> Self { ... }

    /// Retourne `true` si au moins une des trois dimensions est epuisee.
    pub fn is_exhausted(&self) -> bool { ... }

    /// Incremente le compteur de steps. Utilise `Ordering::Relaxed`.
    pub fn increment_steps(&self) { ... }

    /// Incremente le compteur d'appels outils. Utilise `Ordering::Relaxed`.
    pub fn increment_tool_calls(&self) { ... }

    /// Nombre de steps restantes.
    pub fn steps_left(&self) -> u32 { ... }

    /// Nombre d'appels outils restants.
    pub fn tool_calls_left(&self) -> u32 { ... }

    /// Duree ecoulee depuis le debut de l'execution.
    pub fn elapsed(&self) -> Duration { ... }

    /// Description lisible de la raison d'epuisement (pour les messages d'erreur).
    pub fn exhaustion_reason(&self) -> Option<String> { ... }
}
```

### engine.rs — ORIAEngine

```rust
use std::sync::Arc;
use pyo3::PyObject;
use apollia_core::{AIPTask, AIPResult};

/// Erreurs de l'engine ORIA.
#[derive(Debug, thiserror::Error)]
pub enum ORIAError {
    /// Budget d'execution epuise.
    #[error("step budget exceeded: {reason}")]
    BudgetExceeded { reason: String },

    /// Echec de l'execution de l'agent.
    #[error("agent execution failed: {0}")]
    ExecutionFailed(String),

    /// Erreur de l'Observer.
    #[error("observer error: {0}")]
    ObserverError(#[from] ObserverError),

    /// Erreur du bridge AIP.
    #[error("bridge error: {0}")]
    BridgeError(String),
}

/// Moteur d'execution ORIA (Observer-Reasoner-Actor).
///
/// Point d'entree principal pour l'execution des taches.
/// Gere le Mode Direct avec supervision StepBudget.
pub struct ORIAEngine { ... }

impl ORIAEngine {
    /// Execute une tache en Mode Direct.
    ///
    /// 1. Verifie que le budget n'est pas deja epuise
    /// 2. Appelle `bridge.call_run(task, ctx)`
    /// 3. Retourne `AIPResult` ou `ORIAError`
    ///
    /// Le StepBudget est supervise en parallele via `tokio::select!` :
    /// - branche 1 : `bridge.call_run()` termine normalement
    /// - branche 2 : polling periodique de `budget.is_exhausted()` (100ms interval)
    pub async fn execute_direct(
        &self,
        task: AIPTask,
        bridge: &AIPBridge,
        budget: Arc<StepBudget>,
        ctx: PyObject,
    ) -> Result<AIPResult, ORIAError> { ... }
}
```

## Tests requis

### Tests StepBudget (budget.rs) — 5 tests minimum

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_budget_not_exhausted() {
        // GIVEN un StepBudgetConfig avec des valeurs par defaut
        // WHEN on cree un StepBudget
        // THEN is_exhausted() retourne false
    }

    #[test]
    fn test_steps_exhausted() {
        // GIVEN un budget avec max_steps = 2
        // WHEN on incremente 2 fois
        // THEN is_exhausted() retourne true et steps_left() == 0
    }

    #[test]
    fn test_tool_calls_exhausted() {
        // GIVEN un budget avec max_tool_calls = 3
        // WHEN on incremente tool_calls 3 fois
        // THEN is_exhausted() retourne true et tool_calls_left() == 0
    }

    #[test]
    fn test_wall_clock_exhausted() {
        // GIVEN un budget avec wall_clock_limit = 1ms
        // WHEN on attend 5ms
        // THEN is_exhausted() retourne true
    }

    #[test]
    fn test_from_capped_takes_minimum() {
        // GIVEN agent config (max_steps=100, max_tool_calls=50, wall_clock=600)
        // AND runtime config (max_steps=10, max_tool_calls=20, wall_clock=300)
        // WHEN on cree via from_capped()
        // THEN max_steps=10, max_tool_calls=20, wall_clock=300
    }

    #[test]
    fn test_exhaustion_reason_steps() {
        // GIVEN un budget avec max_steps = 1
        // WHEN on incremente 1 fois
        // THEN exhaustion_reason() contient "steps"
    }

    #[test]
    fn test_thread_safety_concurrent_increments() {
        // GIVEN un Arc<StepBudget> avec max_steps = 1000
        // WHEN 10 threads incrementent 100 fois chacun
        // THEN current_steps == 1000 et is_exhausted() == true
    }
}
```

### Tests ORIAEngine (engine.rs) — 3 tests minimum

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_direct_budget_already_exhausted() {
        // GIVEN un budget deja epuise (max_steps=0)
        // WHEN on appelle execute_direct()
        // THEN retourne ORIAError::BudgetExceeded
    }

    #[tokio::test]
    async fn test_execute_direct_success() {
        // GIVEN un budget valide et un bridge mock qui retourne Ok(AIPResult)
        // WHEN on appelle execute_direct()
        // THEN retourne Ok(AIPResult) avec le resultat attendu
    }

    #[tokio::test]
    async fn test_execute_direct_bridge_error() {
        // GIVEN un budget valide et un bridge mock qui retourne Err
        // WHEN on appelle execute_direct()
        // THEN retourne ORIAError::BridgeError
    }
}
```

## Ce que cette story N'implemente PAS

- Le Mode Delegue (ORIA gere la boucle ReAct) — story future
- Le Mode Hybride — story future
- L'integration avec ToolProxy (increment automatique) — deja dans STORY-027
- La configuration runtime depuis `apollia.toml` — story future
- Le mecanisme de graceful shutdown mid-execution — story future
- Les metriques/telemetrie du budget — story future

## Definition of Done

- [ ] `budget.rs` compile sans warning
- [ ] `engine.rs` compile sans warning
- [ ] 8+ tests passent (`cargo test -p apollia-oria`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` dans le code commite
- [ ] `thiserror` pour toutes les erreurs
- [ ] Docstrings `///` sur chaque struct, enum, fn publique
- [ ] `cargo clippy -p apollia-oria` sans warning
- [ ] Commit conventionnel : `feat(apollia-oria): implement Mode Direct with StepBudget enforcement`

## Notes d'implementation

- `StepBudget` utilise `AtomicU32` avec `Ordering::Relaxed` car la precision exacte n'est pas critique — on tolere un depassement d'1 step dans les cas de race condition
- Le polling de `is_exhausted()` dans `execute_direct()` utilise `tokio::time::interval(Duration::from_millis(100))` — suffisant pour le wall_clock timeout
- `from_capped()` est la methode preferee en production ; `new()` est utile pour les tests
- `started_at` est initialise dans `new()` — le chrono demarre a la creation du budget, pas a l'appel de `execute_direct()`
- Les tests du StepBudget sont 100% Rust pur (pas de Python/PyO3 necessaire)

## Liens

- Spec ORIA Engine : `docs/Briques-ORIA-Engine.md`
- Spec Runtime Core : `docs/Briques-Runtime-Core.md`
- Architecture Principes (Principe #7) : `docs/Architecture-Principes.md`
- STORY-026 : `STORIES/sprint-4/story-026-aip-bridge.md`
- STORY-027 : `STORIES/sprint-4/story-027-tool-proxy.md`
- STORY-029 : `STORIES/sprint-4/story-029-observer-context-bundle.md`
