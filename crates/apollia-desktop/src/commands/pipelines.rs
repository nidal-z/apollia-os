//! Commandes IPC Tauri pour la gestion des pipelines.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/pipelines/*`) via
//! les helpers `http_get_json` / `http_post_json`. Les données transitent
//! en JSON brut (`serde_json::Value`) pour éviter de dupliquer les types
//! Rust déjà définis dans `apollia-pipelines`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_post_json, http_put_json};

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

// ─── CRUD types & commands ──────────────────────────────────────────────────

/// Définition complète d'un pipeline retournée par les opérations CRUD.
#[derive(Debug, Serialize)]
pub struct PipelineDefinitionView {
    /// Identifiant du pipeline.
    pub id: String,
    /// Description du pipeline.
    pub description: String,
    /// Politique de gestion d'erreur globale : `"fail"` ou `"continue"`.
    pub on_failure: String,
    /// Liste ordonnée des steps.
    pub steps: Vec<PipelineStepView>,
    /// Indique si le pipeline est actif.
    pub enabled: bool,
    /// Horodatage de création (ISO 8601).
    pub created_at: String,
    /// Horodatage de dernière modification (ISO 8601).
    pub updated_at: String,
}

/// Step d'un pipeline dans les réponses CRUD.
#[derive(Debug, Serialize)]
pub struct PipelineStepView {
    /// Identifiant du step.
    pub id: String,
    /// Agent cible.
    pub agent: String,
    /// Template d'entrée.
    pub input: String,
    /// Dépendances sur d'autres steps.
    pub depends_on: Vec<String>,
    /// Politique d'erreur du step.
    pub on_failure: String,
    /// Condition d'exécution optionnelle.
    pub condition: Option<StepConditionView>,
    /// Step auquel celui-ci sert de fallback.
    pub fallback_for: Option<String>,
}

/// Condition d'exécution d'un step.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StepConditionView {
    /// Opérateur de comparaison.
    pub when: String,
    /// Chemin vers la valeur à inspecter (dot-path).
    pub field: String,
    /// Valeur de référence.
    pub value: String,
}

/// Step dans les requêtes de création/mise à jour de pipeline.
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStepInput {
    /// Identifiant du step.
    pub id: String,
    /// Agent cible.
    pub agent: String,
    /// Template d'entrée.
    pub input: String,
    /// Dépendances sur d'autres steps.
    pub depends_on: Option<Vec<String>>,
    /// Politique d'erreur du step.
    pub on_failure: Option<String>,
    /// Condition d'exécution optionnelle.
    pub condition: Option<StepConditionView>,
    /// Step auquel celui-ci sert de fallback.
    pub fallback_for: Option<String>,
}

/// Corps de requête pour la création d'un pipeline via `create_pipeline`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePipelineRequest {
    /// Identifiant unique du pipeline.
    pub id: String,
    /// Description du pipeline.
    pub description: Option<String>,
    /// Politique de gestion d'erreur globale.
    pub on_failure: Option<String>,
    /// Indique si le pipeline est actif (défaut : `true`).
    pub enabled: Option<bool>,
    /// Liste ordonnée des steps.
    pub steps: Vec<PipelineStepInput>,
}

/// Corps de requête pour la mise à jour d'un pipeline via `update_pipeline`.
///
/// Même structure que `CreatePipelineRequest` — l'ID est passé dans l'URL.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatePipelineRequest {
    /// Identifiant du pipeline (requis par l'API PUT, même si dans l'URL).
    pub id: String,
    /// Description du pipeline.
    pub description: Option<String>,
    /// Politique de gestion d'erreur globale.
    pub on_failure: Option<String>,
    /// Indique si le pipeline est actif.
    pub enabled: Option<bool>,
    /// Liste ordonnée des steps.
    pub steps: Vec<PipelineStepInput>,
}

