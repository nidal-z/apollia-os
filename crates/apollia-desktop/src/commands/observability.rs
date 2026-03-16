//! Commandes IPC Tauri pour la vue Observabilité (STORY-148).
//!
//! Trois commandes couvrant les 3 tabs de la vue :
//! - `get_global_timeline` — événements runtime agrégés multi-tâches
//! - `get_tool_audit_trail` — invocations d'outils avec détails
//! - `get_llm_daily_costs` — coûts LLM ventilés par jour et backend

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

// ---------------------------------------------------------------------------
// Global Timeline (AC-1, AC-2)
// ---------------------------------------------------------------------------

/// Événement de la timeline globale pour l'affichage.
#[derive(Debug, Serialize)]
pub struct GlobalTimelineEvent {
    /// Type d'événement : task, tool, llm, trigger, hitl.
    pub event_type: String,
    /// Horodatage ISO 8601.
    pub timestamp: String,
    /// Résumé de l'événement.
    pub summary: String,
    /// Détails JSON expandables.
    pub detail: serde_json::Value,
}

/// Paramètres pour `get_global_timeline`.
#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    /// Fenêtre temporelle en minutes (30, 60, 360, 720, 1440).
    pub window_minutes: u32,
}

/// Récupère une timeline globale multi-tâches depuis les 5 sources.
///
/// Agrège les événements de toutes les tâches connues dans la fenêtre
/// temporelle demandée. Délègue à l'API REST interne pour récupérer
/// la liste des tâches, puis leurs timelines individuelles.
#[tauri::command]
pub async fn get_global_timeline(
    state: State<'_, RuntimeHandle>,
    params: TimelineParams,
) -> Result<Vec<GlobalTimelineEvent>, String> {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(i64::from(params.window_minutes));
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Fetch all tasks from the router to get their IDs.
    let tasks = state
        .router_handle
        .all_tasks()
        .await
        .map_err(|e| e.to_string())?;

    let mut all_events: Vec<GlobalTimelineEvent> = Vec::new();

    for (task_id, agent_id, _status) in &tasks {
        let path = format!("/api/v1/tasks/{task_id}/timeline");
        let json = match http_get_json(state.api_port, &path).await {
            Ok(j) => j,
            Err(_) => continue,
        };

        // Resolve human-readable agent name once per task to avoid repeated
        // registry lookups in the inner event loop.
        let agent_label = state
            .registry_handle
            .get_agent(agent_id.as_str())
            .await
            .ok()
            .flatten()
            .map(|e| e.manifest.name.clone())
            .unwrap_or_else(|| agent_id.to_string());

        let events = json
            .get("events")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for event in events {
            let timestamp = event
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if timestamp < cutoff_str {
                continue;
            }

            let event_type_raw = event
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let event_type = classify_event_type(event_type_raw);
            let summary = build_event_summary(event_type_raw, &event, &agent_label);

            all_events.push(GlobalTimelineEvent {
                event_type,
                timestamp,
                summary,
                detail: event,
            });
        }
    }

    // Sort by timestamp DESC (most recent first).
    all_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(all_events)
}

/// Classifie un type d'événement brut en catégorie pour le filtrage UI.
fn classify_event_type(raw: &str) -> String {
    match raw {
        "task_transition" | "task_completed" => "task".to_string(),
        "tool_call" => "tool".to_string(),
        "llm_call" => "llm".to_string(),
        "step_started" | "step_completed" => "task".to_string(),
        "hitl_suspended" | "hitl_resolved" => "hitl".to_string(),
        other => other.to_string(),
    }
}

/// Construit un résumé lisible à partir d'un événement timeline brut.
fn build_event_summary(event_type: &str, event: &serde_json::Value, agent_id: &str) -> String {
    match event_type {
        "task_transition" => {
            let status = event
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("[{agent_id}] Task → {status}")
        }
        "task_completed" => {
            let dur = event
                .get("duration_ms")
                .and_then(|v| v.as_i64())
                .map(|ms| format!(" in {ms}ms"))
                .unwrap_or_default();
            format!("[{agent_id}] Task completed{dur}")
        }
        "tool_call" => {
            let tool = event
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let dur = event
                .get("duration_ms")
                .and_then(|v| v.as_i64())
                .map(|ms| format!(" ({ms}ms)"))
                .unwrap_or_default();
            format!("[{agent_id}] Tool: {tool}{dur}")
        }
        "llm_call" => {
            let model = event
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let cost = event
                .get("cost_usd")
                .and_then(|v| v.as_f64())
                .map(|c| format!(" ${c:.4}"))
                .unwrap_or_default();
            format!("[{agent_id}] LLM: {model}{cost}")
        }
        "step_started" => {
            let step_id = event.get("step_id").and_then(|v| v.as_str()).unwrap_or("?");
            let tool = event
                .get("tool")
                .and_then(|v| v.as_str())
                .map(|t| format!(" — {t}"))
                .unwrap_or_default();
            format!("[{agent_id}] Step {step_id} started{tool}")
        }
        "step_completed" => {
            let step_id = event.get("step_id").and_then(|v| v.as_str()).unwrap_or("?");
            let success = event
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let icon = if success { "completed" } else { "failed" };
            format!("[{agent_id}] Step {step_id} {icon}")
        }
        "hitl_suspended" => {
            let prompt = event.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let preview = if prompt.len() > 60 {
                format!("{}...", &prompt[..60])
            } else {
                prompt.to_string()
            };
            format!("[{agent_id}] HITL: {preview}")
        }
        "hitl_resolved" => {
            let approved = event
                .get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let verdict = if approved { "Approved" } else { "Rejected" };
            format!("[{agent_id}] HITL: {verdict}")
        }
        _ => format!("[{agent_id}] {event_type}"),
    }
}

