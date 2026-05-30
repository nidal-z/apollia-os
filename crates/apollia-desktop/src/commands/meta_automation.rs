//! Tauri IPC command for the natural-language → schedule wizard.
//!
//! Single entry point: `meta_parse_automation` takes a free-form description
//! and the list of installed agents, and returns a `ParsedAutomation` that the
//! UI maps onto the wizard steps.
//!
//! Stateless: delegates to the pure `apollia_llm::meta::parse_automation`
//! function. If a structured LLM is wired up later (a prompt via
//! `MetaLlmOrchestrator`), it can be invoked here ahead of the heuristic
//! fallback without changing the IPC signature.

use apollia_llm::meta::parse_automation::{parse_automation, ParsedAutomation};
use chrono::Utc;
use serde::Deserialize;

/// Body of the IPC request.
#[derive(Debug, Deserialize)]
pub struct ParseAutomationRequest {
    /// Free-form text entered by the operator.
    pub input: String,
    /// Identifiers / names of the agents known on the frontend side.
    #[serde(default)]
    pub known_agents: Vec<String>,
}

/// Parses a natural-language description (fr/en) into an automation.
///
/// Never errors: an empty input simply returns a `ParsedAutomation` with
/// `confidence = Low` and one ambiguity. The UI gates the step on the
/// ambiguities; the command never returns `Err`.
#[tauri::command]
pub fn meta_parse_automation(request: ParseAutomationRequest) -> Result<ParsedAutomation, String> {
    Ok(parse_automation(
        &request.input,
        Utc::now(),
        &request.known_agents,
    ))
}
