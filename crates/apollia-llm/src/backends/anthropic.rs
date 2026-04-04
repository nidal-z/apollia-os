//! Client HTTP Anthropic via `reqwest` direct.
//!
//! Ce module est compilé uniquement avec `feature = "cloud"`.
//!
//! Utilise l'API Anthropic Messages (`/v1/messages`) avec les headers natifs
//! `x-api-key`, `anthropic-version: 2023-06-01` et le header beta
//! `prompt-caching-2024-07-31` pour le prompt caching.
//!
//! # Architecture
//!
//! ```text
//! apollia-llm [feature = "cloud"]
//!   └── AnthropicClient : CompletionModel
//!         ├── new()                    — construit reqwest::Client avec clé API
//!         ├── parse_response()         — fonction pure testable sans HTTP
//!         ├── complete()               — POST /v1/messages → CompletionResponse
//!         └── stream()                 — POST /v1/messages (stream=true) → SSE chunks
//! ```
//!
//! # Prompt caching
//!
//! Chaque requête inclut le header beta `prompt-caching-2024-07-31`.
//! `build_request()` applique trois breakpoints automatiques :
//! - system prompt (breakpoint stable)
//! - dernier outil (breakpoint stable)
//! - 3ème message depuis la fin (breakpoint glissant)
//!
//! Les messages avec `ChatMessage.cache_control = Some(CacheControl::Ephemeral)`
//! sont également marqués lors de la conversion.

use std::pin::Pin;
use std::time::Instant;

use futures::{Stream, StreamExt};
use reqwest::header::{HeaderName, HeaderValue, CONTENT_TYPE};
use tokio_util::sync::CancellationToken;

use crate::pricing::{self, PricingTier};
use crate::retry::RetryPolicy;
use crate::types::{
    CacheControl, CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall,
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

/// Header beta Anthropic activant le prompt caching.
const PROMPT_CACHING_BETA_HEADER: &str = "anthropic-beta";

/// Valeur du header beta pour le prompt caching.
const PROMPT_CACHING_BETA: &str = "prompt-caching-2024-07-31";

/// Multiplicateur de coût pour l'écriture dans le cache (25% plus cher que l'input normal).
const CACHE_WRITE_COST_MULTIPLIER: f64 = 1.25;

/// Multiplicateur de coût pour la lecture depuis le cache (90% moins cher que l'input normal).
const CACHE_READ_COST_MULTIPLIER: f64 = 0.1;

// ─────────────────────────────────────────────
// Types internes — cache_control
// ─────────────────────────────────────────────

/// Représentation du `cache_control` dans le format API Anthropic.
///
/// Sérialisé en `{"type": "ephemeral"}`.
#[derive(serde::Serialize, Clone)]
struct AnthropicCacheControl {
    #[serde(rename = "type")]
    cache_type: &'static str,
}

impl AnthropicCacheControl {
    /// Construit un `cache_control` de type `ephemeral`.
    fn ephemeral() -> Self {
        Self {
            cache_type: "ephemeral",
        }
    }
}

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
    system: Option<AnthropicSystem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Contenu du system prompt — soit une chaîne simple, soit un tableau de blocs avec cache_control.
///
/// L'API Anthropic accepte les deux formes. Le tableau est utilisé quand le prompt caching
/// est activé pour permettre d'inclure `cache_control` sur le dernier bloc.
#[derive(serde::Serialize, Clone)]
#[serde(untagged)]
enum AnthropicSystem {
    /// System prompt sans caching — sérialisé comme une chaîne JSON.
    Plain(String),
    /// System prompt avec caching — sérialisé comme un tableau de blocs.
    Blocks(Vec<AnthropicSystemBlock>),
}

/// Bloc de contenu dans le champ `system` de la requête Anthropic.
#[derive(serde::Serialize, Clone)]
struct AnthropicSystemBlock {
    #[serde(rename = "type")]
    block_type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AnthropicCacheControl>,
}

/// Message dans le format Anthropic (`role` + `content`).
#[derive(serde::Serialize, Clone)]
struct AnthropicMessage {
    role: &'static str,
    content: AnthropicContent,
}

/// Contenu d'un message Anthropic — texte simple ou tableau de blocs.
#[derive(serde::Serialize, Clone)]
#[serde(untagged)]
enum AnthropicContent {
    /// Contenu textuel simple.
    Text(String),
    /// Tableau de blocs de contenu (outil, résultat d'outil, ou texte structuré).
    Blocks(Vec<AnthropicBlock>),
}

/// Bloc de contenu dans le format Anthropic.
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    /// Bloc de texte avec support optionnel du cache.
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    /// Appel d'outil demandé par le modèle.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Résultat d'un appel d'outil retourné au modèle, avec support optionnel du cache.
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
}

