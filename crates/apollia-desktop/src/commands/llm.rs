//! Commandes IPC Tauri pour le diagnostic LLM.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/llm/*`) via
//! les helpers `http_get_json` / `http_post_json`. Les données transitent
//! en JSON brut (`serde_json::Value`) pour éviter de dupliquer les types
//! Rust déjà définis dans `apollia-runtime`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::{http_get_json, http_post_json};

/// Statut d'un backend LLM pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct LlmBackendStatus {
    /// Nom logique du backend (clé de configuration).
    pub name: String,
    /// Type : `"embedded"` ou `"api"`.
    pub backend_type: String,
    /// Identifiant du modèle configuré.
    pub model: String,
    /// Statut : `"ready"`, `"loading"`, ou `"error"`.
    pub status: String,
}

/// Résultat d'un ping sur un backend LLM.
#[derive(Debug, Serialize)]
pub struct PingResult {
    /// Nom du backend pingé.
    pub backend: String,
    /// `true` si le backend a répondu.
    pub available: bool,
    /// Latence en millisecondes (si disponible).
    pub latency_ms: Option<u64>,
    /// Message d'erreur si le ping a échoué.
    pub error: Option<String>,
}

/// Ligne de statistiques coût/tokens pour un backend+modèle.
#[derive(Debug, Serialize)]
pub struct CostStatsRow {
    /// Nom du backend.
    pub backend: String,
    /// Identifiant du modèle.
    pub model: String,
    /// Nombre d'appels LLM.
    pub call_count: u64,
    /// Total de tokens (prompt + completion).
    pub total_tokens: u64,
    /// Coût total estimé en USD.
    pub total_cost_usd: f64,
}

/// Réponse agrégée des statistiques coût/tokens.
#[derive(Debug, Serialize)]
pub struct CostStatsResponse {
    /// Lignes par backend+modèle.
    pub rows: Vec<CostStatsRow>,
    /// Nombre de jours agrégés.
    pub days: u32,
}

/// Liste tous les backends LLM configurés avec leur statut.
///
/// Délègue à `GET /api/v1/llm/status` sur l'API REST interne.
#[tauri::command]
pub async fn list_llm_backends(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<LlmBackendStatus>, String> {
    let json = http_get_json(state.api_port, "/api/v1/llm/status").await?;

    let backends = json
        .get("backends")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = backends
        .into_iter()
        .map(|b| {
            let name = b
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let model = b
                .get("model_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let available = b
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let backend_type = if name.contains("local") || name.contains("embedded") {
                "embedded".to_string()
            } else {
                "api".to_string()
            };

            let status = if available {
                "ready".to_string()
            } else {
                "error".to_string()
            };

            LlmBackendStatus {
                name,
                backend_type,
                model,
                status,
            }
        })
        .collect();

    Ok(result)
}

/// Ping un backend LLM et retourne la latence.
///
/// Délègue à `POST /api/v1/llm/ping` sur l'API REST interne.
#[tauri::command]
pub async fn ping_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<PingResult, String> {
    let body = serde_json::json!({ "backend": name });
    let json = http_post_json(state.api_port, "/api/v1/llm/ping", &body).await;

    match json {
        Ok(resp) => Ok(PingResult {
            backend: resp
                .get("backend")
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_string(),
            available: resp
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            latency_ms: resp.get("latency_ms").and_then(|v| v.as_u64()),
            error: resp.get("error").and_then(|v| v.as_str()).map(String::from),
        }),
        Err(e) => Ok(PingResult {
            backend: name,
            available: false,
            latency_ms: None,
            error: Some(e),
        }),
    }
}

/// Récupère les statistiques coût/tokens agrégées sur N jours.
///
/// Délègue à `GET /api/v1/llm/costs?days=N` sur l'API REST interne.
#[tauri::command]
pub async fn get_llm_cost_stats(
    state: State<'_, RuntimeHandle>,
    days: Option<u32>,
) -> Result<CostStatsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs?days={d}");
    let json = http_get_json(state.api_port, &path).await;

    match json {
        Ok(resp) => {
            let rows = resp
                .get("rows")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|r| CostStatsRow {
                    backend: r
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    model: r
                        .get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    call_count: r.get("call_count").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_tokens: r.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_cost_usd: r
                        .get("total_cost_usd")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                })
                .collect();

            Ok(CostStatsResponse { rows, days: d })
        }
        Err(_) => Ok(CostStatsResponse {
            rows: vec![],
            days: d,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_backend_status_serializes() {
        // GIVEN an LlmBackendStatus
        let status = LlmBackendStatus {
            name: "anthropic".to_string(),
            backend_type: "api".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            status: "ready".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["name"], "anthropic");
        assert_eq!(json["backend_type"], "api");
        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["status"], "ready");
    }

    #[test]
    fn test_ping_result_serializes_success() {
        // GIVEN a successful ping result
        let result = PingResult {
            backend: "local".to_string(),
            available: true,
            latency_ms: Some(42),
            error: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN latency_ms is present and error is null
        assert_eq!(json["available"], true);
        assert_eq!(json["latency_ms"], 42);
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_ping_result_serializes_failure() {
        // GIVEN a failed ping result
        let result = PingResult {
            backend: "anthropic".to_string(),
            available: false,
            latency_ms: None,
            error: Some("connection refused".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN available is false and error is set
        assert_eq!(json["available"], false);
        assert!(json["latency_ms"].is_null());
        assert_eq!(json["error"], "connection refused");
    }

    #[test]
    fn test_cost_stats_response_serializes() {
        // GIVEN a CostStatsResponse with one row
        let resp = CostStatsResponse {
            rows: vec![CostStatsRow {
                backend: "anthropic".to_string(),
                model: "sonnet".to_string(),
                call_count: 15,
                total_tokens: 3000,
                total_cost_usd: 0.045,
            }],
            days: 7,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN rows and days are correct
        assert_eq!(json["days"], 7);
        let rows = json["rows"].as_array().expect("rows is array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["call_count"], 15);
        assert_eq!(rows[0]["total_tokens"], 3000);
    }

    #[test]
    fn test_cost_stats_response_empty() {
        // GIVEN an empty CostStatsResponse
        let resp = CostStatsResponse {
            rows: vec![],
            days: 30,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN rows is empty array
        assert_eq!(json["days"], 30);
        assert_eq!(json["rows"].as_array().expect("rows").len(), 0);
    }
}
