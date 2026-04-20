//! Utilitaires `meta/*` — routines pures (sans LLM) exposées par la crate.
//!
//! Les routines LLM-backed vivent dans `meta_orchestrator.rs`. Ce module
//! regroupe les parseurs et heuristiques déterministes appelés par les
//! commandes Tauri côté desktop (US-SP42-050).

pub mod apollia_coach;
pub mod parse_automation;

pub use apollia_coach::{
    invoke_apollia_coach, ActionButton, ApolliaCoachError, CoachAction, CoachContext, CoachMode,
    CoachResponse, CoachTurn,
};
pub use parse_automation::{
    parse_automation, AgentMatch, Confidence, ParsedAutomation, ParsedSchedule,
};