/// Spécification d'un outil au format Anthropic.
#[derive(serde::Serialize, Clone)]
struct AnthropicTool {
    name: String,
    description: String,
    /// Schéma JSON des paramètres (équivalent de `parameters` en format OpenAI).
    input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AnthropicCacheControl>,
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

/// Statistiques de tokens dans la réponse Anthropic, incluant les champs de cache.
#[derive(serde::Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    /// Tokens lus depuis le cache — présents uniquement si le prompt caching est actif.
    #[serde(default)]
    cache_read_input_tokens: u32,
    /// Tokens écrits dans le cache — présents uniquement si le prompt caching est actif.
    #[serde(default)]
    cache_write_input_tokens: u32,
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
/// - `anthropic-beta: prompt-caching-2024-07-31`
/// - `content-type: application/json`
pub struct AnthropicClient {
    /// Client HTTP reqwest de base.
    client: reqwest::Client,
    /// Configuration du backend (nom, URL de base, modèle par défaut).
    config: ApiBackendConfig,
    /// Clé API Anthropic — incluse dans chaque requête via `x-api-key`.
    /// Jamais loggée ni sérialisée (Principe #1).
    api_key: String,
    /// Table de pricing par défaut construite au démarrage via [`pricing::default_pricing`].
    pricing_table: std::collections::HashMap<&'static str, PricingTier>,
    /// Surcharges opérateur chargées depuis `[llm.pricing_overrides]` dans `apollia.toml`.
    pricing_overrides: std::collections::HashMap<String, PricingTier>,
    /// Politique de retry exponentiel partagée avec les autres backends.
    retry_policy: RetryPolicy,
    /// Token d'annulation de session — `cancel()` interrompt les appels et délais en cours.
    cancel: CancellationToken,
}

impl AnthropicClient {
    /// Construit un client Anthropic prêt à envoyer des requêtes.
    ///
    /// La `api_key` doit être obtenue au préalable via
    /// [`ApiBackendConfig::resolve_api_key`] — elle est transmise ici
    /// et non re-lue depuis l'environnement pour éviter les TOCTOU (Principe #1).
    ///
    /// Les `pricing_overrides` sont chargés depuis `[llm.pricing_overrides]` dans
    /// `apollia.toml` et ont priorité sur la table par défaut lors du calcul du coût.
    ///
    /// Le `cancel` est le `CancellationToken` de la session LLM — partagé par
    /// le `LlmRouter`. Un appel à `cancel.cancel()` interrompt les appels en cours
    /// et les délais de retry.
    pub fn new(
        config: &ApiBackendConfig,
        api_key: String,
        pricing_overrides: std::collections::HashMap<String, PricingTier>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: config.clone(),
            api_key,
            pricing_table: pricing::default_pricing(),
            pricing_overrides,
            retry_policy: RetryPolicy::default(),
            cancel,
        }
    }

