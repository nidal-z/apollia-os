//! Client HTTP Anthropic via `reqwest` direct.
//!
//! Ce module est compilé uniquement avec `feature = "cloud"`.
//!
//! Utilise l'API Anthropic Messages (`/v1/messages`) avec les headers natifs
//! `x-api-key` et `anthropic-version: 2023-06-01`. Pas de SDK tiers.
//!
//! # Architecture
//!
//! ```text
//! apollia-llm [feature = "cloud"]
//!   └── AnthropicClient : CompletionModel
//!         ├── new()            — construit reqwest::Client avec clé API
//!         ├── parse_response() — fonction pure testable sans HTTP
//!         ├── complete()       — POST /v1/messages → CompletionResponse
//!         └── stream()         — POST /v1/messages (stream=true) → SSE chunks
//! ```

use std::pin::Pin;
use std::time::Instant;

use futures::{Stream, StreamExt};
use reqwest::header::{HeaderName, HeaderValue, CONTENT_TYPE};

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, TokenUsage, ToolCall,
};

use super::openai::ApiBackendConfig;

// ─────────────────────────────────────────────
// Constantes
// ─────────────────────────────────────────────

/// Version de l'API Anthropic transmise dans le header `anthropic-version`.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Nom du header portant la clé API Anthropic.
const ANTHROPIC_API_KEY_HEADER: &str = "x-api-key";

/// Nom du header de version de l'API Anthropic.
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";

// ─────────────────────────────────────────────
// Table de prix Claude (par token, en USD)
// ─────────────────────────────────────────────

/// Prix d'entrée pour les modèles Claude Haiku par token.
const CLAUDE_HAIKU_PROMPT_RATE: f64 = 0.80e-6;
/// Prix de sortie pour les modèles Claude Haiku par token.
const CLAUDE_HAIKU_COMPLETION_RATE: f64 = 4.00e-6;
/// Prix d'entrée pour les modèles Claude Sonnet par token.
const CLAUDE_SONNET_PROMPT_RATE: f64 = 3.00e-6;
/// Prix de sortie pour les modèles Claude Sonnet par token.
const CLAUDE_SONNET_COMPLETION_RATE: f64 = 15.00e-6;
/// Prix d'entrée pour les modèles Claude Opus par token.
const CLAUDE_OPUS_PROMPT_RATE: f64 = 15.00e-6;
/// Prix de sortie pour les modèles Claude Opus par token.
const CLAUDE_OPUS_COMPLETION_RATE: f64 = 75.00e-6;

// ─────────────────────────────────────────────
// Types internes — sérialisation requête
// ─────────────────────────────────────────────

/// Format de la requête Anthropic Messages API.
#[derive(serde::Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Message dans le format Anthropic (`role` + `content`).
#[derive(serde::Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: AnthropicContent,
}

/// Contenu d'un message Anthropic — texte simple ou tableau de blocs.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    /// Contenu textuel simple.
    Text(String),
    /// Tableau de blocs de contenu (outil, résultat d'outil, ou texte structuré).
    Blocks(Vec<AnthropicBlock>),
}

/// Bloc de contenu dans le format Anthropic.
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    /// Bloc de texte.
    Text { text: String },
    /// Appel d'outil demandé par le modèle.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Résultat d'un appel d'outil retourné au modèle.
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Spécification d'un outil au format Anthropic.
#[derive(serde::Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    /// Schéma JSON des paramètres (équivalent de `parameters` en format OpenAI).
    input_schema: serde_json::Value,
}

// ─────────────────────────────────────────────
// Types internes — désérialisation réponse
// ─────────────────────────────────────────────

/// Réponse Anthropic parsée depuis le JSON.
#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

/// Item de contenu dans la réponse Anthropic.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicResponseContent {
    /// Texte généré par le modèle.
    Text { text: String },
    /// Appel d'outil demandé par le modèle.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Statistiques de tokens dans la réponse Anthropic.
#[derive(serde::Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// ─────────────────────────────────────────────
// Client
// ─────────────────────────────────────────────

