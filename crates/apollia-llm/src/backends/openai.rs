//! Client HTTP OpenAI-compatible via `async-openai`.
//!
//! Ce module est compilé uniquement avec `feature = "cloud"`.
//!
//! Supporte tout fournisseur compatible OpenAI (OpenAI, Mistral, Groq, etc.)
//! via une base URL configurable. La clé API est lue depuis une variable
//! d'environnement au moment de la construction — jamais stockée en clair.

use std::pin::Pin;
use std::time::Instant;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall,
        FunctionObject,
    },
    Client,
};
use futures::{Stream, StreamExt};

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};

// ─────────────────────────────────────────────
// Table de prix (par token, en USD)
// ─────────────────────────────────────────────

/// Prix d'entrée gpt-4o-mini par token (tarifs OpenAI 2024).
const GPT_4O_MINI_PROMPT_RATE: f64 = 0.15e-6;
/// Prix de sortie gpt-4o-mini par token.
const GPT_4O_MINI_COMPLETION_RATE: f64 = 0.60e-6;
/// Prix d'entrée gpt-4o par token.
const GPT_4O_PROMPT_RATE: f64 = 2.50e-6;
/// Prix de sortie gpt-4o par token.
const GPT_4O_COMPLETION_RATE: f64 = 10.00e-6;
/// Prix d'entrée gpt-3.5-turbo par token.
const GPT_35_TURBO_PROMPT_RATE: f64 = 0.50e-6;
/// Prix de sortie gpt-3.5-turbo par token.
const GPT_35_TURBO_COMPLETION_RATE: f64 = 1.50e-6;

// ─────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────

/// Configuration pour un backend API compatible OpenAI.
///
/// Désérialisable depuis TOML via `[[llm.backends]]` dans `apollia.toml`
/// pour les entrées de type `"api"`. La clé API n'est jamais stockée ici —
/// elle est lue depuis la variable d'environnement `api_key_env` (Principe #1).
///
/// # Exemple TOML
///
/// ```toml
/// [[llm.backends]]
/// name        = "openai"
/// api_url     = "https://api.openai.com/v1"
/// api_key_env = "OPENAI_API_KEY"
/// model       = "gpt-4o-mini"
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApiBackendConfig {
    /// Nom logique du backend (clé utilisée dans `LlmRouter`).
    pub name: String,
    /// URL de base de l'API (ex. `https://api.openai.com/v1`).
    pub api_url: String,
    /// Nom de la variable d'environnement contenant la clé API.
    pub api_key_env: String,
    /// Identifiant du modèle par défaut pour ce backend.
    pub model: String,
}

impl ApiBackendConfig {
    /// Lit la clé API depuis la variable d'environnement `api_key_env`.
    ///
    /// Retourne `Err(LlmError::ApiKeyMissing)` si la variable n'est pas définie.
    /// La clé n'est jamais loggée ni stockée au-delà de l'appel (Principe #1).
    pub fn resolve_api_key(&self) -> Result<String, LlmError> {
        std::env::var(&self.api_key_env).map_err(|_| LlmError::ApiKeyMissing {
            var: self.api_key_env.clone(),
        })
    }
}

// ─────────────────────────────────────────────
// Client
// ─────────────────────────────────────────────

/// Client HTTP pour tout backend compatible OpenAI.
///
/// Construit via [`OpenAICompatibleClient::new`] avec une [`ApiBackendConfig`]
/// et une clé API résolue. Supporte [`complete`](Self::complete) (réponse
/// complète) et [`stream`](Self::stream) (streaming SSE chunk par chunk).
///
/// Un seul client peut être partagé via `Arc<OpenAICompatibleClient>` —
/// `async_openai::Client` est `Clone + Send + Sync`.
pub struct OpenAICompatibleClient {
    /// Client HTTP async-openai configuré avec la base URL et la clé API.
    client: Client<OpenAIConfig>,
    /// Configuration du backend (nom, URL, modèle par défaut).
    config: ApiBackendConfig,
}