    /// Parse une réponse JSON Anthropic en [`CompletionResponse`].
    ///
    /// Fonction pure et testable sans HTTP. Mappe :
    /// - `stop_reason = "end_turn"` → [`FinishReason::Stop`]
    /// - `stop_reason = "tool_use"` → [`FinishReason::ToolCalls`]
    /// - `content[].type = "text"` → `response.content`
    /// - `content[].type = "tool_use"` → `response.tool_calls`
    /// - `usage.cache_read_input_tokens` / `cache_write_input_tokens` → `TokenUsage`
    ///
    /// Retourne `LlmError::ParseError` si le JSON ne respecte pas le format
    /// attendu de l'API Anthropic Messages.
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
                cache_read_input_tokens: response.usage.cache_read_input_tokens,
                cache_write_input_tokens: response.usage.cache_write_input_tokens,
            },
            finish_reason,
            latency_ms: 0, // rempli par complete() après l'appel HTTP
            ttft_ms: None,
        })
    }

    /// Construit un [`reqwest::RequestBuilder`] avec les headers Anthropic obligatoires.
    ///
    /// Inclut le header beta `prompt-caching-2024-07-31` sur toutes les requêtes
    /// pour activer le prompt caching.
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
            .header(
                HeaderName::from_static(PROMPT_CACHING_BETA_HEADER),
                HeaderValue::from_static(PROMPT_CACHING_BETA),
            )
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json")))
    }

    /// Effectue un unique appel HTTP POST `/v1/messages` sans retry.
    ///
    /// Mappe les status HTTP transitoires vers les variantes retryables de [`LlmError`] :
    /// - 429 → [`LlmError::RateLimit`]
    /// - 503 → [`LlmError::ServiceUnavailable`]
    /// - 529 → [`LlmError::Overload`]
    /// - 401 → [`LlmError::Unauthorized`]
    /// - 400 → [`LlmError::BadRequest`]
    /// - autre ≥ 400 → [`LlmError::HttpError`]
    async fn do_complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
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
            return Err(match status.as_u16() {
                400 => LlmError::BadRequest(body_text),
                401 => LlmError::Unauthorized,
                429 => LlmError::RateLimit,
                503 => LlmError::ServiceUnavailable,
                529 => LlmError::Overload,
                code => LlmError::HttpError {
                    status: code,
                    body: body_text,
                },
            });
        }

        let json: serde_json::Value = http_response.json().await.map_err(|e| {
            LlmError::ParseError(format!("failed to decode Anthropic response body: {e}"))
        })?;

        let mut result = Self::parse_response(&json)?;
        result.latency_ms = started.elapsed().as_millis() as u64;
        result.usage.cost_usd =
            match pricing::lookup_pricing(&model, &self.pricing_table, &self.pricing_overrides) {
                Some(tier) => {
                    let input_cost =
                        result.usage.prompt_tokens as f64 * tier.input_per_mtok / 1_000_000.0;
                    let output_cost =
                        result.usage.completion_tokens as f64 * tier.output_per_mtok / 1_000_000.0;
                    let cache_write_cost = result.usage.cache_write_input_tokens as f64
                        * tier.input_per_mtok
                        * CACHE_WRITE_COST_MULTIPLIER
                        / 1_000_000.0;
                    let cache_read_cost = result.usage.cache_read_input_tokens as f64
                        * tier.input_per_mtok
                        * CACHE_READ_COST_MULTIPLIER
                        / 1_000_000.0;
                    Some(input_cost + output_cost + cache_write_cost + cache_read_cost)
                }
                None => {
                    tracing::warn!(model_id = %model, "unknown model for pricing");
                    None
                }
            };

        tracing::info!(
            backend = %self.config.name,
            model = %model,
            prompt_tokens = result.usage.prompt_tokens,
            completion_tokens = result.usage.completion_tokens,
            cache_read_tokens = result.usage.cache_read_input_tokens,
            cache_write_tokens = result.usage.cache_write_input_tokens,
            latency_ms = result.latency_ms,
            "Anthropic complete() done"
        );

        Ok(result)
    }

    /// Convertit une [`CompletionRequest`] en [`AnthropicRequest`] avec breakpoints de cache.
    ///
    /// Extrait le message de rôle `System` (premier trouvé) vers le champ `system`
    /// de la requête Anthropic. Les autres messages sont convertis via
    /// [`convert_message`].
    ///
    /// Applique ensuite trois breakpoints de cache automatiques via
    /// [`apply_cache_breakpoints`] :
    /// 1. System prompt
    /// 2. Dernier outil
    /// 3. 3ème message depuis la fin de l'historique (breakpoint glissant)
    fn build_request(&self, req: &CompletionRequest, stream: bool) -> AnthropicRequest {
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        // Premier message System → champ `system` séparé (format Anthropic)
        let mut system: Option<AnthropicSystem> = req.messages.iter().find_map(|msg| {
            if msg.role == Role::System {
                if let MessageContent::Text(text) = &msg.content {
                    return Some(AnthropicSystem::Plain(text.clone()));
                }
            }
            None
        });

        // Messages non-System convertis au format Anthropic
        let mut messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|msg| msg.role != Role::System)
            .filter_map(convert_message)
            .collect();

        // Outils Apollia → format Anthropic (input_schema au lieu de parameters)
        let mut tools: Option<Vec<AnthropicTool>> = if req.tools.is_empty() {
            None
        } else {
            Some(
                req.tools
                    .iter()
                    .map(|spec| AnthropicTool {
                        name: spec.name.clone(),
                        description: spec.description.clone(),
                        input_schema: spec.parameters.clone(),
                        cache_control: None,
                    })
                    .collect(),
            )
        };

        // Applique les breakpoints de cache automatiques
        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

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
    /// Délègue à [`do_complete`](Self::do_complete) via [`RetryPolicy::execute`] :
    /// les erreurs transitoires (429, 503, 529) sont retentées avec backoff exponentiel.
    /// Une annulation via le `CancellationToken` interrompt immédiatement l'attente.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.retry_policy
            .execute(self.cancel.clone(), || {
                let req = req.clone();
                async move { self.do_complete(req).await }
            })
            .await
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
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
            if status.as_u16() == 400 {
                return Err(LlmError::BadRequest(body_text));
            }
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
// Helpers privés — prompt caching
// ─────────────────────────────────────────────

