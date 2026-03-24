//! Commandes IPC Tauri pour la gestion des tâches.
//!
//! `list_tasks` et `submit_task` délèguent aux handles du runtime.
//! `get_task_timeline` appelle l'API REST interne `GET /api/v1/tasks/{id}/timeline`
//! pour éviter de dupliquer la logique d'agrégation.

use apollia_core::{AIPInput, AIPPart, TaskStatus, TextPart};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

/// Filtre optionnel pour la liste des tâches.
#[derive(Debug, Deserialize)]
pub struct TaskFilter {
    /// Filtrer par statut (submitted, working, completed, failed, etc.).
    pub status: Option<String>,
    /// Filtrer par identifiant agent.
    pub agent_id: Option<String>,
}

/// Résumé d'une tâche pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    /// Identifiant unique de la tâche.
    pub id: String,
    /// Identifiant de l'agent assigné.
    pub agent_id: String,
    /// Nom de l'agent assigné.
    pub agent_name: String,
    /// Statut courant.
    pub status: String,
    /// Aperçu du texte d'entrée (tronqué).
    pub input_preview: String,
    /// Texte de sortie complet (possiblement tronqué par l'observabilité).
    pub output_text: Option<String>,
    /// Durée d'exécution en millisecondes.
    pub duration_ms: Option<u64>,
    /// Date de création ISO8601.
    pub created_at: String,
}

/// Convertit un `TaskStatus` en chaîne snake_case pour le frontend.
fn status_to_string(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{status:?}"))
}

/// Nombre maximum de tâches historiques à charger depuis SQLite.
const PERSISTED_TASK_LIMIT: usize = 50;

/// Liste toutes les tâches avec filtrage optionnel par statut ou agent.
///
/// Fusionne deux sources :
/// 1. **Runtime** (TaskRouter en mémoire) — tâches actives de la session courante.
/// 2. **SQLite** (TaskRepository) — tâches historiques persistées à travers les redémarrages.
///
/// Les tâches runtime sont prioritaires : si une tâche existe dans les deux sources,
/// seule la version runtime est conservée.
#[tauri::command]
pub async fn list_tasks(
    state: State<'_, RuntimeHandle>,
    filter: Option<TaskFilter>,
) -> Result<Vec<TaskSummary>, String> {
    let all = state
        .router_handle
        .all_tasks()
        .await
        .map_err(|e| e.to_string())?;

    // Résoudre le nom de l'agent une seule fois pour filtrer les tâches
    // persistées (qui n'ont pas l'UUID runtime mais ont le nom de l'agent).
    let filter_agent_name: Option<String> = if let Some(ref f) = filter {
        if let Some(ref agent_id) = f.agent_id {
            state
                .registry_handle
                .get_agent(agent_id.as_str())
                .await
                .ok()
                .flatten()
                .map(|e| e.manifest.name.clone())
        } else {
            None
        }
    } else {
        None
    };

    let mut summaries = Vec::with_capacity(all.len());
    let mut seen_task_ids = std::collections::HashSet::new();

    // 1. Runtime tasks (current session, in-memory).
    for (task_id, agent_id, status) in &all {
        let status_str = status_to_string(status);

        if let Some(ref f) = filter {
            if let Some(ref filter_status) = f.status {
                if &status_str != filter_status {
                    continue;
                }
            }
            if let Some(ref filter_agent) = f.agent_id {
                if agent_id.as_str() != filter_agent.as_str() {
                    continue;
                }
            }
        }

        let agent_name = state
            .registry_handle
            .get_agent(agent_id.as_str())
            .await
            .ok()
            .flatten()
            .map(|e| e.manifest.name.clone())
            .unwrap_or_default();

        let (input_preview, output_text, duration_ms, created_at) =
            if let Some(repo) = state.task_repository.as_ref() {
                match repo.get_task_detail(task_id.as_str()).await {
                    Ok(Some(detail)) => {
                        let preview = detail
                            .input_text
                            .as_deref()
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>();
                        let dur = detail.duration_ms.map(|ms| ms as u64);
                        (preview, detail.output_text, dur, detail.created_at)
                    }
                    _ => (String::new(), None, None, String::new()),
                }
            } else {
                (String::new(), None, None, String::new())
            };

        seen_task_ids.insert(task_id.to_string());

        summaries.push(TaskSummary {
            id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            agent_name,
            status: status_str,
            input_preview,
            output_text,
            duration_ms,
            created_at,
        });
    }

    // 2. Persisted tasks from SQLite (historical, survived restart).
    if let Some(repo) = state.task_repository.as_ref() {
        if let Ok(persisted) = repo.list_recent_tasks(PERSISTED_TASK_LIMIT).await {
            for row in persisted {
                if seen_task_ids.contains(&row.task_id) {
                    continue;
                }

                let status_str = &row.status;

                if let Some(ref f) = filter {
                    if let Some(ref filter_status) = f.status {
                        if status_str != filter_status {
                            continue;
                        }
                    }
                    if let Some(ref name) = filter_agent_name {
                        if &row.agent_name != name {
                            continue;
                        }
                    }
                }

                seen_task_ids.insert(row.task_id.clone());

                summaries.push(TaskSummary {
                    id: row.task_id,
                    agent_id: String::new(),
                    agent_name: row.agent_name,
                    status: row.status,
                    input_preview: row.input_preview,
                    output_text: row.output_text,
                    duration_ms: row.duration_ms.map(|ms| ms as u64),
                    created_at: row.created_at,
                });
            }
        }
    }

    Ok(summaries)
}