impl OpenAICompatibleClient {
    /// Construit un client OpenAI-compatible prêt à envoyer des requêtes.
    ///
    /// La `api_key` doit être obtenue au préalable via
    /// [`ApiBackendConfig::resolve_api_key`] — elle est transmise ici
    /// et non re-lue depuis l'environnement pour éviter les TOCTOU.
    pub fn new(config: &ApiBackendConfig, api_key: String) -> Self {
        let openai_config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(config.api_url.clone());
        Self {
            client: Client::with_config(openai_config),
            config: config.clone(),
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for OpenAICompatibleClient {
    /// Envoie une requête d'inférence et retourne la réponse complète.
    ///
    /// Le modèle est celui de `req.model` si présent, sinon celui de `config.model`.
    /// Les `tool_calls` de la réponse sont mappés vers [`ToolCall`].
    /// Le `cost_usd` est estimé via la table de prix intégrée.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        let messages = build_messages(&req.messages)?;
        let tools = build_tools(&req.tools);

        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&model).messages(messages);

        if !tools.is_empty() {
            builder.tools(tools);
        }
        if let Some(temp) = req.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = req.max_tokens {
            builder.max_tokens(max_tokens);
        }

        let request = builder
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build request: {e}")))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(map_openai_error)?;

        let latency_ms = started.elapsed().as_millis() as u64;

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::ParseError("no choices in response".to_owned()))?;

        let finish_reason = map_finish_reason(choice.finish_reason.as_ref());
        let content = choice.message.content.unwrap_or_default();

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|e| {
                    tracing::warn!(
                        tool_id = %tc.id,
                        error = %e,
                        "failed to parse tool call arguments as JSON, using null"
                    );
                    serde_json::Value::Null
                });
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let usage = match response.usage {
            Some(u) => TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                cost_usd: estimate_cost_usd(&model, u.prompt_tokens, u.completion_tokens),
            },
            None => TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
            },
        };

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
            latency_ms,
        })
    }

    /// Retourne un stream de chunks texte via SSE.
    ///
    /// Chaque item est `Ok(String)` non vide. Les chunks vides (heartbeat SSE)
    /// sont silencieusement ignorés. Le stream se termine normalement à la fin
    /// de la génération (`finish_reason = Stop` ou `Length`).
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        let messages = build_messages(&req.messages)?;
        let tools = build_tools(&req.tools);

        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&model).messages(messages);

        if !tools.is_empty() {
            builder.tools(tools);
        }
        if let Some(temp) = req.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = req.max_tokens {
            builder.max_tokens(max_tokens);
        }

        let request = builder
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build stream request: {e}")))?;

        let sse_stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(map_openai_error)?;

        let mapped = sse_stream.filter_map(|result| async move {
            match result {
                Ok(response) => {
                    let chunk = response
                        .choices
                        .into_iter()
                        .next()
                        .and_then(|c| c.delta.content)
                        .unwrap_or_default();
                    if chunk.is_empty() {
                        None
                    } else {
                        Some(Ok(StreamChunk::Text(chunk)))
                    }
                }
                Err(e) => Some(Err(map_openai_error(e))),
            }
        });

        Ok(Box::pin(mapped))
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

/// Convertit les messages Apollia en messages `async-openai`.
fn build_messages(
    messages: &[crate::types::ChatMessage],
) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
    messages
        .iter()
        .map(|msg| -> Result<ChatCompletionRequestMessage, LlmError> {
            match (&msg.role, &msg.content) {
                (Role::System, MessageContent::Text(text)) => {
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("system message: {e}")))
                }
                (Role::User, MessageContent::Text(text)) => {
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("user message: {e}")))
                }
                (Role::Assistant, MessageContent::Text(text)) => {
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("assistant message: {e}")))
                }
                (Role::Assistant, MessageContent::WithToolCalls { text, tool_calls }) => {
                    let openai_calls: Vec<ChatCompletionMessageToolCall> = tool_calls
                        .iter()
                        .map(|tc| ChatCompletionMessageToolCall {
                            id: tc.id.clone(),
                            r#type: ChatCompletionToolType::Function,
                            function: FunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.to_string(),
                            },
                        })
                        .collect();
                    let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                    if !text.is_empty() {
                        builder.content(text.as_str());
                    }
                    builder
                        .tool_calls(openai_calls)
                        .build()
                        .map(Into::into)
                        .map_err(|e| {
                            LlmError::InferenceError(format!("assistant+tools message: {e}"))
                        })
                }
                (
                    Role::Tool,
                    MessageContent::ToolResult {
                        tool_call_id,
                        content,
                    },
                ) => ChatCompletionRequestToolMessageArgs::default()
                    .content(content.as_str())
                    .tool_call_id(tool_call_id.as_str())
                    .build()
                    .map(Into::into)
                    .map_err(|e| LlmError::InferenceError(format!("tool message: {e}"))),
                (role, content) => Err(LlmError::InferenceError(format!(
                    "unsupported role/content combination: {role:?}/{content:?}"
                ))),
            }
        })
        .collect()
}

