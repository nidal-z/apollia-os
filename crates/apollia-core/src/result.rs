use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::task::{AIPPart, DataPart, TextPart};

// ─────────────────────────────────────────────
// HITL - Human-in-the-Loop types (Sprint 11)
// ─────────────────────────────────────────────

/// Données portées par [`AIPResult`] quand `status == InputRequired`.
///
/// Persist dans SQLite par le runtime et restituées dans
/// [`InputResponseData::context`] lors de la reprise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRequiredData {
    /// Prompt affiché à l'utilisateur pour prendre sa décision.
    pub prompt: String,
    /// Contexte JSON sérialisé par l'agent au moment de la suspension.
    ///
    /// Le runtime le stocke tel quel dans SQLite et le restitue dans
    /// [`InputResponseData::context`] lors de la reprise.
    pub context: serde_json::Value,
}

/// Réponse humaine reçue après une suspension `input_required`.
///
/// Peuplée par `TaskRepository::rebuild_for_resume()` et injectée
/// dans [`crate::task::AIPTask::input_response`] lors de la reprise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputResponseData {
    /// `true` si l'utilisateur a approuvé, `false` si rejeté.
    pub approved: bool,
    /// Raison transmise à l'agent - `None` si approuvé, potentiellement peuplé si rejeté.
    pub reason: Option<String>,
    /// Contexte JSON sérialisé par l'agent au moment du suspend, restitué intégralement dans [`InputResponseData::context`].
    pub context: serde_json::Value,
    /// Horodatage ISO 8601 de la décision humaine.
    pub responded_at: String,
}

/// Machine d'état d'une tâche individuelle, alignée A2A TaskState.
///
/// Transitions valides :
/// `Submitted` → `Working` → `Completed`
///                    ↓           ↑ (reprise après input)
///                `InputRequired` → `Working`
///                    ↓
///              `Failed` | `Canceled`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Tâche reçue, en attente d'un agent disponible.
    Submitted,
    /// Tâche en cours d'exécution par l'agent.
    Working,
    /// Tâche terminée avec succès.
    Completed,
    /// L'agent a rencontré une erreur non récupérable.
    Failed,
    /// L'agent attend une entrée humaine pour continuer (Human-in-the-Loop).
    InputRequired,
    /// Tâche annulée par l'opérateur ou timeout.
    Canceled,
}

/// Résultat retourné par l'agent au runtime via le bridge AIP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPResult {
    /// Identifiant de la tâche correspondante.
    #[serde(default)]
    pub task_id: String,
    /// Statut final de la tâche.
    pub status: TaskStatus,
    /// Parties de la réponse produite par l'agent.
    #[serde(default)]
    pub output: Vec<AIPPart>,
    /// Erreur structurée si `status == Failed`.
    #[serde(default)]
    pub error: Option<AIPError>,
    /// Artefacts produits par la tâche (fichiers générés, rapports, etc.).
    #[serde(default)]
    pub artifacts: Vec<AIPArtifact>,
    /// Données de la demande d'approbation si `status == InputRequired`.
    ///
    /// Peuplé par [`AIPResult::input_required`].
    /// Persisté dans SQLite par le runtime.
    /// `None` pour tous les autres statuts.
    #[serde(default)]
    pub input_required_data: Option<InputRequiredData>,
}

/// Artefact binaire produit par une tâche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPArtifact {
    /// Nom du fichier ou de l'artefact.
    pub name: String,
    /// Type MIME (ex: "application/pdf", "text/plain").
    pub mime_type: String,
    /// Contenu binaire de l'artefact.
    pub data: Vec<u8>,
}

/// Erreur structurée retournée par l'agent en cas d'échec.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AIPError {
    /// Code d'erreur machine (ex: "TIMEOUT", "TOOL_NOT_FOUND").
    pub code: String,
    /// Message d'erreur lisible par un humain.
    pub message: String,
    /// Détails supplémentaires structurés (optionnel).
    pub details: Option<serde_json::Value>,
}

impl AIPResult {
    /// Construit un résultat demandant une approbation humaine (Human-in-the-Loop).
    ///
    /// Le runtime détecte ce variant via `status == InputRequired`, suspend la tâche,
    /// persiste `prompt` et `context` dans SQLite, puis notifie l'utilisateur
    /// sur les canaux configurés.
    ///
    /// À la reprise, `context` est restitué dans [`InputResponseData::context`]
    /// injecté dans [`crate::task::AIPTask::input_response`].
    pub fn input_required(prompt: &str, context: serde_json::Value) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::InputRequired,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: Some(InputRequiredData {
                prompt: prompt.to_string(),
                context,
            }),
        }
    }

    /// Construit un résultat de succès avec un texte de réponse.
    ///
    /// Raccourci utilisé par `execute_orchestrated` pour la concaténation automatique
    /// des outputs de steps (fallback quand `on_plan_complete()` est absent).
    pub fn completed(text: &str) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::Completed,
            output: vec![AIPPart::Text(TextPart {
                text: text.to_string(),
            })],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }

    /// Construit un résultat d'échec avec un code et un message structurés.
    ///
    /// Raccourci utilisé par `ActorLoop` pour les erreurs de budget, de step ou de plan.
    pub fn failed(code: &str, message: &str) -> Self {
        Self {
            task_id: String::new(),
            status: TaskStatus::Failed,
            output: vec![],
            error: Some(AIPError {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            }),
            artifacts: vec![],
            input_required_data: None,
        }
    }

    /// Construit un résultat de succès avec les outputs de chaque step sérialisés en JSON.
    ///
    /// Utilisé par `ActorLoop` en fin d'exécution orchestrée pour transmettre
    /// les résultats step par step au hook `on_plan_complete()`.
    ///
    /// La `HashMap<step_id → output>` est sérialisée dans `output[0]`
    /// comme `AIPPart::Data`, avec fallback `AIPPart::Text` si la sérialisation échoue.
    pub fn completed_with_steps(steps: HashMap<String, String>) -> Self {
        let part = match serde_json::to_value(&steps) {
            Ok(val) => AIPPart::Data(DataPart { data: val }),
            Err(_) => AIPPart::Text(TextPart {
                text: steps
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            }),
        };
        Self {
            task_id: String::new(),
            status: TaskStatus::Completed,
            output: vec![part],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_result_failed_round_trip() {
        // GIVEN
        let result = AIPResult {
            task_id: "task-123".into(),
            status: TaskStatus::Failed,
            output: vec![],
            error: Some(AIPError {
                code: "TIMEOUT".into(),
                message: "Agent timed out".into(),
                details: None,
            }),
            artifacts: vec![],
            input_required_data: None,
        };
        // WHEN
        let json = serde_json::to_string(&result).expect("serialize failed");
        let restored: AIPResult = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert_eq!(restored.status, TaskStatus::Failed);
        assert!(restored.error.is_some());
        assert_eq!(restored.error.unwrap().code, "TIMEOUT");
    }

    #[test]
    fn test_task_status_serialization() {
        // GIVEN
        let status = TaskStatus::Completed;
        // WHEN
        let json = serde_json::to_string(&status).expect("serialize failed");
        let restored: TaskStatus = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert_eq!(restored, TaskStatus::Completed);
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn test_aip_error_display() {
        // GIVEN
        let err = AIPError {
            code: "TOOL_NOT_FOUND".into(),
            message: "Tool 'bash_executor' not registered".into(),
            details: None,
        };
        // WHEN
        let display = err.to_string();
        // THEN
        assert!(display.contains("TOOL_NOT_FOUND"));
        assert!(display.contains("Tool 'bash_executor' not registered"));
    }
}
