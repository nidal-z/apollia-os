//! Utilitaires `meta/*` — routines pures (sans LLM) exposées par la crate.
//!
//! Les routines LLM-backed vivent dans `meta_orchestrator.rs`. Ce module
//! regroupe les parseurs et heuristiques déterministes appelés par les
//! commandes Tauri côté desktop (US-SP42-050).

pub mod parse_automation;

pub use parse_automation::{
    parse_automation, AgentMatch, Confidence, ParsedAutomation, ParsedSchedule,
};
