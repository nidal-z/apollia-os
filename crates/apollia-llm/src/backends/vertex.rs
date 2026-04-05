//! Client Google Vertex AI via `reqwest` direct.
//!
//! Backend Anthropic-compatible sur Google Cloud Platform via l'endpoint
//! `rawPredict`. Le corps de requête est identique à l'API Anthropic Messages.
//! Auth via Application Default Credentials (ADC) avec cache mémoire et
//! refresh automatique avant expiration.
//!
//! # Architecture
//!
//! ```text
//! apollia-llm [feature = "cloud"]
//!   └── VertexClient : CompletionModel
//!         ├── new()          — construit le client, vérifie l'existence du fichier ADC
//!         ├── get_token()    — retourne le token en cache ou déclenche un refresh
//!         ├── refresh_token() — lit le fichier ADC et POST oauth2.googleapis.com/token
//!         ├── build_request() — convertit CompletionRequest en corps JSON Vertex
//!         └── do_complete()  — POST rawPredict → CompletionResponse
//! ```
//!
//! # Fichier ADC
//!
//! Chemin résolu dans l'ordre :
//! 1. Variable d'environnement `GOOGLE_APPLICATION_CREDENTIALS`
//! 2. `~/.config/gcloud/application_default_credentials.json`
//!
//! Seul le type `authorized_user` (credentials gcloud) est supporté.
//! Les clés de service JSON sont hors périmètre.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use futures::Stream;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use apollia_core::VertexConfig;

use crate::backends::anthropic::AnthropicClient;
use crate::retry::RetryPolicy;
use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, MessageContent, Role,
    StreamChunk,
};

// ─────────────────────────────────────────────
// Constantes
// ─────────────────────────────────────────────

/// URL de base de l'API Vertex AI (sans région).
const VERTEX_API_BASE: &str = "https://aiplatform.googleapis.com";

/// Version du protocole Anthropic transmise dans le corps des requêtes Vertex.
const ANTHROPIC_VERSION_VERTEX: &str = "vertex-2023-10-16";

/// Endpoint de refresh OAuth2 Google.
const TOKEN_REFRESH_URL: &str = "https://oauth2.googleapis.com/token";

/// Marge de sécurité avant expiration (secondes) — renouvelle le token 60 s avant son échéance.
const TOKEN_EXPIRY_MARGIN_SECS: i64 = 60;

// ─────────────────────────────────────────────
// ADC — désérialisation credentials
// ─────────────────────────────────────────────

/// Contenu du fichier ADC `authorized_user` généré par `gcloud auth application-default login`.
#[derive(serde::Deserialize)]
struct AdcCredentials {
    #[serde(rename = "type")]
    credential_type: String,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}

/// Réponse du endpoint OAuth2 de refresh.
#[derive(serde::Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    expires_in: i64,
}

// ─────────────────────────────────────────────
// Cache token
// ─────────────────────────────────────────────

/// Token d'accès OAuth2 mis en cache avec son instant d'expiration.
#[derive(Debug, Clone)]
struct GoogleToken {
    access_token: String,
    expires_at: DateTime<Utc>,
}

impl GoogleToken {
    /// Retourne `true` si le token est encore utilisable, compte tenu de la marge de sécurité.
    fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at - Duration::seconds(TOKEN_EXPIRY_MARGIN_SECS)
    }
}

// ─────────────────────────────────────────────
// Types de requête — format Vertex / Anthropic
// ─────────────────────────────────────────────

/// Corps de requête pour l'endpoint `rawPredict` Vertex AI.
///
/// Format identique à l'API Anthropic Messages, avec `anthropic_version`
/// dans le corps au lieu d'un header.
#[derive(serde::Serialize)]
struct VertexRequest {
    anthropic_version: &'static str,
    max_tokens: u32,
    messages: Vec<VertexMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Message dans le format Vertex / Anthropic.
#[derive(serde::Serialize)]
struct VertexMessage {
    role: &'static str,
    content: serde_json::Value,
}

// ─────────────────────────────────────────────
// Client
// ─────────────────────────────────────────────

/// Client HTTP pour Google Vertex AI — modèles Anthropic via `rawPredict`.
///
/// Auth via Application Default Credentials (ADC) avec cache mémoire.
/// Le token est rafraîchi automatiquement 60 secondes avant son expiration.
///
/// Partageable via `Arc<VertexClient>` — `reqwest::Client` et
/// `Arc<Mutex<...>>` sont `Clone + Send + Sync`.
pub struct VertexClient {
    config: VertexConfig,
    http_client: reqwest::Client,
    token_cache: Arc<Mutex<Option<GoogleToken>>>,
    retry_policy: RetryPolicy,
    cancel: CancellationToken,
}

impl VertexClient {
    /// Construit un client Vertex AI et vérifie la présence du fichier ADC.
    ///
    /// Retourne `LlmError::Unauthorized` si le fichier ADC est absent au démarrage.
    /// L'absence du fichier en production est détectée immédiatement (Principe #4 — Fail fast).
    pub fn new(config: &VertexConfig, cancel: CancellationToken) -> Result<Self, LlmError> {
        let path = adc_credentials_path();
        if !path.exists() {
            return Err(LlmError::Unauthorized);
        }
        Ok(Self {
            config: config.clone(),
            http_client: reqwest::Client::new(),
            token_cache: Arc::new(Mutex::new(None)),
            retry_policy: RetryPolicy::default(),
            cancel,
        })
    }