/// Soumet une tâche à un agent et retourne le `TaskId` généré.
///
/// Construit un `AIPInput` à partir du texte brut fourni par le frontend
/// et le soumet via `TaskRouterHandle::submit()`.
#[tauri::command]
pub async fn submit_task(
    state: State<'_, RuntimeHandle>,
    agent_id: String,
    input: String,
) -> Result<String, String> {
    let aip_input = AIPInput {
        parts: vec![AIPPart::Text(TextPart { text: input })],
    };

    let task_id = state
        .router_handle
        .submit(&agent_id, aip_input)
        .await
        .map_err(|e| e.to_string())?;

    Ok(task_id.to_string())
}

/// Récupère la timeline d'une tâche via l'API REST interne.
///
/// Appelle `GET /api/v1/tasks/{id}/timeline` qui
/// agrège les événements de 5 sources SQLite (transitions, plans, LLM calls,
/// tool calls, HITL). Le résultat est retourné tel quel au frontend.
#[tauri::command]
pub async fn get_task_timeline(
    state: State<'_, RuntimeHandle>,
    task_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let path = format!("/api/v1/tasks/{task_id}/timeline");
    let json = http_get_json(state.api_port, &path).await?;

    match json.get("events").and_then(|v| v.as_array()) {
        Some(events) => Ok(events.clone()),
        None => {
            if let Some(arr) = json.as_array() {
                Ok(arr.clone())
            } else {
                Ok(vec![json])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_to_string_all_variants() {
        // GIVEN all TaskStatus variants
        // WHEN converted to string
        // THEN each produces the expected snake_case representation
        assert_eq!(status_to_string(&TaskStatus::Submitted), "submitted");
        assert_eq!(status_to_string(&TaskStatus::Working), "working");
        assert_eq!(status_to_string(&TaskStatus::Completed), "completed");
        assert_eq!(status_to_string(&TaskStatus::Failed), "failed");
        assert_eq!(
            status_to_string(&TaskStatus::InputRequired),
            "input_required"
        );
        assert_eq!(status_to_string(&TaskStatus::Canceled), "canceled");
    }

    #[test]
    fn test_task_summary_serializes_to_json() {
        // GIVEN a TaskSummary struct
        let summary = TaskSummary {
            id: "task-001".to_string(),
            agent_id: "agent-001".to_string(),
            agent_name: "hello-agent".to_string(),
            status: "completed".to_string(),
            input_preview: "generate report".to_string(),
            output_text: Some("Report generated.".to_string()),
            duration_ms: Some(1200),
            created_at: "2026-03-13T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["id"], "task-001");
        assert_eq!(json["agent_name"], "hello-agent");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["duration_ms"], 1200);
    }

    #[test]
    fn test_task_filter_deserializes() {
        // GIVEN a JSON filter with status only
        let json = serde_json::json!({ "status": "working" });

        // WHEN deserialized
        let filter: TaskFilter = serde_json::from_value(json).expect("deserialize");

        // THEN the status field is populated
        assert_eq!(filter.status.as_deref(), Some("working"));
        assert!(filter.agent_id.is_none());
    }
}