/// Applique les trois breakpoints de cache automatiques sur la requête Anthropic.
///
/// Breakpoint 1 — system prompt : converti en tableau de blocs avec `cache_control: ephemeral`.
/// Breakpoint 2 — dernier outil : marqué avec `cache_control: ephemeral`.
/// Breakpoint 3 — 3ème message depuis la fin : dernier bloc de contenu marqué.
///
/// Les breakpoints sont cumulatifs avec les marques individuelles issues de
/// `ChatMessage.cache_control` (déjà appliquées par [`convert_message`]).
fn apply_cache_breakpoints(
    messages: &mut [AnthropicMessage],
    system: &mut Option<AnthropicSystem>,
    tools: &mut Option<Vec<AnthropicTool>>,
) {
    // Breakpoint 1 : system prompt → bloc avec cache_control
    if let Some(sys) = system.as_mut() {
        *sys = match sys.clone() {
            AnthropicSystem::Plain(text) => AnthropicSystem::Blocks(vec![AnthropicSystemBlock {
                block_type: "text",
                text,
                cache_control: Some(AnthropicCacheControl::ephemeral()),
            }]),
            AnthropicSystem::Blocks(mut blocks) => {
                if let Some(last) = blocks.last_mut() {
                    last.cache_control = Some(AnthropicCacheControl::ephemeral());
                }
                AnthropicSystem::Blocks(blocks)
            }
        };
    }

    // Breakpoint 2 : dernier outil
    if let Some(tools_vec) = tools.as_mut() {
        if let Some(last_tool) = tools_vec.last_mut() {
            last_tool.cache_control = Some(AnthropicCacheControl::ephemeral());
        }
    }

    // Breakpoint 3 : 3ème message depuis la fin (breakpoint glissant)
    let len = messages.len();
    if len >= 3 {
        mark_message_cache_control(&mut messages[len - 3]);
    }
}

