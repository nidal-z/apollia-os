//! `meta/*` utilities: pure routines (no LLM) exposed by the crate.
//!
//! LLM-backed routines live in `meta_orchestrator.rs`. This module groups the
//! deterministic parsers and heuristics called by the desktop-side Tauri
//! commands.

pub mod next_steps;
pub mod parse_automation;
pub mod rewrite_input;

pub use next_steps::{
    generate_next_steps, heuristic_fallback as next_steps_fallback, NextStep, NextStepAction,
    NextStepButton, NextStepsContext, NextStepsError, NextStepsFacts, NextStepsMode,
    NextStepsRequest, NextStepsResponse,
};
pub use parse_automation::{
    parse_automation, AgentMatch, Confidence, ParsedAutomation, ParsedSchedule,
};
pub use rewrite_input::{
    rewrite_input, RewriteInputError, RewriteInputRequest, RewriteInputResponse, WorkContext,
};
