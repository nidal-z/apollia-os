//! ctx.budget — read-only StepBudget view exposed to Python (LOT 4 — ADR-101).
//!
//! Successeur typé du [`crate::context::PyStepBudgetView`] historique. La
//! sémantique est strictement identique (snapshot lecture-seule au moment de
//! l'accès) ; seul le nommage du namespace change : un agent qui suit le
//! Ctx Protocol consomme `ctx.budget.steps_remaining` au lieu de
//! `ctx.step_budget.steps_remaining`.
//!
//! Construit par [`crate::context::RuntimeContext::new_with_llm`] à chaque
//! exécution d'agent en lisant les compteurs live du `StepBudgetView` Rust.

use pyo3::prelude::*;

/// Vue snapshot du budget d'exécution exposée à Python via `ctx.budget`.
///
/// Tous les compteurs sont en `i64` pour permettre la convention `-1`
/// = illimité. `wall_clock_remaining` est `Option<f64>` car certains profils
/// (CLI sans deadline) n'imposent pas de limite wall-clock.
#[pyclass(frozen, name = "BudgetView", module = "apollia._native")]
pub struct BudgetView {
    /// Steps restants avant `max_steps`, ou `-1` si illimité.
    steps_remaining: i64,
    /// Tool calls restants avant `max_tool_calls`, ou `-1` si illimité.
    tool_calls_remaining: i64,
    /// Secondes écoulées depuis le démarrage de la tâche.
    elapsed_seconds: f64,
    /// Secondes restantes avant le deadline wall-clock — `None` si pas de
    /// deadline.
    wall_clock_remaining: Option<f64>,
}

#[pymethods]
impl BudgetView {
    /// Steps restants avant d'atteindre `max_steps` (ReAct).
    ///
    /// Convention : `-1` = illimité, sinon `>= 0` (clamp à 0 si dépassement
    /// transitoire pendant l'incrément concurrent).
    #[getter]
    fn steps_remaining(&self) -> i64 {
        self.steps_remaining
    }

    /// Tool calls restants avant `max_tool_calls`.
    ///
    /// Convention : `-1` = illimité, sinon `>= 0`.
    #[getter]
    fn tool_calls_remaining(&self) -> i64 {
        self.tool_calls_remaining
    }

    /// Secondes écoulées depuis le démarrage de la tâche.
    ///
    /// Toujours `>= 0`. Compteur monotone (basé sur `Instant::now`).
    #[getter]
    fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    /// Secondes restantes avant le deadline wall-clock, ou `None` si aucun
    /// deadline n'est imposé.
    #[getter]
    fn wall_clock_remaining(&self) -> Option<f64> {
        self.wall_clock_remaining
    }
}

impl BudgetView {
    /// Construit une vue depuis les compteurs Rust live.
    ///
    /// Appelé par [`crate::context::RuntimeContext`] au moment où Python
    /// demande `ctx.budget` (le PyObject est créé à la volée à chaque accès
    /// pour rester un snapshot frais).
    pub fn new(
        steps_remaining: i64,
        tool_calls_remaining: i64,
        elapsed_seconds: f64,
        wall_clock_remaining: Option<f64>,
    ) -> Self {
        Self {
            steps_remaining,
            tool_calls_remaining,
            elapsed_seconds,
            wall_clock_remaining,
        }
    }
}