/// Convertit les spécifications d'outils Apollia en tools `async-openai`.
fn build_tools(tools: &[crate::types::ToolSpec]) -> Vec<ChatCompletionTool> {
    tools
        .iter()
        .map(|spec| ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: spec.name.clone(),
                description: Some(spec.description.clone()),
                parameters: Some(spec.parameters.clone()),
                strict: None,
            },
        })
        .collect()
}

/// Mappe le `FinishReason` d'`async-openai` vers [`FinishReason`] Apollia.
fn map_finish_reason(reason: Option<&async_openai::types::FinishReason>) -> FinishReason {
    match reason {
        Some(async_openai::types::FinishReason::Stop) => FinishReason::Stop,
        Some(async_openai::types::FinishReason::Length) => FinishReason::Length,
        Some(async_openai::types::FinishReason::ToolCalls) => FinishReason::ToolCalls,
        Some(async_openai::types::FinishReason::FunctionCall) => FinishReason::ToolCalls,
        Some(async_openai::types::FinishReason::ContentFilter) => FinishReason::Error,
        None => FinishReason::Stop,
    }
}

/// Mappe une erreur `async-openai` vers [`LlmError`].
fn map_openai_error(err: async_openai::error::OpenAIError) -> LlmError {
    use async_openai::error::OpenAIError;
    match err {
        OpenAIError::Reqwest(req_err) => LlmError::HttpError {
            status: req_err.status().map(|s| s.as_u16()).unwrap_or(0),
            body: req_err.to_string(),
        },
        OpenAIError::ApiError(api_err) => LlmError::HttpError {
            status: 0,
            body: api_err.message,
        },
        other => LlmError::InferenceError(other.to_string()),
    }
}