    /// Retourne un Bearer token valide, en rafraîchissant depuis OAuth2 si nécessaire.
    ///
    /// 1. Vérifie le cache — si le token n'a pas expiré, le retourne directement.
    /// 2. Sinon, lit le fichier ADC et POST `https://oauth2.googleapis.com/token`.
    /// 3. Met à jour le cache avec le nouveau token.
    async fn get_token(&self) -> Result<String, LlmError> {
        {
            let cache = self.token_cache.lock().await;
            if let Some(token) = cache.as_ref() {
                if token.is_valid() {
                    return Ok(token.access_token.clone());
                }
            }
        }

        let new_token = self.refresh_token().await?;
        let access_token = new_token.access_token.clone();
        {
            let mut cache = self.token_cache.lock().await;
            *cache = Some(new_token);
        }
        Ok(access_token)
    }

    /// Lit le fichier ADC et appelle le endpoint OAuth2 de refresh.
    async fn refresh_token(&self) -> Result<GoogleToken, LlmError> {
        let path = adc_credentials_path();
        let content = std::fs::read_to_string(&path).map_err(|e| {
            LlmError::InferenceError(format!(
                "cannot read ADC credentials file {}: {e}",
                path.display()
            ))
        })?;

        let creds: AdcCredentials = serde_json::from_str(&content)
            .map_err(|e| LlmError::InferenceError(format!("invalid ADC file format: {e}")))?;

        if creds.credential_type != "authorized_user" {
            return Err(LlmError::InferenceError(format!(
                "ADC credential type '{}' not supported — \
                 run `gcloud auth application-default login` to generate user credentials",
                creds.credential_type
            )));
        }

        let params = [
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("refresh_token", creds.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let response = self
            .http_client
            .post(TOKEN_REFRESH_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                LlmError::InferenceError(format!("OAuth2 token refresh request failed: {e}"))
            })?;

        if response.status().as_u16() == 401 {
            return Err(LlmError::Unauthorized);
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::HttpError { status, body });
        }

        let refresh: TokenRefreshResponse = response.json().await.map_err(|e| {
            LlmError::ParseError(format!("cannot parse OAuth2 token refresh response: {e}"))
        })?;

        Ok(GoogleToken {
            access_token: refresh.access_token,
            expires_at: Utc::now() + Duration::seconds(refresh.expires_in),
        })
    }

    /// Construit l'URL `rawPredict` pour ce client.
    fn endpoint_url(&self) -> String {
        format!(
            "{}/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
            VERTEX_API_BASE, self.config.project_id, self.config.location, self.config.model_id,
        )
    }

