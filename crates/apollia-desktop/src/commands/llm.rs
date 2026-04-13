//! Commandes IPC Tauri pour la gestion des backends LLM.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/llm/*`) via
//! les helpers HTTP du module parent. Les opérations CRUD ciblent
//! `/api/v1/llm/backends` ; le ping et les statistiques utilisent leurs
//! propres routes dédiées.

use std::sync::Arc;

use apollia_llm::LlmRouter;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_post_json, http_put_json};
use crate::SharedLlmRouter;

/// Vue d'un backend LLM pour les opérations CRUD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendView {
    /// Nom logique unique du backend (ex : `"local-code"`).
    pub name: String,
    /// Fournisseur : `"llama-cpp"`, `"openai"`, `"mistral"`, `"anthropic"`, `"ollama"`.
    pub provider: String,
    /// Identifiant du modèle configuré.
    pub model: String,
    /// Configuration JSON supplémentaire propre au fournisseur.
    pub config_json: serde_json::Value,
    /// `true` si ce backend est actif.
    pub enabled: bool,
    /// `true` si c'est le backend utilisé par défaut.
    pub is_default: bool,
}

/// Corps de requête pour la création d'un backend LLM.
#[derive(Debug, Deserialize)]
pub struct CreateLlmBackendPayload {
    /// Nom unique du backend (pattern `^[a-z0-9_-]+$`).
    pub name: String,
    /// Fournisseur du backend.
    pub provider: String,
    /// Identifiant du modèle.
    pub model: String,
    /// Configuration JSON supplémentaire (doit être un objet JSON).
    pub config_json: serde_json::Value,
    /// Activer le backend à la création.
    pub enabled: bool,
    /// Définir comme backend par défaut à la création.
    pub is_default: bool,
}

/// Corps de requête pour la mise à jour d'un backend LLM existant.
#[derive(Debug, Deserialize)]
pub struct UpdateLlmBackendPayload {
    /// Nouveau fournisseur.
    pub provider: String,
    /// Nouvel identifiant de modèle.
    pub model: String,
    /// Nouvelle configuration JSON (doit être un objet JSON).
    pub config_json: serde_json::Value,
    /// Activer ou désactiver le backend.
    pub enabled: bool,
    /// Marquer comme backend par défaut.
    pub is_default: bool,
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

/// Désérialise une valeur JSON en `LlmBackendView`.
fn parse_backend_view(json: serde_json::Value) -> Result<LlmBackendView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid backend response: {e}"))
}

/// Liste tous les backends LLM configurés.
///
/// Délègue à `GET /api/v1/llm/backends`.
#[tauri::command]
pub async fn list_llm_backends(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<LlmBackendView>, String> {
    let json = http_get_json(state.api_port, "/api/v1/llm/backends").await?;

    let backends = json
        .get("backends")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    backends.into_iter().map(parse_backend_view).collect()
}

/// Crée un nouveau backend LLM.
///
/// Délègue à `POST /api/v1/llm/backends`.
#[tauri::command]
pub async fn create_llm_backend(
    state: State<'_, RuntimeHandle>,
    payload: CreateLlmBackendPayload,
) -> Result<LlmBackendView, String> {
    let body = serde_json::json!({
        "name": payload.name,
        "provider": payload.provider,
        "model": payload.model,
        "config_json": payload.config_json,
        "enabled": payload.enabled,
        "is_default": payload.is_default,
    });

    let json = http_post_json(state.api_port, "/api/v1/llm/backends", &body).await?;
    parse_backend_view(json)
}

/// Met à jour un backend LLM existant.
///
/// Délègue à `PUT /api/v1/llm/backends/:name`.
#[tauri::command]
pub async fn update_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
    payload: UpdateLlmBackendPayload,
) -> Result<LlmBackendView, String> {
    let path = format!("/api/v1/llm/backends/{name}");
    let body = serde_json::json!({
        "provider": payload.provider,
        "model": payload.model,
        "config_json": payload.config_json,
        "enabled": payload.enabled,
        "is_default": payload.is_default,
    });

    let json = http_put_json(state.api_port, &path, &body).await?;
    parse_backend_view(json)
}

/// Supprime un backend LLM.
///
/// Délègue à `DELETE /api/v1/llm/backends/:name`.
/// Retourne une erreur 409 si le backend est le backend par défaut.
#[tauri::command]
pub async fn delete_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/llm/backends/{name}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}