/// Marque le dernier bloc de contenu d'un message avec `cache_control: ephemeral`.
///
/// Si le contenu est `Text(String)`, le convertit en `Blocks([Text { cache_control }])`.
/// Si le contenu est `Blocks(...)`, applique `cache_control` au dernier bloc `Text` ou `ToolResult`.
fn mark_message_cache_control(msg: &mut AnthropicMessage) {
    let new_content = match msg.content.clone() {
        AnthropicContent::Text(text) => AnthropicContent::Blocks(vec![AnthropicBlock::Text {
            text,
            cache_control: Some(AnthropicCacheControl::ephemeral()),
        }]),
        AnthropicContent::Blocks(mut blocks) => {
            if let Some(last) = blocks.last_mut() {
                match last {
                    AnthropicBlock::Text { cache_control, .. } => {
                        *cache_control = Some(AnthropicCacheControl::ephemeral());
                    }
                    AnthropicBlock::ToolResult { cache_control, .. } => {
                        *cache_control = Some(AnthropicCacheControl::ephemeral());
                    }
                    AnthropicBlock::ToolUse { .. } => {
                        // ToolUse ne supporte pas cache_control — breakpoint ignoré
                        tracing::debug!(
                            "cache breakpoint on message ending with tool_use block — ignored"
                        );
                    }
                }
            }
            AnthropicContent::Blocks(blocks)
        }
    };
    msg.content = new_content;
}

// ─────────────────────────────────────────────
// Helpers privés — conversion messages
// ─────────────────────────────────────────────