    /// Convertit une [`CompletionRequest`] en corps JSON Vertex AI.
    ///
    /// Extrait le premier message `System` vers le champ `system` de niveau supérieur.
    /// Les messages `Tool` (résultats d'outils) sont envoyés avec le rôle `user`.
    fn build_request(&self, req: &CompletionRequest) -> VertexRequest {
        let system = req.messages.iter().find_map(|m| {
            if m.role == Role::System {
                if let MessageContent::Text(text) = &m.content {
                    return Some(text.clone());
                }
            }
            None
        });

        let messages = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .filter_map(|m| {
                let role: &'static str = match m.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    Role::System => return None,
                };

                let content = match &m.content {
                    MessageContent::Text(text) => serde_json::Value::String(text.clone()),
                    MessageContent::ToolResult {
                        content,
                        tool_call_id,
                    } => serde_json::json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                    }]),
                    MessageContent::WithToolCalls { text, tool_calls } => {
                        let mut blocks: Vec<serde_json::Value> = Vec::new();
                        if !text.is_empty() {
                            blocks.push(serde_json::json!({ "type": "text", "text": text }));
                        }
                        for tc in tool_calls {
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments,
                            }));
                        }
                        serde_json::Value::Array(blocks)
                    }
                };

                Some(VertexMessage { role, content })
            })
            .collect();

        VertexRequest {
            anthropic_version: ANTHROPIC_VERSION_VERTEX,
            max_tokens: req.max_tokens.unwrap_or(4096),
            messages,
            system,
            temperature: req.temperature,
        }
    }

    /// Effectue un unique appel HTTP `rawPredict` sans retry.
    ///
    /// Mappe les codes HTTP vers les variantes [`LlmError`] :
    /// - 401 → [`LlmError::Unauthorized`] (pas de retry)
    /// - 429 → [`LlmError::RateLimit`] (retryable)
    /// - 503 → [`LlmError::ServiceUnavailable`] (retryable)
    async fn do_complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let token = self.get_token().await?;
        let body = self.build_request(&req);
        let url = self.endpoint_url();

        let http_response = self
            .http_client
            .post(&url)
            .bearer_auth(&token)
            .header(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::HttpError {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                body: e.to_string(),
            })?;

        let status = http_response.status();
        if !status.is_success() {
            let body_text = http_response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => LlmError::Unauthorized,
                429 => LlmError::RateLimit,
                503 => LlmError::ServiceUnavailable,
                code => LlmError::HttpError {
                    status: code,
                    body: body_text,
                },
            });
        }

        let json: serde_json::Value = http_response.json().await.map_err(|e| {
            LlmError::ParseError(format!("failed to decode Vertex AI response: {e}"))
        })?;

        let mut result = AnthropicClient::parse_response(&json)?;
        result.latency_ms = started.elapsed().as_millis() as u64;
        // Vertex AI ne fournit pas le détail de pricing Anthropic — cost_usd reste None.
        result.usage.cost_usd = None;

        tracing::info!(
            project = %self.config.project_id,
            location = %self.config.location,
            model = %self.config.model_id,
            prompt_tokens = result.usage.prompt_tokens,
            completion_tokens = result.usage.completion_tokens,
            latency_ms = result.latency_ms,
            "Vertex AI complete() done"
        );

        Ok(result)
    }
}