/// Définit un backend LLM comme backend par défaut.
///
/// Délègue à `POST /api/v1/llm/backends/:name/set-default`.
#[tauri::command]
pub async fn set_default_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/llm/backends/{name}/set-default");
    let body = serde_json::json!({});
    http_post_json(state.api_port, &path, &body).await?;
    Ok(())
}

/// Ping un backend LLM et retourne la latence.
///
/// Délègue à `POST /api/v1/llm/ping`.
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

/// Retourne le seuil d'alerte de coût LLM configuré en USD.
///
/// Retourne `None` si `cost_alert_threshold_usd` n'est pas configuré dans `apollia.toml`.
#[tauri::command]
pub async fn get_cost_alert_threshold(
    state: State<'_, RuntimeHandle>,
) -> Result<Option<f64>, String> {
    Ok(state
        .llm_router
        .as_ref()
        .and_then(|router| router.cost_alert_threshold_usd()))
}

/// Récupère les statistiques coût/tokens agrégées sur N jours.
///
/// Délègue à `GET /api/v1/llm/costs?days=N`.
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

/// Recharge le routeur LLM depuis `apollia.toml` après un changement de configuration.
///
/// Relit la section `[llm]` du fichier TOML, reconstruit un `LlmRouter` et met à
/// jour le `SharedLlmRouter` géré par Tauri. Utilisé pendant l'onboarding après
/// `setup_local_llm` pour rendre le modèle disponible immédiatement.
#[tauri::command]
pub async fn reload_llm(shared: State<'_, SharedLlmRouter>) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let config_path = home.join(".apollia").join("apollia.toml");

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("failed to read apollia.toml: {e}"))?;

    #[derive(serde::Deserialize)]
    struct TomlLlm {
        llm: Option<apollia_llm::LlmConfig>,
    }
    let parsed: TomlLlm =
        toml::from_str(&content).map_err(|e| format!("failed to parse apollia.toml: {e}"))?;

    let mut llm_config = parsed
        .llm
        .ok_or_else(|| "no [llm] section in apollia.toml".to_string())?;

    // Ensure [llm.routing] exists — default both precise and fast to the
    // configured default backend so the router can be built during onboarding
    // even if the user hasn't manually added a routing section.
    if llm_config.routing.is_none() {
        llm_config.routing = Some(apollia_core::LlmRoutingConfig {
            precise: llm_config.default.clone(),
            fast: llm_config.default.clone(),
        });
    }

    let router = LlmRouter::from_config(&llm_config)
        .await
        .map_err(|e| format!("failed to build LLM router: {e}"))?;

    *shared.write().map_err(|e| format!("lock poisoned: {e}"))? = Some(Arc::new(router));

    tracing::info!("LLM router reloaded from apollia.toml");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_backend_view_round_trips() {
        // GIVEN a JSON object matching LlmBackendView
        let json = serde_json::json!({
            "name": "local-code",
            "provider": "llama-cpp",
            "model": "qwen3-0.6b-q8_0",
            "config_json": { "device": "metal" },
            "enabled": true,
            "is_default": true,
        });

        // WHEN deserialized and re-serialized
        let view: LlmBackendView = serde_json::from_value(json.clone()).expect("deserialize");
        let back = serde_json::to_value(&view).expect("serialize");

        // THEN all fields match
        assert_eq!(back["name"], "local-code");
        assert_eq!(back["provider"], "llama-cpp");
        assert_eq!(back["model"], "qwen3-0.6b-q8_0");
        assert_eq!(back["enabled"], true);
        assert_eq!(back["is_default"], true);
    }

    #[test]
    fn test_parse_backend_view_returns_error_on_invalid_json() {
        // GIVEN invalid JSON (missing required fields)
        let json = serde_json::json!({ "name": "only-name" });

        // WHEN parsing
        let result = parse_backend_view(json);

        // THEN an error is returned
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid backend response"));
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

        // WHEN serialized
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

        // WHEN serialized
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

        // WHEN serialized
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

        // WHEN serialized
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN rows is empty array
        assert_eq!(json["days"], 30);
        assert_eq!(json["rows"].as_array().expect("rows").len(), 0);
    }
}