/// Convertit un [`ChatMessage`](crate::types::ChatMessage) Apollia en
/// [`AnthropicMessage`].
///
/// Retourne `None` pour les messages `System` (gérés séparément dans le champ
/// `system` de la requête) et pour les combinaisons rôle/contenu non supportées.
///
/// Quand `ChatMessage.cache_control` est `Some(CacheControl::Ephemeral)`, le contenu
/// texte est converti en bloc avec `cache_control: ephemeral`.
fn convert_message(msg: &crate::types::ChatMessage) -> Option<AnthropicMessage> {
    let cache = msg
        .cache_control
        .as_ref()
        .filter(|cc| **cc == CacheControl::Ephemeral)
        .map(|_| AnthropicCacheControl::ephemeral());

    match (&msg.role, &msg.content) {
        (Role::User, MessageContent::Text(text)) => {
            let content = if let Some(cc) = cache {
                AnthropicContent::Blocks(vec![AnthropicBlock::Text {
                    text: text.clone(),
                    cache_control: Some(cc),
                }])
            } else {
                AnthropicContent::Text(text.clone())
            };
            Some(AnthropicMessage {
                role: "user",
                content,
            })
        }
        (Role::Assistant, MessageContent::Text(text)) => {
            let content = if let Some(cc) = cache {
                AnthropicContent::Blocks(vec![AnthropicBlock::Text {
                    text: text.clone(),
                    cache_control: Some(cc),
                }])
            } else {
                AnthropicContent::Text(text.clone())
            };
            Some(AnthropicMessage {
                role: "assistant",
                content,
            })
        }
        (Role::Assistant, MessageContent::WithToolCalls { text, tool_calls }) => {
            let mut blocks: Vec<AnthropicBlock> = Vec::new();
            if !text.is_empty() {
                blocks.push(AnthropicBlock::Text {
                    text: text.clone(),
                    cache_control: None,
                });
            }
            for tc in tool_calls {
                blocks.push(AnthropicBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }
            // cache_control sur le dernier bloc si marqué
            if let Some(cc) = cache {
                if let Some(last) = blocks.last_mut() {
                    match last {
                        AnthropicBlock::Text { cache_control, .. } => {
                            *cache_control = Some(cc);
                        }
                        AnthropicBlock::ToolUse { .. } => {
                            // ToolUse ne supporte pas cache_control — ignoré
                        }
                        AnthropicBlock::ToolResult { cache_control, .. } => {
                            *cache_control = Some(cc);
                        }
                    }
                }
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
                    cache_control: cache,
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

/// Convertit un stream de bytes en stream de chunks SSE Anthropic.
///
/// Parse les événements SSE ligne par ligne :
/// - `content_block_delta` avec `delta.type = "text_delta"` → émet `StreamChunk::Text`
/// - `content_block_start` avec `type = "tool_use"` → enregistre id + name
/// - `content_block_delta` avec `delta.type = "input_json_delta"` → accumule les arguments JSON
/// - `content_block_stop` → émet `StreamChunk::ToolCall` si un outil était en cours
/// - `message_stop` → termine le stream
fn parse_sse_stream(
    byte_stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
) -> impl Stream<Item = Result<StreamChunk, LlmError>> + Send {
    /// In-progress tool call being assembled from SSE fragments.
    struct PendingToolCall {
        id: String,
        name: String,
        arguments_json: String,
    }

    struct SseState {
        stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
        buffer: Vec<u8>,
        /// Tool call currently being accumulated (one at a time).
        pending_tool: Option<PendingToolCall>,
    }

    futures::stream::unfold(
        SseState {
            stream: byte_stream,
            buffer: Vec::new(),
            pending_tool: None,
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

                                // Text token — emit immediately
                                Some("content_block_delta") => {
                                    let delta_type =
                                        json.pointer("/delta/type").and_then(|t| t.as_str());

                                    match delta_type {
                                        Some("text_delta") => {
                                            if let Some(text) =
                                                json.pointer("/delta/text").and_then(|t| t.as_str())
                                            {
                                                if !text.is_empty() {
                                                    return Some((
                                                        Ok(StreamChunk::Text(text.to_owned())),
                                                        state,
                                                    ));
                                                }
                                            }
                                        }
                                        // Tool call arguments arrive as JSON fragments
                                        Some("input_json_delta") => {
                                            if let Some(partial) = json
                                                .pointer("/delta/partial_json")
                                                .and_then(|t| t.as_str())
                                            {
                                                if let Some(ref mut pending) = state.pending_tool {
                                                    pending.arguments_json.push_str(partial);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                // Tool call starts — record id and name
                                Some("content_block_start") => {
                                    let block_type = json
                                        .pointer("/content_block/type")
                                        .and_then(|t| t.as_str());
                                    if block_type == Some("tool_use") {
                                        let id = json
                                            .pointer("/content_block/id")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("")
                                            .to_owned();
                                        let name = json
                                            .pointer("/content_block/name")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("")
                                            .to_owned();
                                        state.pending_tool = Some(PendingToolCall {
                                            id,
                                            name,
                                            arguments_json: String::new(),
                                        });
                                    }
                                }

                                // Content block ends — emit tool call if one was pending
                                Some("content_block_stop") => {
                                    if let Some(pending) = state.pending_tool.take() {
                                        let arguments =
                                            serde_json::from_str(&pending.arguments_json)
                                                .unwrap_or(serde_json::Value::Null);
                                        return Some((
                                            Ok(StreamChunk::ToolCall(ToolCall {
                                                id: pending.id,
                                                name: pending.name,
                                                arguments,
                                            })),
                                            state,
                                        ));
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
        assert_eq!(response.usage.cache_read_input_tokens, 0);
        assert_eq!(response.usage.cache_write_input_tokens, 0);
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

        let client = AnthropicClient::new(
            &config,
            "sk-ant-test".into(),
            std::collections::HashMap::new(),
            tokio_util::sync::CancellationToken::new(),
        );

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

    // GIVEN une réponse avec cache_read_input_tokens et cache_write_input_tokens
    // WHEN on appelle parse_response()
    // THEN les champs cache sont correctement extraits dans TokenUsage
    #[test]
    fn test_parse_response_extracts_cache_tokens() {
        let json = json!({
            "content": [{"type": "text", "text": "response"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 150,
                "cache_write_input_tokens": 300
            }
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.usage.cache_read_input_tokens, 150);
        assert_eq!(response.usage.cache_write_input_tokens, 300);
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
    }

    // GIVEN un system prompt + liste de messages (>= 3)
    // WHEN apply_cache_breakpoints() est appelé
    // THEN le system est en forme Blocks avec cache_control
    //   ET le 3ème message depuis la fin a cache_control
    #[test]
    fn test_apply_cache_breakpoints_system_and_sliding() {
        let mut system = Some(AnthropicSystem::Plain("Be helpful".to_string()));
        let mut messages = vec![
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg1".to_string()),
            },
            AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Text("msg2".to_string()),
            },
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg3".to_string()),
            },
            AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Text("msg4".to_string()),
            },
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg5".to_string()),
            },
        ];
        let mut tools: Option<Vec<AnthropicTool>> = None;

        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

        // System doit être en Blocks avec cache_control
        match &system {
            Some(AnthropicSystem::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 1);
                assert!(blocks[0].cache_control.is_some());
            }
            _ => panic!("system must be Blocks after apply_cache_breakpoints"),
        }

        // messages[2] = 3ème depuis la fin (index len-3 = 5-3 = 2) doit avoir cache_control
        match &messages[2].content {
            AnthropicContent::Blocks(blocks) => {
                assert!(
                    blocks.iter().any(|b| matches!(
                        b,
                        AnthropicBlock::Text {
                            cache_control: Some(_),
                            ..
                        }
                    )),
                    "messages[2] must have cache_control"
                );
            }
            _ => panic!("messages[2] must be Blocks after breakpoint"),
        }

        // messages[4] (dernier) ne doit PAS avoir cache_control auto
        match &messages[4].content {
            AnthropicContent::Text(_) => {}
            _ => panic!("messages[4] must remain Text (no auto breakpoint)"),
        }
    }

    // GIVEN une liste d'outils
    // WHEN apply_cache_breakpoints() est appelé
    // THEN le dernier outil a cache_control: ephemeral
    #[test]
    fn test_apply_cache_breakpoints_last_tool() {
        let mut system: Option<AnthropicSystem> = None;
        let mut messages: Vec<AnthropicMessage> = Vec::new();
        let mut tools = Some(vec![
            AnthropicTool {
                name: "tool_a".to_string(),
                description: "first".to_string(),
                input_schema: json!({}),
                cache_control: None,
            },
            AnthropicTool {
                name: "tool_b".to_string(),
                description: "last".to_string(),
                input_schema: json!({}),
                cache_control: None,
            },
        ]);

        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

        let tools_vec = tools.as_ref().unwrap();
        assert!(
            tools_vec[0].cache_control.is_none(),
            "tool_a must not be marked"
        );
        assert!(
            tools_vec[1].cache_control.is_some(),
            "tool_b (last) must be marked"
        );
    }

    // GIVEN un message avec ChatMessage.cache_control = Some(Ephemeral)
    // WHEN convert_message() est appelé
    // THEN le contenu texte est converti en bloc avec cache_control: ephemeral
    #[test]
    fn test_convert_message_respects_cache_control() {
        use crate::types::ChatMessage;

        let mut msg = ChatMessage::user("hello");
        msg.cache_control = Some(CacheControl::Ephemeral);

        let anthropic_msg = convert_message(&msg).expect("must convert");

        match &anthropic_msg.content {
            AnthropicContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                assert!(
                    matches!(
                        &blocks[0],
                        AnthropicBlock::Text {
                            cache_control: Some(_),
                            ..
                        }
                    ),
                    "user message with cache_control must produce a cached text block"
                );
            }
            _ => panic!("expected Blocks content for cached user message"),
        }
    }

    // GIVEN un message sans cache_control
    // WHEN convert_message() est appelé
    // THEN le contenu est Text simple (pas de blocs)
    #[test]
    fn test_convert_message_no_cache_control_stays_text() {
        use crate::types::ChatMessage;

        let msg = ChatMessage::user("hello");
        let anthropic_msg = convert_message(&msg).expect("must convert");

        assert!(
            matches!(anthropic_msg.content, AnthropicContent::Text(_)),
            "user message without cache_control must remain Text"
        );
    }

    // GIVEN AnthropicCacheControl::ephemeral()
    // WHEN sérialisé
    // THEN produit {"type": "ephemeral"}
    #[test]
    fn test_anthropic_cache_control_serialization() {
        let cc = AnthropicCacheControl::ephemeral();
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(json["type"], "ephemeral");
    }
}