#[async_trait::async_trait]
impl CompletionModel for VertexClient {
    /// Envoie une requête d'inférence et retourne la réponse complète.
    ///
    /// Utilise [`RetryPolicy`] pour les erreurs transitoires (429, 503).
    /// Les erreurs non-retryables (401) sont propagées immédiatement.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.retry_policy
            .execute(self.cancel.clone(), || {
                let req = req.clone();
                async move { self.do_complete(req).await }
            })
            .await
    }

    /// Streaming non supporté pour Vertex AI dans cette version.
    ///
    /// Retourne `LlmError::InferenceError`. Le streaming SSE via Vertex sera
    /// implémenté dans une version future.
    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Err(LlmError::InferenceError(
            "streaming not supported for Vertex AI backend in this version".to_owned(),
        ))
    }

    /// Retourne `true` : le fichier ADC est présent et le client est prêt.
    fn is_available(&self) -> bool {
        true
    }

    /// Nom logique du backend dans le `LlmRouter`.
    fn backend_name(&self) -> &str {
        "vertex"
    }

    /// Identifiant du modèle Anthropic configuré sur Vertex.
    fn model_id(&self) -> &str {
        &self.config.model_id
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

/// Retourne le chemin vers le fichier ADC à utiliser.
///
/// Vérifie `GOOGLE_APPLICATION_CREDENTIALS` en premier, puis le chemin
/// gcloud par défaut `~/.config/gcloud/application_default_credentials.json`.
fn adc_credentials_path() -> PathBuf {
    if let Ok(p) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/root"));
    PathBuf::from(home).join(".config/gcloud/application_default_credentials.json")
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> VertexConfig {
        VertexConfig {
            enabled: true,
            project_id: "test-project".to_owned(),
            location: "us-east5".to_owned(),
            model_id: "claude-test@001".to_owned(),
        }
    }

    fn make_client() -> VertexClient {
        VertexClient {
            config: make_config(),
            http_client: reqwest::Client::new(),
            token_cache: Arc::new(Mutex::new(None)),
            retry_policy: RetryPolicy::default(),
            cancel: CancellationToken::new(),
        }
    }

    // GIVEN VertexConfig désérialisée avec enabled = false
    // WHEN on lit le champ enabled
    // THEN il vaut false
    #[test]
    fn test_vertex_config_disabled_by_default() {
        let config: VertexConfig = toml::from_str(
            r#"
            enabled = false
            project_id = "test"
            location = "us-east5"
            model_id = "test"
        "#,
        )
        .expect("valid toml");
        assert!(!config.enabled);
    }

    // GIVEN un token valide en cache (expiration dans 1h)
    // WHEN get_token() est appelé
    // THEN le token en cache est retourné sans appel réseau
    #[tokio::test]
    async fn test_token_cache_returns_cached_when_valid() {
        let client = make_client();
        {
            let mut cache = client.token_cache.lock().await;
            *cache = Some(GoogleToken {
                access_token: "valid-cached-token".to_owned(),
                expires_at: Utc::now() + Duration::hours(1),
            });
        }

        let token = client.get_token().await.expect("cache hit must succeed");
        assert_eq!(token, "valid-cached-token");
    }

    // GIVEN un token expiré en cache (expires_at dans le passé)
    // WHEN get_token() est appelé
    // THEN un refresh est tenté (échec car pas de fichier ADC) — le token expiré n'est pas retourné
    #[tokio::test]
    async fn test_expired_token_triggers_refresh() {
        let client = make_client();
        {
            let mut cache = client.token_cache.lock().await;
            *cache = Some(GoogleToken {
                access_token: "expired-token".to_owned(),
                expires_at: Utc::now() - Duration::hours(1),
            });
        }

        // Le token expiré n'est pas retourné — un refresh est tenté
        let result = client.get_token().await;
        assert!(
            result.is_err(),
            "expired token must trigger refresh, not return the stale value"
        );
        // Vérifier que ce n'est pas le token expiré qui a été retourné
        assert!(!matches!(result, Ok(ref t) if t == "expired-token"));
    }

    // GIVEN une requête avec un message system + user
    // WHEN build_request() est appelé
    // THEN le message system est extrait dans le champ `system` et absent des messages
    #[test]
    fn test_build_request_extracts_system_message() {
        use crate::types::ChatMessage;

        let client = make_client();
        let req = CompletionRequest {
            messages: vec![
                ChatMessage::system("Be helpful"),
                ChatMessage::user("Hello"),
            ],
            ..Default::default()
        };

        let vertex_req = client.build_request(&req);
        assert_eq!(vertex_req.system.as_deref(), Some("Be helpful"));
        assert_eq!(vertex_req.messages.len(), 1);
        assert_eq!(vertex_req.messages[0].role, "user");
    }

    // GIVEN une config Vertex avec project, location et model
    // WHEN endpoint_url() est appelé
    // THEN l'URL contient le bon chemin rawPredict
    #[test]
    fn test_endpoint_url_format() {
        let client = make_client();
        let url = client.endpoint_url();
        assert!(url.contains("test-project"), "URL must contain project_id");
        assert!(url.contains("us-east5"), "URL must contain location");
        assert!(url.contains("claude-test@001"), "URL must contain model_id");
        assert!(
            url.ends_with(":rawPredict"),
            "URL must end with :rawPredict"
        );
    }

    // GIVEN aucun fichier ADC présent (GOOGLE_APPLICATION_CREDENTIALS pointe vers un chemin inexistant)
    // WHEN VertexClient::new() est appelé
    // THEN LlmError::Unauthorized est retourné
    #[test]
    fn test_new_returns_unauthorized_when_adc_absent() {
        // Pointer vers un fichier qui n'existe pas
        std::env::set_var(
            "GOOGLE_APPLICATION_CREDENTIALS",
            "/tmp/apollia-test-nonexistent-adc.json",
        );
        let config = make_config();
        let cancel = CancellationToken::new();
        let result = VertexClient::new(&config, cancel);
        // Nettoyer la variable d'env pour les autres tests
        std::env::remove_var("GOOGLE_APPLICATION_CREDENTIALS");

        assert!(
            matches!(result, Err(LlmError::Unauthorized)),
            "absent ADC must produce Unauthorized, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    #[ignore = "requires valid GCP credentials and network access"]
    async fn test_vertex_integration() {
        let config = VertexConfig {
            enabled: true,
            project_id: std::env::var("GCP_PROJECT_ID").expect("GCP_PROJECT_ID required"),
            location: "us-east5".to_owned(),
            model_id: "claude-sonnet-4-6@20251001".to_owned(),
        };
        let cancel = CancellationToken::new();
        let client = VertexClient::new(&config, cancel).expect("ADC must be present");

        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("Say 'ok'")],
            max_tokens: Some(10),
            ..Default::default()
        };
        let response = client
            .complete(req)
            .await
            .expect("Vertex call must succeed");
        assert!(!response.content.is_empty());
    }
}