/// Parse un step JSON en `PipelineStepView`.
fn parse_step(s: &serde_json::Value) -> PipelineStepView {
    let condition = s.get("condition").and_then(|c| {
        if c.is_null() {
            return None;
        }
        Some(StepConditionView {
            when: c
                .get("when")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            field: c
                .get("field")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            value: c
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    });

    PipelineStepView {
        id: s
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        agent: s
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        input: s
            .get("input")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        depends_on: s
            .get("depends_on")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        on_failure: s
            .get("on_failure")
            .and_then(|v| v.as_str())
            .unwrap_or("fail")
            .to_string(),
        condition,
        fallback_for: s
            .get("fallback_for")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Parse un JSON de réponse API en `PipelineDefinitionView`.
fn parse_pipeline_definition(json: &serde_json::Value) -> PipelineDefinitionView {
    let steps = json
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_step).collect())
        .unwrap_or_default();

    PipelineDefinitionView {
        id: json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        on_failure: json
            .get("on_failure")
            .and_then(|v| v.as_str())
            .unwrap_or("fail")
            .to_string(),
        steps,
        enabled: json
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        created_at: json
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        updated_at: json
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Crée un nouveau pipeline.
///
/// Délègue à `POST /api/v1/pipelines`.
#[tauri::command]
pub async fn create_pipeline(
    state: State<'_, RuntimeHandle>,
    definition: CreatePipelineRequest,
) -> Result<PipelineDefinitionView, String> {
    let body = serde_json::to_value(&definition)
        .map_err(|e| format!("failed to serialize request: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/pipelines", &body).await?;
    Ok(parse_pipeline_definition(&json))
}

/// Met à jour un pipeline existant.
///
/// Délègue à `PUT /api/v1/pipelines/:id`.
#[tauri::command]
pub async fn update_pipeline(
    state: State<'_, RuntimeHandle>,
    id: String,
    definition: UpdatePipelineRequest,
) -> Result<PipelineDefinitionView, String> {
    let body = serde_json::to_value(&definition)
        .map_err(|e| format!("failed to serialize request: {e}"))?;
    let path = format!("/api/v1/pipelines/{id}");
    let json = http_put_json(state.api_port, &path, &body).await?;
    Ok(parse_pipeline_definition(&json))
}

/// Supprime un pipeline.
///
/// Délègue à `DELETE /api/v1/pipelines/:id`.
#[tauri::command]
pub async fn delete_pipeline(state: State<'_, RuntimeHandle>, id: String) -> Result<(), String> {
    let path = format!("/api/v1/pipelines/{id}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}

/// Liste toutes les définitions de pipelines avec leurs steps.
///
/// Délègue à `GET /api/v1/pipelines` qui retourne les définitions complètes.
#[tauri::command]
pub async fn list_pipeline_definitions(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<PipelineDefinitionView>, String> {
    let json = http_get_json(state.api_port, "/api/v1/pipelines").await?;

    let items = match json {
        serde_json::Value::Array(arr) => arr,
        _ => json
            .get("pipelines")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
    };

    Ok(items.iter().map(parse_pipeline_definition).collect())
}

/// Récupère la définition complète d'un pipeline par son identifiant.
///
/// Délègue à `GET /api/v1/pipelines/:id`.
#[tauri::command]
pub async fn get_pipeline_definition(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<PipelineDefinitionView, String> {
    let path = format!("/api/v1/pipelines/{id}");
    let json = http_get_json(state.api_port, &path).await?;
    Ok(parse_pipeline_definition(&json))
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

    #[test]
    fn test_pipeline_definition_view_serializes() {
        // GIVEN a PipelineDefinitionView with steps
        let view = PipelineDefinitionView {
            id: "devis-pipeline".to_string(),
            description: "Generate devis from specs".to_string(),
            on_failure: "fail".to_string(),
            steps: vec![
                PipelineStepView {
                    id: "extract".to_string(),
                    agent: "extractor".to_string(),
                    input: "Extract specs from {{trigger.payload}}".to_string(),
                    depends_on: vec![],
                    on_failure: "fail".to_string(),
                    condition: None,
                    fallback_for: None,
                },
                PipelineStepView {
                    id: "generate".to_string(),
                    agent: "devis-agent".to_string(),
                    input: "{{steps.extract.output}}".to_string(),
                    depends_on: vec!["extract".to_string()],
                    on_failure: "fallback".to_string(),
                    condition: Some(StepConditionView {
                        when: "contains".to_string(),
                        field: "steps.extract.output".to_string(),
                        value: "specs_found".to_string(),
                    }),
                    fallback_for: None,
                },
            ],
            enabled: true,
            created_at: "2026-03-20T10:00:00Z".to_string(),
            updated_at: "2026-03-20T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["id"], "devis-pipeline");
        assert_eq!(json["on_failure"], "fail");
        assert_eq!(json["enabled"], true);
        let steps = json["steps"].as_array().expect("array");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["id"], "extract");
        assert_eq!(steps[1]["depends_on"][0], "extract");
        assert_eq!(steps[1]["condition"]["when"], "contains");
    }

    #[test]
    fn test_parse_pipeline_definition_from_api_response() {
        // GIVEN a JSON response matching API PipelineDefinitionResponse
        let api_json = serde_json::json!({
            "id": "p-test",
            "description": "Test pipeline",
            "on_failure": "continue",
            "steps": [
                {
                    "id": "step1",
                    "agent": "agent-a",
                    "input": "hello",
                    "depends_on": [],
                    "on_failure": "skip",
                    "condition": null,
                    "fallback_for": null
                }
            ],
            "enabled": true,
            "created_at": "2026-03-20T10:00:00Z",
            "updated_at": "2026-03-20T11:00:00Z"
        });

        // WHEN parsed
        let view = parse_pipeline_definition(&api_json);

        // THEN all fields are correctly mapped
        assert_eq!(view.id, "p-test");
        assert_eq!(view.description, "Test pipeline");
        assert_eq!(view.on_failure, "continue");
        assert!(view.enabled);
        assert_eq!(view.steps.len(), 1);
        assert_eq!(view.steps[0].id, "step1");
        assert_eq!(view.steps[0].on_failure, "skip");
        assert!(view.steps[0].condition.is_none());
    }

    #[test]
    fn test_create_pipeline_request_serializes() {
        // GIVEN a CreatePipelineRequest
        let req = CreatePipelineRequest {
            id: "new-pipe".to_string(),
            description: Some("A new pipeline".to_string()),
            on_failure: None,
            enabled: Some(true),
            steps: vec![PipelineStepInput {
                id: "s1".to_string(),
                agent: "agent-x".to_string(),
                input: "do something".to_string(),
                depends_on: None,
                on_failure: None,
                condition: None,
                fallback_for: None,
            }],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&req).expect("serialize");

        // THEN required fields are present
        assert_eq!(json["id"], "new-pipe");
        assert_eq!(json["steps"][0]["agent"], "agent-x");
    }
}