/// Client HTTP natif pour l'API Anthropic Messages (`/v1/messages`).
///
/// Construit via [`AnthropicClient::new`] avec une [`ApiBackendConfig`]
/// et une clé API résolue depuis la variable d'environnement.
/// Supporte [`complete`](Self::complete) (réponse complète) et
/// [`stream`](Self::stream) (streaming SSE chunk par chunk).
///
/// Un seul client peut être partagé via `Arc<AnthropicClient>` —
/// `reqwest::Client` est `Clone + Send + Sync`.
///
/// # Headers Anthropic obligatoires
///
/// Chaque requête inclut :
/// - `x-api-key: {api_key}`
/// - `anthropic-version: 2023-06-01`
/// - `content-type: application/json`
pub struct AnthropicClient {
    /// Client HTTP reqwest de base.
    client: reqwest::Client,
    /// Configuration du backend (nom, URL de base, modèle par défaut).
    config: ApiBackendConfig,
    /// Clé API Anthropic — incluse dans chaque requête via `x-api-key`.
    /// Jamais loggée ni sérialisée (Principe #1).
    api_key: String,
}

impl AnthropicClient {
    /// Construit un client Anthropic prêt à envoyer des requêtes.
    ///
    /// La `api_key` doit être obtenue au préalable via
    /// [`ApiBackendConfig::resolve_api_key`] — elle est transmise ici
    /// et non re-lue depuis l'environnement pour éviter les TOCTOU (Principe #1).
    ///
    /// Les headers Anthropic obligatoires (`x-api-key`, `anthropic-version`,
    /// `content-type`) sont ajoutés à chaque requête via [`request_builder`](Self::request_builder).
    pub fn new(config: &ApiBackendConfig, api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: config.clone(),
            api_key,
        }
    }

    /// Parse une réponse JSON Anthropic en [`CompletionResponse`].
    ///
    /// Fonction pure et testable sans HTTP. Mappe :
    /// - `stop_reason = "end_turn"` → [`FinishReason::Stop`]
    /// - `stop_reason = "tool_use"` → [`FinishReason::ToolCalls`]
    /// - `content[].type = "text"` → `response.content`
    /// - `content[].type = "tool_use"` → `response.tool_calls`
    ///
    /// Retourne `LlmError::ParseError` si le JSON ne respecte pas le format
    /// attendu de l'API Anthropic Messages.
    ///
    /// # Note
    ///
    /// `latency_ms` et `usage.cost_usd` sont initialisés à `0` / `None` —
    /// ils sont remplis par [`complete`](Self::complete) après l'appel HTTP.
    pub fn parse_response(json: &serde_json::Value) -> Result<CompletionResponse, LlmError> {
        let response: AnthropicResponse = serde_json::from_value(json.clone())
            .map_err(|e| LlmError::ParseError(format!("invalid Anthropic response: {e}")))?;

        let stop_reason = response.stop_reason.as_deref().unwrap_or("end_turn");
        let finish_reason = map_stop_reason(stop_reason);

        let mut content_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for item in response.content {
            match item {
                AnthropicResponseContent::Text { text } => {
                    content_text = text;
                }
                AnthropicResponseContent::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        Ok(CompletionResponse {
            content: content_text,
            tool_calls,
            usage: TokenUsage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                cost_usd: None, // rempli par complete() avec l'id du modèle
            },
            finish_reason,
            latency_ms: 0, // rempli par complete() après l'appel HTTP
        })
    }

    /// Construit un [`reqwest::RequestBuilder`] avec les headers Anthropic obligatoires.
    ///
    /// Retourne `LlmError::InferenceError` si la clé API contient des caractères
    /// non-ASCII (ne peut pas arriver avec une clé Anthropic valide).
    fn request_builder(&self, url: &str) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key_val = HeaderValue::try_from(self.api_key.as_str())
            .map_err(|e| LlmError::InferenceError(format!("invalid api_key header value: {e}")))?;

        Ok(self
            .client
            .post(url)
            .header(
                HeaderName::from_static(ANTHROPIC_API_KEY_HEADER),
                api_key_val,
            )
            .header(
                HeaderName::from_static(ANTHROPIC_VERSION_HEADER),
                HeaderValue::from_static(ANTHROPIC_VERSION),
            )
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json")))
    }

    /// Convertit une [`CompletionRequest`] en [`AnthropicRequest`].
    ///
    /// Extrait le message de rôle `System` (premier trouvé) vers le champ `system`
    /// de la requête Anthropic. Les autres messages sont convertis via
    /// [`convert_message`].
    fn build_request(&self, req: &CompletionRequest, stream: bool) -> AnthropicRequest {
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        // Premier message System → champ `system` séparé (format Anthropic)
        let system = req.messages.iter().find_map(|msg| {
            if msg.role == Role::System {
                if let MessageContent::Text(text) = &msg.content {
                    return Some(text.clone());
                }
            }
            None
        });

        // Messages non-System convertis au format Anthropic
        let messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|msg| msg.role != Role::System)
            .filter_map(convert_message)
            .collect();

        // Outils Apollia → format Anthropic (input_schema au lieu de parameters)
        let tools = if req.tools.is_empty() {
            None
        } else {
            Some(
                req.tools
                    .iter()
                    .map(|spec| AnthropicTool {
                        name: spec.name.clone(),
                        description: spec.description.clone(),
                        input_schema: spec.parameters.clone(),
                    })
                    .collect(),
            )
        };

        AnthropicRequest {
            model,
            max_tokens: req.max_tokens.unwrap_or(4096),
            messages,
            system,
            tools,
            stream: if stream { Some(true) } else { None },
            temperature: req.temperature,
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for AnthropicClient {
    /// Envoie une requête d'inférence et retourne la réponse complète.
    ///
    /// Mappe `CompletionRequest` → format Anthropic, POST `/v1/messages`,
    /// parse la réponse via [`parse_response`](Self::parse_response), et remplit
    /// `latency_ms` et `cost_usd` (estimatif depuis la table de prix intégrée).
    ///
    /// Retourne `LlmError::HttpError` pour tout status HTTP ≥ 400.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let body = self.build_request(&req, false);
        let model = body.model.clone();

        let url = format!("{}/v1/messages", self.config.api_url);

        let http_response = self
            .request_builder(&url)?
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
            return Err(LlmError::HttpError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let json: serde_json::Value = http_response.json().await.map_err(|e| {
            LlmError::ParseError(format!("failed to decode Anthropic response body: {e}"))
        })?;

        let mut result = Self::parse_response(&json)?;
        result.latency_ms = started.elapsed().as_millis() as u64;
        result.usage.cost_usd = estimate_cost_usd(
            &model,
            result.usage.prompt_tokens,
            result.usage.completion_tokens,
        );

        tracing::info!(
            backend = %self.config.name,
            model = %model,
            prompt_tokens = result.usage.prompt_tokens,
            completion_tokens = result.usage.completion_tokens,
            latency_ms = result.usage.completion_tokens,
            "Anthropic complete() done"
        );

        Ok(result)
    }

    /// Retourne un stream de chunks texte via SSE Anthropic.
    ///
    /// Envoie POST `/v1/messages` avec `stream: true`, consomme les événements SSE
    /// ligne par ligne, extrait les `content_block_delta.delta.text` et les émet.
    /// Le stream se termine à `message_stop`.
    ///
    /// Retourne `LlmError::HttpError` pour tout status HTTP ≥ 400 avant le début
    /// du stream. Les erreurs réseau pendant le stream sont propagées dans les items.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let body = self.build_request(&req, true);
        let url = format!("{}/v1/messages", self.config.api_url);

        let http_response = self
            .request_builder(&url)?
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
            return Err(LlmError::HttpError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        // Convertit le bytes_stream en Vec<u8> pour éviter de nommer bytes::Bytes
        let byte_stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>> =
            Box::pin(http_response.bytes_stream().map(|result| {
                result.map(|b| b.to_vec()).map_err(|e| LlmError::HttpError {
                    status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                    body: e.to_string(),
                })
            }));

        Ok(Box::pin(parse_sse_stream(byte_stream)))
    }

    /// Retourne `true` : le client est configuré et prêt à envoyer des requêtes.
    fn is_available(&self) -> bool {
        true
    }

    /// Nom logique du backend tel que configuré dans `apollia.toml`.
    fn backend_name(&self) -> &str {
        &self.config.name
    }

    /// Identifiant du modèle par défaut de ce backend.
    fn model_id(&self) -> &str {
        &self.config.model
    }
}

// ─────────────────────────────────────────────
// Helpers privés
// ─────────────────────────────────────────────

/// Convertit un [`ChatMessage`](crate::types::ChatMessage) Apollia en
/// [`AnthropicMessage`].
///
/// Retourne `None` pour les messages `System` (gérés séparément dans le champ
/// `system` de la requête) et pour les combinaisons rôle/contenu non supportées.
fn convert_message(msg: &crate::types::ChatMessage) -> Option<AnthropicMessage> {
    match (&msg.role, &msg.content) {
        (Role::User, MessageContent::Text(text)) => Some(AnthropicMessage {
            role: "user",
            content: AnthropicContent::Text(text.clone()),
        }),
        (Role::Assistant, MessageContent::Text(text)) => Some(AnthropicMessage {
            role: "assistant",
            content: AnthropicContent::Text(text.clone()),
        }),
        (Role::Assistant, MessageContent::WithToolCalls { text, tool_calls }) => {
            let mut blocks: Vec<AnthropicBlock> = Vec::new();
            if !text.is_empty() {
                blocks.push(AnthropicBlock::Text { text: text.clone() });
            }
            for tc in tool_calls {
                blocks.push(AnthropicBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }
            Some(AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Blocks(blocks),
            })
        }
        (
            Role::Tool,
            MessageContent::ToolResult {
                tool_call_id,
                content,
            },
        ) => {
            // Les résultats d'outils sont des messages user avec un bloc tool_result
            Some(AnthropicMessage {
                role: "user",
                content: AnthropicContent::Blocks(vec![AnthropicBlock::ToolResult {
                    tool_use_id: tool_call_id.clone(),
                    content: content.clone(),
                }]),
            })
        }
        // System géré séparément — ignoré ici
        (Role::System, _) => None,
        // Combinaisons non supportées — ignorées avec warning
        (role, content) => {
            tracing::warn!(
                role = ?role,
                content = ?content,
                "AnthropicClient: combinaison rôle/contenu non supportée, message ignoré"
            );
            None
        }
    }
}

/// Mappe le `stop_reason` Anthropic vers [`FinishReason`] Apollia.
fn map_stop_reason(stop_reason: &str) -> FinishReason {
    match stop_reason {
        "end_turn" => FinishReason::Stop,
        "tool_use" => FinishReason::ToolCalls,
        "max_tokens" => FinishReason::Length,
        _ => FinishReason::Stop,
    }
}

/// Estime le coût en USD depuis le nombre de tokens consommés et le nom du modèle.
///
/// Retourne `None` pour les modèles non référencés dans la table de prix.
/// La détection se fait par sous-chaîne sur le nom du modèle :
/// `"haiku"` → Haiku, `"sonnet"` → Sonnet, `"opus"` → Opus.
fn estimate_cost_usd(model: &str, prompt_tokens: u32, completion_tokens: u32) -> Option<f64> {
    let (prompt_rate, completion_rate) = if model.contains("haiku") {
        (CLAUDE_HAIKU_PROMPT_RATE, CLAUDE_HAIKU_COMPLETION_RATE)
    } else if model.contains("sonnet") {
        (CLAUDE_SONNET_PROMPT_RATE, CLAUDE_SONNET_COMPLETION_RATE)
    } else if model.contains("opus") {
        (CLAUDE_OPUS_PROMPT_RATE, CLAUDE_OPUS_COMPLETION_RATE)
    } else {
        return None;
    };

    Some(prompt_tokens as f64 * prompt_rate + completion_tokens as f64 * completion_rate)
}

/// Convertit un stream de bytes en stream de chunks texte SSE Anthropic.
///
/// Parse les événements SSE ligne par ligne :
/// - `content_block_delta` avec `delta.type = "text_delta"` → émet `delta.text`
/// - `message_stop` → termine le stream
/// - Tous les autres événements sont silencieusement ignorés
fn parse_sse_stream(
    byte_stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
) -> impl Stream<Item = Result<String, LlmError>> + Send {
    struct SseState {
        stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
        buffer: Vec<u8>,
    }

    futures::stream::unfold(
        SseState {
            stream: byte_stream,
            buffer: Vec::new(),
        },
        |mut state| async move {
            loop {
                // Cherche le prochain saut de ligne dans le buffer
                if let Some(nl) = state.buffer.iter().position(|&b| b == b'\n') {
                    let raw: Vec<u8> = state.buffer.drain(..=nl).collect();
                    // Supprime \n et \r finaux
                    let end = raw
                        .iter()
                        .rposition(|&b| b != b'\n' && b != b'\r')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let line = std::str::from_utf8(&raw[..end]).unwrap_or("");

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            match json.get("type").and_then(|t| t.as_str()) {
                                Some("message_stop") => return None,
                                Some("content_block_delta") => {
                                    let is_text_delta =
                                        json.pointer("/delta/type").and_then(|t| t.as_str())
                                            == Some("text_delta");
                                    if is_text_delta {
                                        if let Some(text) =
                                            json.pointer("/delta/text").and_then(|t| t.as_str())
                                        {
                                            if !text.is_empty() {
                                                return Some((Ok(text.to_owned()), state));
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Ligne non-pertinente, continue la boucle
                    continue;
                }

                // Besoin de plus de bytes depuis le stream HTTP
                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.extend_from_slice(&chunk);
                    }
                    Some(Err(e)) => {
                        return Some((Err(e), state));
                    }
                    None => {
                        // Stream HTTP terminé (normalement via message_stop)
                        return None;
                    }
                }
            }
        },
    )
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(all(test, feature = "cloud"))]
mod tests {
    use super::*;
    use serde_json::json;

    // GIVEN une réponse Anthropic avec stop_reason = "end_turn" et content[].text
    // WHEN on appelle parse_response()
    // THEN finish_reason == Stop, content == texte extrait, tokens corrects
    #[test]
    fn test_ac2_parse_end_turn_response() {
        let json = json!({
            "content": [{"type": "text", "text": "Bonjour !"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.content, "Bonjour !");
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
    }

    // GIVEN une réponse Anthropic avec stop_reason = "tool_use" et content[].type = "tool_use"
    // WHEN on appelle parse_response()
    // THEN finish_reason == ToolCalls, tool_calls[0] contient id et name corrects
    #[test]
    fn test_ac3_parse_tool_use_response() {
        let json = json!({
            "content": [
                {"type": "tool_use", "id": "toolu_01", "name": "file_io",
                 "input": {"path": "/tmp/test.txt"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 10}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_01");
        assert_eq!(response.tool_calls[0].name, "file_io");
    }

    // GIVEN un JSON Anthropic malformé (champs attendus absents)
    // WHEN on appelle parse_response()
    // THEN Err(LlmError::ParseError) est retourné
    #[test]
    fn test_parse_error_on_malformed_json() {
        let json = json!({"unexpected": "format"});

        let result = AnthropicClient::parse_response(&json);

        assert!(
            matches!(result, Err(LlmError::ParseError(_))),
            "expected ParseError for malformed response, got: {result:?}"
        );
    }

    // GIVEN un ApiBackendConfig pour Anthropic avec une clé fictive
    // WHEN on appelle AnthropicClient::new()
    // THEN le client est construit sans panique et backend_name() retourne config.name
    #[test]
    fn test_ac1_new_does_not_panic() {
        std::env::set_var("APOLLIA_ANT_TEST_KEY", "sk-ant-test");
        let config = ApiBackendConfig {
            name: "anthropic".into(),
            api_url: "https://api.anthropic.com".into(),
            api_key_env: "APOLLIA_ANT_TEST_KEY".into(),
            model: "claude-haiku-4-5-20251001".into(),
        };

        let client = AnthropicClient::new(&config, "sk-ant-test".into());

        std::env::remove_var("APOLLIA_ANT_TEST_KEY");
        assert!(client.is_available());
        assert_eq!(client.backend_name(), "anthropic");
        assert_eq!(client.model_id(), "claude-haiku-4-5-20251001");
    }

    // GIVEN une réponse avec stop_reason = "max_tokens"
    // WHEN on appelle parse_response()
    // THEN finish_reason == Length
    #[test]
    fn test_parse_max_tokens_finish_reason() {
        let json = json!({
            "content": [{"type": "text", "text": "tronqué"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 100, "output_tokens": 500}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::Length);
    }

    // GIVEN le modèle "claude-haiku-4-5-20251001" avec des tokens non nuls
    // WHEN on appelle estimate_cost_usd
    // THEN Some(valeur > 0.0) est retourné
    #[test]
    fn test_estimate_cost_usd_haiku_nonzero() {
        let cost = estimate_cost_usd("claude-haiku-4-5-20251001", 1000, 500);

        assert!(cost.is_some(), "cost_usd must be Some for Claude Haiku");
        assert!(cost.unwrap() > 0.0, "cost_usd must be positive");
    }

    // GIVEN un modèle inconnu
    // WHEN on appelle estimate_cost_usd
    // THEN None est retourné
    #[test]
    fn test_estimate_cost_usd_none_for_unknown_model() {
        let cost = estimate_cost_usd("some-unknown-model-xyz", 1000, 500);

        assert!(
            cost.is_none(),
            "cost_usd must be None for unknown model pricing"
        );
    }

    // GIVEN une réponse avec text + tool_use dans le contenu
    // WHEN on appelle parse_response()
    // THEN content contient le texte ET tool_calls contient l'outil
    #[test]
    fn test_parse_mixed_content_text_and_tool_use() {
        let json = json!({
            "content": [
                {"type": "text", "text": "Je vais lire ce fichier."},
                {"type": "tool_use", "id": "toolu_02", "name": "bash_executor",
                 "input": {"command": "cat /tmp/data.txt"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 30, "output_tokens": 15}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.content, "Je vais lire ce fichier.");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_02");
        assert_eq!(response.tool_calls[0].name, "bash_executor");
    }
}
