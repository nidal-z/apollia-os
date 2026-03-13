//! Commandes IPC Tauri pour la gestion des pipelines.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/pipelines/*`) via
//! les helpers `http_get_json` / `http_post_json`. Les données transitent
//! en JSON brut (`serde_json::Value`) pour éviter de dupliquer les types
//! Rust déjà définis dans `apollia-pipelines`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::{http_get_json, http_post_json};

/// Résumé d'un pipeline run pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct PipelineRunSummary {
    /// Identifiant unique du run.
    pub run_id: String,
    /// Identifiant du pipeline.
    pub pipeline_id: String,
    /// Statut agrégé : `"running"` | `"waiting_approval"` | `"completed"` | `"failed"`.
    pub status: String,
    /// Horodatage de création (RFC3339) ou `null`.
    pub started_at: Option<String>,
    /// Horodatage de fin (RFC3339) ou `null` si en cours.
    pub ended_at: Option<String>,
}

/// Step d'un pipeline run avec son statut détaillé.
#[derive(Debug, Serialize)]
pub struct PipelineStepSummary {
    /// Identifiant du step.
    pub step_id: String,
    /// Statut : `"pending"` | `"running"` | `"completed"` | `"failed"`.
    pub status: String,
    /// Sortie du step (si complété).
    pub output: Option<String>,
    /// Message d'erreur (si échoué).
    pub error: Option<String>,
    /// Horodatage de début (RFC3339).
    pub started_at: Option<String>,
    /// Horodatage de fin (RFC3339).
    pub ended_at: Option<String>,
}

/// Détail complet d'un pipeline run avec ses steps.
#[derive(Debug, Serialize)]
pub struct PipelineRunDetail {
    /// Identifiant unique du run.
    pub run_id: String,
    /// Identifiant du pipeline.
    pub pipeline_id: String,
    /// Statut agrégé.
    pub status: String,
    /// Liste des steps dans l'ordre.
    pub step_runs: Vec<PipelineStepSummary>,
    /// Horodatage de création (RFC3339).
    pub started_at: String,
    /// Horodatage de fin (RFC3339) ou `null`.
    pub ended_at: Option<String>,
}

/// Pipeline disponible pour lancement.
#[derive(Debug, Serialize)]
pub struct PipelineInfo {
    /// Identifiant du pipeline.
    pub id: String,
    /// Description du pipeline.
    pub description: String,
}

/// Résultat du lancement d'un pipeline run.
#[derive(Debug, Serialize)]
pub struct RunPipelineResult {
    /// Identifiant du run créé.
    pub run_id: String,
    /// Identifiant du pipeline.
    pub pipeline_id: String,
    /// Statut initial (`"running"`).
    pub status: String,
}