/// Estime le coût en USD à partir du nombre de tokens consommés.
///
/// Retourne `None` pour les modèles non référencés dans la table de prix.
/// Les tarifs sont basés sur les prix OpenAI publiés en 2024 (mai).
/// Cette estimation est indicative — les prix peuvent varier.
fn estimate_cost_usd(model: &str, prompt_tokens: u32, completion_tokens: u32) -> Option<f64> {
    let (prompt_rate, completion_rate) = if model.contains("gpt-4o-mini") {
        (GPT_4O_MINI_PROMPT_RATE, GPT_4O_MINI_COMPLETION_RATE)
    } else if model.contains("gpt-4o") {
        (GPT_4O_PROMPT_RATE, GPT_4O_COMPLETION_RATE)
    } else if model.contains("gpt-3.5-turbo") {
        (GPT_35_TURBO_PROMPT_RATE, GPT_35_TURBO_COMPLETION_RATE)
    } else {
        return None;
    };

    Some(prompt_tokens as f64 * prompt_rate + completion_tokens as f64 * completion_rate)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN une ApiBackendConfig avec api_key_env non définie dans l'environnement
    // WHEN on appelle resolve_api_key()
    // THEN Err(LlmError::ApiKeyMissing { var: "APOLLIA_TEST_KEY_ABSENT_XYZ" }) est retourné
    #[test]
    fn test_ac2_resolve_api_key_missing() {
        let config = ApiBackendConfig {
            name: "openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "APOLLIA_TEST_KEY_ABSENT_XYZ".into(),
            model: "gpt-4o-mini".into(),
        };

        let result = config.resolve_api_key();

        assert!(
            matches!(result, Err(LlmError::ApiKeyMissing { ref var }) if var == "APOLLIA_TEST_KEY_ABSENT_XYZ"),
            "expected ApiKeyMissing for missing env var, got: {result:?}"
        );
    }

    // GIVEN une ApiBackendConfig avec api_key_env définie dans l'environnement
    // WHEN on appelle resolve_api_key()
    // THEN Ok("sk-test-key") est retourné
    #[test]
    fn test_ac2_resolve_api_key_present() {
        // GIVEN — set env var for this test only
        // Safety: test isolation via unique key name
        std::env::set_var("APOLLIA_TEST_KEY_PRESENT_XYZ", "sk-test-key");
        let config = ApiBackendConfig {
            name: "openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "APOLLIA_TEST_KEY_PRESENT_XYZ".into(),
            model: "gpt-4o-mini".into(),
        };

        let result = config.resolve_api_key();

        std::env::remove_var("APOLLIA_TEST_KEY_PRESENT_XYZ");
        assert_eq!(
            result.expect("resolve_api_key must succeed when env var is set"),
            "sk-test-key"
        );
    }

    // GIVEN une chaîne TOML représentant un ApiBackendConfig
    // WHEN on désérialise avec toml::from_str
    // THEN les champs name et model sont corrects
    #[test]
    fn test_ac1_api_backend_config_serde_toml() {
        let toml_str = r#"
            name        = "openai"
            api_url     = "https://api.openai.com/v1"
            api_key_env = "OPENAI_API_KEY"
            model       = "gpt-4o-mini"
        "#;

        let config: ApiBackendConfig =
            toml::from_str(toml_str).expect("TOML deserialization must succeed");

        assert_eq!(config.name, "openai");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_url, "https://api.openai.com/v1");
        assert_eq!(config.api_key_env, "OPENAI_API_KEY");
    }

    // GIVEN un modèle connu "gpt-4o-mini" avec des tokens non nuls
    // WHEN on appelle estimate_cost_usd
    // THEN Some(valeur > 0.0) est retourné
    #[test]
    fn test_ac4_estimate_cost_usd_nonzero_for_known_model() {
        let cost = estimate_cost_usd("gpt-4o-mini", 1000, 500);

        assert!(cost.is_some(), "cost_usd must be Some for gpt-4o-mini");
        assert!(
            cost.unwrap() > 0.0,
            "cost_usd must be positive for non-zero token counts"
        );
    }

    // GIVEN le modèle "gpt-4o-mini" avec 100 prompt + 50 completion tokens
    // WHEN on calcule le coût
    // THEN il correspond aux tarifs attendus (0.15$/1M prompt, 0.60$/1M completion)
    #[test]
    fn test_estimate_cost_usd_exact_value_gpt4o_mini() {
        let cost = estimate_cost_usd("gpt-4o-mini", 100, 50);
        let expected = 100.0 * GPT_4O_MINI_PROMPT_RATE + 50.0 * GPT_4O_MINI_COMPLETION_RATE;

        assert_eq!(cost, Some(expected));
    }

    // GIVEN un modèle inconnu
    // WHEN on appelle estimate_cost_usd
    // THEN None est retourné
    #[test]
    fn test_estimate_cost_usd_none_for_unknown_model() {
        let cost = estimate_cost_usd("mistral-7b-instruct", 1000, 500);

        assert!(
            cost.is_none(),
            "cost_usd must be None for unknown model pricing"
        );
    }

    // GIVEN un OpenAICompatibleClient construit avec une config valide et une clé fictive
    // WHEN on lit is_available(), backend_name(), model_id()
    // THEN les valeurs attendues sont retournées (AC-1)
    #[test]
    fn test_ac1_client_new_is_available() {
        let config = ApiBackendConfig {
            name: "test-openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model: "gpt-4o-mini".into(),
        };
        let client = OpenAICompatibleClient::new(&config, "sk-fake-key".into());

        assert!(client.is_available(), "is_available() must return true");
        assert_eq!(client.backend_name(), "test-openai");
        assert_eq!(client.model_id(), "gpt-4o-mini");
    }

    // GIVEN le FinishReason::Stop d'async-openai
    // WHEN on appelle map_finish_reason
    // THEN FinishReason::Stop Apollia est retourné
    #[test]
    fn test_map_finish_reason_stop() {
        assert_eq!(
            map_finish_reason(Some(&async_openai::types::FinishReason::Stop)),
            FinishReason::Stop
        );
    }

    // GIVEN FinishReason::ToolCalls d'async-openai
    // WHEN on appelle map_finish_reason
    // THEN FinishReason::ToolCalls Apollia est retourné
    #[test]
    fn test_map_finish_reason_tool_calls() {
        assert_eq!(
            map_finish_reason(Some(&async_openai::types::FinishReason::ToolCalls)),
            FinishReason::ToolCalls
        );
    }

    // GIVEN None (FinishReason absent du chunk SSE)
    // WHEN on appelle map_finish_reason
    // THEN FinishReason::Stop est retourné par défaut
    #[test]
    fn test_map_finish_reason_none_defaults_to_stop() {
        assert_eq!(map_finish_reason(None), FinishReason::Stop);
    }
}