// ---------------------------------------------------------------------------
// Audit Trail (AC-4)
// ---------------------------------------------------------------------------

/// Entrée de l'audit trail pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct AuditTrailEntry {
    /// Identifiant unique de l'invocation.
    pub id: String,
    /// Nom de l'outil invoqué.
    pub tool_name: String,
    /// Identifiant UUID de l'agent (utilisé pour le filtrage).
    pub agent_id: String,
    /// Nom lisible de l'agent, résolu depuis le registre (ex: "standup-scribe").
    /// Retombe sur agent_id si l'agent n'est plus enregistré.
    pub agent_name: String,
    /// Horodatage ISO 8601.
    pub timestamp: String,
    /// Durée d'exécution en millisecondes.
    pub duration_ms: Option<u64>,
    /// Code de sortie du processus.
    pub exit_code: Option<i32>,
    /// Arguments JSON complets de l'invocation.
    pub args_json: Option<String>,
    /// Sortie standard de l'outil.
    pub stdout: Option<String>,
    /// Sortie d'erreur de l'outil.
    pub stderr: Option<String>,
}

/// Récupère les dernières invocations d'outils via l'API REST audit.
///
/// Délègue à `GET /api/v1/audit?limit=N` et retourne les entrées parsées
/// pour l'affichage dans la table AuditTrail.
#[tauri::command]
pub async fn get_tool_audit_trail(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<AuditTrailEntry>, String> {
    let l = limit.unwrap_or(50);
    let path = format!("/api/v1/audit?limit={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Resolve agent names asynchronously from the registry so the UI shows
    // "standup-scribe" instead of a raw UUID. Falls back to the UUID when the
    // agent is no longer registered (e.g. stopped between runs).
    let mut entries: Vec<AuditTrailEntry> = Vec::with_capacity(events.len());
    for e in events {
        let agent_id = e
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let agent_name = state
            .registry_handle
            .get_agent(&agent_id)
            .await
            .ok()
            .flatten()
            .map(|entry| entry.manifest.name.clone())
            .unwrap_or_else(|| agent_id.clone()); // agent_id is already String here

        entries.push(AuditTrailEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tool_name: e
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent_id,
            agent_name,
            timestamp: e
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            duration_ms: e.get("duration_ms").and_then(|v| v.as_u64()),
            exit_code: e
                .get("exit_code")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            args_json: e
                .get("args_json")
                .and_then(|v| v.as_str())
                .map(String::from),
            stdout: e.get("stdout").and_then(|v| v.as_str()).map(String::from),
            stderr: e.get("stderr").and_then(|v| v.as_str()).map(String::from),
        });
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// LLM Daily Costs (AC-3)
// ---------------------------------------------------------------------------

/// Entrée coût journalier par backend pour le graphique SVG.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostEntry {
    /// Date au format `YYYY-MM-DD`.
    pub date: String,
    /// Nom du backend.
    pub backend: String,
    /// Coût total estimé en USD pour ce jour.
    pub cost_usd: f64,
}

/// Réponse des coûts journaliers LLM.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostsResponse {
    /// Entrées par jour et backend.
    pub entries: Vec<LlmDailyCostEntry>,
    /// Nombre de jours demandés.
    pub days: u32,
}

/// Récupère les coûts LLM ventilés par jour et backend.
///
/// Délègue à `GET /api/v1/llm/costs/daily?days=N`.
#[tauri::command]
pub async fn get_llm_daily_costs(
    state: State<'_, RuntimeHandle>,
    days: Option<u32>,
) -> Result<LlmDailyCostsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs/daily?days={d}");
    let json = http_get_json(state.api_port, &path).await;

    match json {
        Ok(resp) => {
            let entries = resp
                .get("entries")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|e| LlmDailyCostEntry {
                    date: e
                        .get("date")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    backend: e
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    cost_usd: e.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
                .collect();

            Ok(LlmDailyCostsResponse { entries, days: d })
        }
        Err(_) => Ok(LlmDailyCostsResponse {
            entries: vec![],
            days: d,
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_event_type_task() {
        // GIVEN task-related event types
        // WHEN classified
        // THEN they all map to "task"
        assert_eq!(classify_event_type("task_transition"), "task");
        assert_eq!(classify_event_type("task_completed"), "task");
        assert_eq!(classify_event_type("step_started"), "task");
        assert_eq!(classify_event_type("step_completed"), "task");
    }

    #[test]
    fn test_classify_event_type_tool() {
        // GIVEN a tool_call event type
        // WHEN classified
        // THEN it maps to "tool"
        assert_eq!(classify_event_type("tool_call"), "tool");
    }

    #[test]
    fn test_classify_event_type_llm() {
        // GIVEN an llm_call event type
        // WHEN classified
        // THEN it maps to "llm"
        assert_eq!(classify_event_type("llm_call"), "llm");
    }

    #[test]
    fn test_classify_event_type_hitl() {
        // GIVEN HITL event types
        // WHEN classified
        // THEN they map to "hitl"
        assert_eq!(classify_event_type("hitl_suspended"), "hitl");
        assert_eq!(classify_event_type("hitl_resolved"), "hitl");
    }

    #[test]
    fn test_classify_event_type_unknown() {
        // GIVEN an unknown event type
        // WHEN classified
        // THEN it passes through unchanged
        assert_eq!(classify_event_type("custom_event"), "custom_event");
    }

    #[test]
    fn test_build_event_summary_task_transition() {
        // GIVEN a task_transition event JSON
        let event = serde_json::json!({
            "type": "task_transition",
            "status": "working",
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("task_transition", &event, "agent-1");

        // THEN it includes the agent and status
        assert_eq!(summary, "[agent-1] Task → working");
    }

    #[test]
    fn test_build_event_summary_tool_call() {
        // GIVEN a tool_call event JSON
        let event = serde_json::json!({
            "type": "tool_call",
            "tool_name": "bash_executor",
            "duration_ms": 150,
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("tool_call", &event, "agent-2");

        // THEN it includes tool name and duration
        assert_eq!(summary, "[agent-2] Tool: bash_executor (150ms)");
    }

    #[test]
    fn test_build_event_summary_llm_call() {
        // GIVEN an llm_call event JSON
        let event = serde_json::json!({
            "type": "llm_call",
            "model": "sonnet",
            "cost_usd": 0.0015,
            "timestamp": "2026-03-13T10:00:00Z"
        });

        // WHEN building summary
        let summary = build_event_summary("llm_call", &event, "agent-3");

        // THEN it includes model and cost
        assert_eq!(summary, "[agent-3] LLM: sonnet $0.0015");
    }

    #[test]
    fn test_audit_trail_entry_serializes() {
        // GIVEN an AuditTrailEntry with both agent_id (UUID) and agent_name
        let entry = AuditTrailEntry {
            id: "inv-001".to_string(),
            tool_name: "file_io".to_string(),
            agent_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            agent_name: "standup-scribe".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            duration_ms: Some(42),
            exit_code: Some(0),
            args_json: Some(r#"{"path": "/tmp/test"}"#.to_string()),
            stdout: Some("ok".to_string()),
            stderr: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are present including agent_name
        assert_eq!(json["tool_name"], "file_io");
        assert_eq!(json["agent_name"], "standup-scribe");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["exit_code"], 0);
        assert!(json["stderr"].is_null());
    }

    #[test]
    fn test_llm_daily_costs_response_serializes() {
        // GIVEN an LlmDailyCostsResponse
        let resp = LlmDailyCostsResponse {
            entries: vec![
                LlmDailyCostEntry {
                    date: "2026-03-12".to_string(),
                    backend: "anthropic".to_string(),
                    cost_usd: 0.15,
                },
                LlmDailyCostEntry {
                    date: "2026-03-13".to_string(),
                    backend: "local".to_string(),
                    cost_usd: 0.0,
                },
            ],
            days: 7,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN entries and days are correct
        assert_eq!(json["days"], 7);
        let entries = json["entries"].as_array().expect("entries is array");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["date"], "2026-03-12");
        assert_eq!(entries[0]["backend"], "anthropic");
    }

    #[test]
    fn test_global_timeline_event_serializes() {
        // GIVEN a GlobalTimelineEvent
        let event = GlobalTimelineEvent {
            event_type: "task".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            summary: "[agent-1] Task → working".to_string(),
            detail: serde_json::json!({"status": "working"}),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&event).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["event_type"], "task");
        assert_eq!(json["summary"], "[agent-1] Task → working");
        assert_eq!(json["detail"]["status"], "working");
    }
}