/// Liste les pipelines disponibles.
///
/// Délègue à `GET /api/v1/pipelines`.
#[tauri::command]
pub async fn list_pipelines(state: State<'_, RuntimeHandle>) -> Result<Vec<PipelineInfo>, String> {
    let json = http_get_json(state.api_port, "/api/v1/pipelines").await?;

    let pipelines = json
        .get("pipelines")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = pipelines
        .into_iter()
        .map(|p| PipelineInfo {
            id: p
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: p
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    Ok(result)
}

/// Liste les pipeline runs récents pour un pipeline donné.
///
/// Délègue à `GET /api/v1/pipelines/:id/runs`.
#[tauri::command]
pub async fn list_pipeline_runs(
    state: State<'_, RuntimeHandle>,
    pipeline_id: String,
) -> Result<Vec<PipelineRunSummary>, String> {
    let path = format!("/api/v1/pipelines/{pipeline_id}/runs");
    let json = http_get_json(state.api_port, &path).await?;

    let runs = json.as_array().cloned().unwrap_or_default();

    let result = runs
        .into_iter()
        .map(|r| PipelineRunSummary {
            run_id: r
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            pipeline_id: r
                .get("pipeline_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: r
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            started_at: r
                .get("started_at")
                .and_then(|v| v.as_str())
                .map(String::from),
            ended_at: r.get("ended_at").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(result)
}

/// Liste les runs de tous les pipelines.
///
/// Charge les pipelines disponibles puis agrège les runs de chacun.
#[tauri::command]
pub async fn list_all_pipeline_runs(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<PipelineRunSummary>, String> {
    let pipelines_json = http_get_json(state.api_port, "/api/v1/pipelines").await?;
    let pipelines = pipelines_json
        .get("pipelines")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let max = limit.unwrap_or(50) as usize;
    let mut all_runs: Vec<PipelineRunSummary> = Vec::new();

    for p in &pipelines {
        let pid = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if pid.is_empty() {
            continue;
        }
        let path = format!("/api/v1/pipelines/{pid}/runs");
        if let Ok(json) = http_get_json(state.api_port, &path).await {
            let runs = json.as_array().cloned().unwrap_or_default();
            for r in runs {
                all_runs.push(PipelineRunSummary {
                    run_id: r
                        .get("run_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pipeline_id: r
                        .get("pipeline_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: r
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    started_at: r
                        .get("started_at")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    ended_at: r.get("ended_at").and_then(|v| v.as_str()).map(String::from),
                });
            }
        }
    }

    // Sort by started_at descending (most recent first).
    all_runs.sort_by(|a, b| {
        let a_time = a.started_at.as_deref().unwrap_or("");
        let b_time = b.started_at.as_deref().unwrap_or("");
        b_time.cmp(a_time)
    });
    all_runs.truncate(max);

    Ok(all_runs)
}

/// Récupère le détail d'un pipeline run avec ses steps.
///
/// Délègue à `GET /api/v1/runs/:run_id`.
#[tauri::command]
pub async fn get_pipeline_run_detail(
    state: State<'_, RuntimeHandle>,
    run_id: String,
) -> Result<PipelineRunDetail, String> {
    let path = format!("/api/v1/runs/{run_id}");
    let json = http_get_json(state.api_port, &path).await?;

    let step_runs = json
        .get("step_runs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let steps = step_runs
        .into_iter()
        .map(|s| PipelineStepSummary {
            step_id: s
                .get("step_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: s
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            output: s.get("output").and_then(|v| v.as_str()).map(String::from),
            error: s.get("error").and_then(|v| v.as_str()).map(String::from),
            started_at: s
                .get("started_at")
                .and_then(|v| v.as_str())
                .map(String::from),
            ended_at: s.get("ended_at").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(PipelineRunDetail {
        run_id: json
            .get("run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        pipeline_id: json
            .get("pipeline_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        status: json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        step_runs: steps,
        started_at: json
            .get("started_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        ended_at: json
            .get("ended_at")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Lance un nouveau pipeline run.
///
/// Délègue à `POST /api/v1/pipelines/:id/run`.
#[tauri::command]
pub async fn run_pipeline(
    state: State<'_, RuntimeHandle>,
    pipeline_id: String,
    input: Option<String>,
) -> Result<RunPipelineResult, String> {
    let body = serde_json::json!({ "input": input });
    let path = format!("/api/v1/pipelines/{pipeline_id}/run");
    let json = http_post_json(state.api_port, &path, &body).await?;

    Ok(RunPipelineResult {
        run_id: json
            .get("run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        pipeline_id: json
            .get("pipeline_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        status: json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("running")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_run_summary_serializes() {
        // GIVEN a PipelineRunSummary
        let summary = PipelineRunSummary {
            run_id: "r-abc123".to_string(),
            pipeline_id: "devis-pipeline".to_string(),
            status: "running".to_string(),
            started_at: Some("2026-03-13T10:00:00Z".to_string()),
            ended_at: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["run_id"], "r-abc123");
        assert_eq!(json["pipeline_id"], "devis-pipeline");
        assert_eq!(json["status"], "running");
        assert_eq!(json["started_at"], "2026-03-13T10:00:00Z");
        assert!(json["ended_at"].is_null());
    }

    #[test]
    fn test_pipeline_run_summary_serializes_completed() {
        // GIVEN a completed PipelineRunSummary
        let summary = PipelineRunSummary {
            run_id: "r-def456".to_string(),
            pipeline_id: "report-pipeline".to_string(),
            status: "completed".to_string(),
            started_at: Some("2026-03-13T08:00:00Z".to_string()),
            ended_at: Some("2026-03-13T08:05:00Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN ended_at is set
        assert_eq!(json["status"], "completed");
        assert_eq!(json["ended_at"], "2026-03-13T08:05:00Z");
    }

    #[test]
    fn test_pipeline_step_summary_serializes() {
        // GIVEN a PipelineStepSummary
        let step = PipelineStepSummary {
            step_id: "step-extract".to_string(),
            status: "completed".to_string(),
            output: Some("extracted data".to_string()),
            error: None,
            started_at: Some("2026-03-13T10:00:01Z".to_string()),
            ended_at: Some("2026-03-13T10:00:05Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&step).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["step_id"], "step-extract");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["output"], "extracted data");
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_pipeline_step_summary_serializes_failed() {
        // GIVEN a failed step
        let step = PipelineStepSummary {
            step_id: "step-validate".to_string(),
            status: "failed".to_string(),
            output: None,
            error: Some("validation timeout".to_string()),
            started_at: Some("2026-03-13T10:00:06Z".to_string()),
            ended_at: Some("2026-03-13T10:01:06Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&step).expect("serialize");

        // THEN error is set and output is null
        assert_eq!(json["status"], "failed");
        assert!(json["output"].is_null());
        assert_eq!(json["error"], "validation timeout");
    }

    #[test]
    fn test_pipeline_run_detail_serializes() {
        // GIVEN a PipelineRunDetail with steps
        let detail = PipelineRunDetail {
            run_id: "r-xyz789".to_string(),
            pipeline_id: "ingestion".to_string(),
            status: "running".to_string(),
            step_runs: vec![
                PipelineStepSummary {
                    step_id: "step-a".to_string(),
                    status: "completed".to_string(),
                    output: Some("done".to_string()),
                    error: None,
                    started_at: Some("2026-03-13T10:00:00Z".to_string()),
                    ended_at: Some("2026-03-13T10:00:02Z".to_string()),
                },
                PipelineStepSummary {
                    step_id: "step-b".to_string(),
                    status: "running".to_string(),
                    output: None,
                    error: None,
                    started_at: Some("2026-03-13T10:00:03Z".to_string()),
                    ended_at: None,
                },
            ],
            started_at: "2026-03-13T10:00:00Z".to_string(),
            ended_at: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&detail).expect("serialize");

        // THEN step_runs array has 2 elements
        let steps = json["step_runs"].as_array().expect("array");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["step_id"], "step-a");
        assert_eq!(steps[1]["status"], "running");
    }

    #[test]
    fn test_pipeline_info_serializes() {
        // GIVEN a PipelineInfo
        let info = PipelineInfo {
            id: "devis-pipeline".to_string(),
            description: "Generate devis from input".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&info).expect("serialize");

        // THEN fields are correct
        assert_eq!(json["id"], "devis-pipeline");
        assert_eq!(json["description"], "Generate devis from input");
    }

    #[test]
    fn test_run_pipeline_result_serializes() {
        // GIVEN a RunPipelineResult
        let result = RunPipelineResult {
            run_id: "r-new123".to_string(),
            pipeline_id: "test-pipe".to_string(),
            status: "running".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["run_id"], "r-new123");
        assert_eq!(json["pipeline_id"], "test-pipe");
        assert_eq!(json["status"], "running");
    }
}
