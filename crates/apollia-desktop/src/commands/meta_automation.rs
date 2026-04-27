//! Commande IPC Tauri pour le wizard natural-language → schedule.
//!
//! Point unique : `meta_parse_automation` — prend une description libre et la
//! liste des agents installés, retourne une `ParsedAutomation` que l'UI mappe
//! aux étapes du wizard.
//!
//! Zéro état : délègue à la fonction pure `apollia_llm::meta::parse_automation`.
//! Si demain un LLM structuré est câblé (prompt Haiku 4.5 via `MetaLlmOrchestrator`),
//! il pourra être invoqué ici en amont du fallback heuristique sans changer la
//! signature IPC.

use apollia_llm::meta::parse_automation::{parse_automation, ParsedAutomation};
use chrono::Utc;
use serde::Deserialize;

/// Corps de la requête IPC.
#[derive(Debug, Deserialize)]
pub struct ParseAutomationRequest {
    /// Texte libre entré par l'opérateur.
    pub input: String,
    /// Identifiants / noms des agents connus côté frontend.
    #[serde(default)]
    pub known_agents: Vec<String>,
}

/// Parse une description en langage naturel (fr/en) vers une automatisation.
///
/// Jamais d'erreur : un input vide retourne simplement un `ParsedAutomation`
/// avec `confidence = Low` et une ambiguïté. L'UI gate le step sur les
/// ambiguïtés — la commande ne renvoie jamais `Err`.
#[tauri::command]
pub fn meta_parse_automation(request: ParseAutomationRequest) -> Result<ParsedAutomation, String> {
    Ok(parse_automation(
        &request.input,
        Utc::now(),
        &request.known_agents,
    ))
}
