use serde::{Deserialize, Serialize};

use crate::task::AIPPart;

/// Statut d'une tâche AIP.
///
/// Défini ici car requis par `AIPResult`. Les transitions d'état du cycle de vie
/// de l'agent (`ProcessState`) sont définies dans STORY-003 avec la logique
/// de machine à états associée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Tâche en attente de traitement.
    Pending,
    /// Tâche en cours d'exécution.
    Running,
    /// Tâche terminée avec succès.
    Completed,
    /// Tâche échouée.
    Failed,
    /// Tâche annulée.
    Cancelled,
}

/// Budget d'étapes alloué à un agent.
///
/// Défini ici car requis par `AgentManifest`. La logique de validation
/// et les champs complets seront définis dans STORY-004.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBudgetConfig {
    /// Nombre maximum d'étapes autorisées pour une tâche.
    pub max_steps: u32,
}

/// Résultat retourné par l'agent au runtime via le bridge AIP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPResult {
    /// Identifiant de la tâche correspondante.
    pub task_id: String,
    /// Statut final de la tâche.
    pub status: TaskStatus,
    /// Parties de la réponse produite par l'agent.
    pub output: Vec<AIPPart>,
    /// Erreur structurée si `status == Failed`.
    pub error: Option<AIPError>,
    /// Artefacts produits par la tâche (fichiers générés, rapports, etc.).
    pub artifacts: Vec<AIPArtifact>,
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
