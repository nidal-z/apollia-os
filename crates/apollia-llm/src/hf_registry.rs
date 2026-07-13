//! HuggingFace Hub API client for discovering and downloading GGUF models.
//!
//! Uses the public HF API (no authentication for public models). An optional
//! token allows access to gated models (Llama 3.1, etc.).
//!
//! Architecture:
//! - [`HfRegistryClient`]: main HTTP client
//! - [`HfModelCard`]: metadata for an HF model
//! - [`HfFile`]: a file in an HF repo (with size for the compatibility badges)
//! - [`HfSearchFilter`]: search filters (format, sort, etc.)

#![cfg(feature = "cloud")]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hardware::{CompatibilityBadge, HardwareProfile};

/// Errors of the HuggingFace client.
#[derive(Debug, Error)]
pub enum HfError {
    /// HTTP network error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialization error.
    #[error("json parse error: {0}")]
    Json(String),

    /// Model not found on HuggingFace.
    #[error("model not found: {0}")]
    NotFound(String),

    /// The model is gated and requires an HF token.
    #[error("model '{0}' is gated - a HuggingFace token is required")]
    Gated(String),
}

/// Metadata card for a HuggingFace model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModelCard {
    /// Repo identifier (e.g. `"bartowski/Qwen3-30B-A3B-GGUF"`).
    pub repo_id: String,
    /// Author / organization.
    pub author: Option<String>,
    /// Download count (rolling 30 days).
    pub downloads: u64,
    /// Number of likes.
    pub likes: u64,
    /// SPDX license (e.g. `"apache-2.0"`, `"mit"`).
    pub license: Option<String>,
    /// HF tags (e.g. `["gguf", "qwen3", "text-generation"]`).
    pub tags: Vec<String>,
    /// `true` if the model requires an HF token to download.
    pub gated: bool,
    /// Total repo size in bytes (sum of all files).
    pub total_size_bytes: Option<u64>,
    /// GGUF files available in this repo.
    pub gguf_files: Vec<HfFile>,
    /// Recommended generation parameters (from `generation_config.json`).
    pub generation_config: Option<GenerationConfig>,
    /// Detected compatibility issue (pipeline_tag or model_type).
    pub compatibility_issue: Option<CompatIssue>,
    /// HF model type from `config.json` (e.g. `"LlamaForCausalLM"`).
    pub model_type: Option<String>,
}

/// Reasons a model is incompatible with llama.cpp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatIssue {
    /// Embedding or feature-extraction model, not suited to text generation.
    EmbeddingModel,
    /// Architecture in `config.json` absent from the list supported by llama.cpp.
    UnknownArchitecture,
    /// No GGUF file found in this repo.
    NoGgufFiles,
}

/// A file in a HuggingFace repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFile {
    /// File name (e.g. `"Qwen3-30B-A3B-Q4_K_M.gguf"`).
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Human-readable size (e.g. `"17.2 GB"`).
    pub size_human: String,
    /// Compatibility badge computed against the local hardware.
    pub compatibility: Option<CompatibilityBadge>,
    /// Direct download URL.
    pub download_url: String,
}

/// Recommended generation parameters from `generation_config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f64>,
    pub max_new_tokens: Option<u32>,
}

/// Filters for HF model search.
#[derive(Debug, Clone, Default)]
pub struct HfSearchFilter {
    /// Filter by tag (e.g. `"gguf"`).
    pub filter: Option<String>,
    /// Sort: `"downloads"` (default), `"likes"`, `"trending"`, `"createdAt"`.
    pub sort: Option<String>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Filter by HF task (e.g. `"text-generation"`).
    pub pipeline_tag: Option<String>,
    /// Filter by language (e.g. `"fr"`, `"en"`, `"zh"`).
    pub language: Option<String>,
    /// "Load more" cursor: the full URL returned by the HF API in the
    /// `Link: rel="next"` header. When set, it replaces all other parameters
    /// (the request is already encoded in the URL).
    pub next_cursor: Option<String>,
}

/// Result of one HF search page.
#[derive(Debug, Clone, Serialize)]
pub struct SearchPage {
    /// Models in this page.
    pub models: Vec<HfModelCard>,
    /// URL of the next page (`Link: rel="next"` header); `None` on the last page.
    pub next_cursor: Option<String>,
}

// ── Compatibility detection ───────────────────────────────────────────────

/// `model_type` architectures llama.cpp guarantees support for.
/// Source: `src/llama.cpp` (LLAMA_MODEL_ARCH), updated on each crate bump.
static LLAMA_CPP_SUPPORTED_MODEL_TYPES: &[&str] = &[
    // LLaMA family
    "LlamaForCausalLM",
    "MistralForCausalLM",
    "MixtralForCausalLM",
    "LlamaForConditionalGeneration",
    "Llama4ForConditionalGeneration",
    // Qwen family
    "Qwen2ForCausalLM",
    "Qwen2MoeForCausalLM",
    "Qwen3ForCausalLM",
    "Qwen3MoeForCausalLM",
    // Phi family
    "PhiForCausalLM",
    "Phi3ForCausalLM",
    "Phi3SmallForCausalLM",
    // Gemma family
    "GemmaForCausalLM",
    "Gemma2ForCausalLM",
    "Gemma3ForCausalLM",
    "RecurrentGemmaForCausalLM",
    // DeepSeek family
    "DeepseekV2ForCausalLM",
    "DeepseekV3ForCausalLM",
    // Others
    "FalconForCausalLM",
    "MPTForCausalLM",
    "GPT2LMHeadModel",
    "GPTNeoXForCausalLM",
    "GPTJForCausalLM",
    "BloomForCausalLM",
    "InternLM2ForCausalLM",
    "InternLM3ForCausalLM",
    "BaichuanForCausalLM",
    "YiForCausalLM",
    "CohereForCausalLM",
    "ExaoneForCausalLM",
    "GraniteForCausalLM",
    "GraniteMoeForCausalLM",
    "ChatGLMModel",
    "GLM4ForCausalLM",
    "StableLMForCausalLM",
    "StarCoder2ForCausalLM",
    "OlmoForCausalLM",
    "OlmoeForCausalLM",
    "ArcticForCausalLM",
    "DbrxForCausalLM",
    "JambaForCausalLM",
    "MambaForCausalLM",
    "SmolLMForCausalLM",
    "Zamba2ForCausalLM",
    "OpenELMForCausalLM",
    "NemotronForCausalLM",
    "MiniCPM3ForCausalLM",
];

fn compat_from_pipeline_tag(pipeline_tag: Option<&str>) -> Option<CompatIssue> {
    match pipeline_tag {
        Some(
            "feature-extraction"
            | "sentence-similarity"
            | "fill-mask"
            | "text-classification"
            | "token-classification"
            | "question-answering",
        ) => Some(CompatIssue::EmbeddingModel),
        Some(
            "text-to-image"
            | "image-to-text"
            | "automatic-speech-recognition"
            | "text-to-speech"
            | "text-to-video"
            | "image-classification"
            | "object-detection",
        ) => Some(CompatIssue::EmbeddingModel),
        _ => None,
    }
}

fn compat_from_model_type(model_type: &str) -> Option<CompatIssue> {
    // Suffixes that indicate a non-generative model
    if model_type.ends_with("ForMaskedLM")
        || model_type.ends_with("ForSequenceClassification")
        || model_type.ends_with("ForTokenClassification")
        || model_type.ends_with("ForQuestionAnswering")
        || model_type.ends_with("ForMultipleChoice")
        || model_type.contains("Whisper")
        || model_type.contains("CLIP")
        || model_type.contains("ViT")
        || model_type.contains("Diffusion")
    {
        return Some(CompatIssue::EmbeddingModel);
    }
    if LLAMA_CPP_SUPPORTED_MODEL_TYPES.contains(&model_type) {
        return None; // known and supported architecture
    }
    Some(CompatIssue::UnknownArchitecture)
}

async fn extract_model_type(resp: Result<reqwest::Response, reqwest::Error>) -> Option<String> {
    let r = resp.ok()?;
    if !r.status().is_success() {
        return None;
    }
    let json = r.json::<serde_json::Value>().await.ok()?;
    json.get("model_type")
        .and_then(|v| v.as_str())
        .map(String::from)
}

// ── Model type cache (session-level, TTL 24h) ─────────────────────────────

struct CachedModelType {
    model_type: Option<String>,
    cached_at: std::time::Instant,
}

/// Session cache of HF model types (from `config.json`).
///
/// Avoids refetching `config.json` each time a card is opened. TTL 24h,
/// stored as Tauri state (`Arc<HfModelTypeCache>`).
pub struct HfModelTypeCache {
    entries: tokio::sync::RwLock<std::collections::HashMap<String, CachedModelType>>,
}

const MODEL_TYPE_CACHE_TTL_SECS: u64 = 86_400;

impl Default for HfModelTypeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl HfModelTypeCache {
    pub fn new() -> Self {
        Self {
            entries: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    async fn get(&self, repo_id: &str) -> Option<Option<String>> {
        let guard = self.entries.read().await;
        guard.get(repo_id).and_then(|e| {
            if e.cached_at.elapsed().as_secs() < MODEL_TYPE_CACHE_TTL_SECS {
                Some(e.model_type.clone())
            } else {
                None
            }
        })
    }

    async fn set(&self, repo_id: &str, model_type: Option<String>) {
        let mut guard = self.entries.write().await;
        guard.insert(
            repo_id.to_string(),
            CachedModelType {
                model_type,
                cached_at: std::time::Instant::now(),
            },
        );
    }
}

// ── Internal types ────────────────────────────────────────────────────────

/// Entry returned by `/api/models/{id}/tree/main`.
#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    size: u64,
    #[serde(rename = "type")]
    entry_type: String,
}

// ── Client ────────────────────────────────────────────────────────────────

const HF_API_BASE: &str = "https://huggingface.co/api";
const HF_CDN_BASE: &str = "https://huggingface.co";

/// HuggingFace Hub client: public API plus an optional token for gated models.
pub struct HfRegistryClient {
    client: reqwest::Client,
    token: Option<String>,
}

impl HfRegistryClient {
    /// Create a new client. `token` is optional (only for gated models).
    pub fn new(token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Apollia-OS/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build never fails with valid config");
        Self { client, token }
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = &self.token {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }
        headers
    }

    /// Search GGUF models on HuggingFace.
    ///
    /// Returns a [`SearchPage`] with the models and a `next_cursor` for
    /// pagination. The cursor is extracted from the HTTP `Link: rel="next"`
    /// header and holds the full URL of the next page. Pass this cursor in
    /// `filter.next_cursor` to load the next page.
    ///
    /// # Errors
    /// [`HfError::Http`] if the request fails.
    pub async fn search(
        &self,
        query: &str,
        filter: HfSearchFilter,
        hardware: Option<&HardwareProfile>,
    ) -> Result<SearchPage, HfError> {
        // If a cursor is provided, use it directly (the URL already encodes all filters).
        let url = match filter.next_cursor.as_ref() {
            Some(cursor) => cursor.clone(),
            None => build_search_url(query, &filter),
        };

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        // Extract the pagination cursor from the `Link: <url>; rel="next"` header.
        let next_cursor = parse_next_link(resp.headers());

        let json = resp.json::<serde_json::Value>().await?;
        let raw_models = json.as_array().cloned().unwrap_or_default();

        let mut cards = Vec::new();
        for m in raw_models {
            if let Some(card) = parse_model_list_item(&m, hardware) {
                cards.push(card);
            }
        }

        Ok(SearchPage {
            models: cards,
            next_cursor,
        })
    }

    /// Fetch the full metadata of a model (including the list of GGUF files).
    ///
    /// Issues two requests in parallel:
    /// 1. `/api/models/{id}`: metadata (downloads, likes, tags, license, gated)
    /// 2. `/api/models/{id}/tree/main`: real sizes of the LFS files
    ///
    /// # Errors
    /// - [`HfError::NotFound`] if the repo does not exist.
    /// - [`HfError::Gated`] if the model is gated and no token is provided.
    pub async fn get_model(
        &self,
        repo_id: &str,
        hardware: Option<&HardwareProfile>,
        type_cache: Option<&HfModelTypeCache>,
    ) -> Result<HfModelCard, HfError> {
        let meta_url = format!("{HF_API_BASE}/models/{repo_id}");
        let tree_url = format!("{HF_API_BASE}/models/{repo_id}/tree/main?recursive=true");
        let config_url = format!("{HF_CDN_BASE}/{repo_id}/resolve/main/config.json");

        // Check cache before firing the config.json request.
        let cached_model_type = if let Some(cache) = type_cache {
            cache.get(repo_id).await
        } else {
            None
        };

        // Fire meta + tree always; config.json only on cache miss.
        let (meta_resp, tree_resp, config_resp) = tokio::join!(
            self.client
                .get(&meta_url)
                .headers(self.auth_headers())
                .send(),
            self.client
                .get(&tree_url)
                .headers(self.auth_headers())
                .send(),
            async {
                if cached_model_type.is_some() {
                    return Ok(None::<reqwest::Response>);
                }
                self.client
                    .get(&config_url)
                    .headers(self.auth_headers())
                    .send()
                    .await
                    .map(Some)
            },
        );

        let meta_resp = meta_resp?;
        if meta_resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HfError::NotFound(repo_id.to_string()));
        }
        if meta_resp.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(HfError::Gated(repo_id.to_string()));
        }
        let meta_json = meta_resp.json::<serde_json::Value>().await?;

        // Tree entries: path → size. Best-effort (empty on failure).
        let tree_entries: Vec<TreeEntry> = match tree_resp {
            Ok(r) if r.status().is_success() => {
                r.json::<Vec<TreeEntry>>().await.unwrap_or_default()
            }
            _ => vec![],
        };

        let mut card = parse_model_list_item(&meta_json, None)
            .ok_or_else(|| HfError::Json(format!("failed to parse model {repo_id}")))?;

        // Build gguf_files from tree (sizes are accurate here).
        card.gguf_files = build_gguf_files(&tree_entries, repo_id, hardware);

        // Re-evaluate NoGgufFiles based on authoritative tree data.
        reevaluate_gguf_compat(&mut card);

        // Determine model_type: prefer cache, then config_resp.
        let model_type =
            resolve_model_type(cached_model_type, config_resp, type_cache, repo_id).await;

        card.model_type = model_type.clone();

        // Refine compat with model_type only if pipeline_tag didn't already flag an issue.
        if card.compatibility_issue.is_none() {
            if let Some(ref mt) = model_type {
                card.compatibility_issue = compat_from_model_type(mt);
            }
        }

        Ok(card)
    }

    /// Resolve the `base_model` declared by a derived repo (Bartowski/Unsloth/
    /// mradermacher quantizations, fine-tunes, merges).
    ///
    /// Returns `Some("org/name")` when the repo declares a parent via
    /// `cardData.base_model` (structured field) or a `base_model:org/name` tag.
    /// `None` if the repo is first-order or the field is absent.
    ///
    /// Needed to fetch a `generation_config.json` when the downloaded repo is a
    /// GGUF derivation: quantizers do not republish the config file (it comes
    /// from the upstream repo).
    pub async fn resolve_base_model(&self, repo_id: &str) -> Option<String> {
        let url = format!("{HF_API_BASE}/models/{repo_id}");
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let json = resp.json::<serde_json::Value>().await.ok()?;
        extract_base_model_from_json(&json)
    }

    /// Fetch the recommended generation parameters from `generation_config.json`.
    ///
    /// Returns `None` if the file is absent (model with no explicit generation config).
    pub async fn get_generation_config(&self, repo_id: &str) -> Option<GenerationConfig> {
        let url = format!("{HF_CDN_BASE}/{repo_id}/resolve/main/generation_config.json");
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let json = resp.json::<serde_json::Value>().await.ok()?;
        Some(GenerationConfig {
            temperature: json.get("temperature").and_then(|v| v.as_f64()),
            top_p: json.get("top_p").and_then(|v| v.as_f64()),
            top_k: json.get("top_k").and_then(|v| v.as_u64()).map(|v| v as u32),
            repetition_penalty: json.get("repetition_penalty").and_then(|v| v.as_f64()),
            max_new_tokens: json
                .get("max_new_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        })
    }
}

// ── Pagination helper ─────────────────────────────────────────────────────

/// Extract the `base_model` (org/name) from the `/api/models/{id}` response.
///
/// Two-pass strategy:
/// 1. `cardData.base_model` (string or array): the structured field HF exposes
///    when the model card YAML declares `base_model: foo/bar`.
/// 2. `base_model:org/name` tags: fallback for repos that only set tags. An
///    *unqualified* tag (`base_model:Qwen/X`) is preferred since it points
///    directly upstream. If only a qualified tag exists
///    (`base_model:quantized:Qwen/X`), the org/name after the second `:` is
///    extracted.
fn extract_base_model_from_json(json: &serde_json::Value) -> Option<String> {
    if let Some(from_card) = base_model_from_card_data(json) {
        return Some(from_card);
    }

    let tags: Vec<&str> = json
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();

    base_model_from_unqualified_tag(&tags).or_else(|| base_model_from_qualified_tag(&tags))
}

/// Pass 1: `cardData.base_model` (string or array).
fn base_model_from_card_data(json: &serde_json::Value) -> Option<String> {
    let v = json.get("cardData").and_then(|c| c.get("base_model"))?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.as_array()
        .and_then(|arr| arr.iter().find_map(|x| x.as_str()))
        .map(str::to_string)
}

/// Pass 2a: unqualified `base_model:org/name` tag (points directly upstream).
fn base_model_from_unqualified_tag(tags: &[&str]) -> Option<String> {
    tags.iter().find_map(|t| {
        let rest = t.strip_prefix("base_model:")?;
        if !rest.contains(':') && rest.contains('/') {
            Some(rest.to_string())
        } else {
            None
        }
    })
}

/// Pass 2b: qualified `base_model:quantized:org/name` tag; extract the
/// org/name after the second `:`.
fn base_model_from_qualified_tag(tags: &[&str]) -> Option<String> {
    tags.iter().find_map(|t| {
        let rest = t.strip_prefix("base_model:")?;
        let colon = rest.find(':')?;
        let upstream = &rest[colon + 1..];
        if upstream.contains('/') {
            Some(upstream.to_string())
        } else {
            None
        }
    })
}

/// Build the list of GGUF files from the repo's tree entries.
/// Sizes are accurate here (from the tree API).
fn build_gguf_files(
    tree_entries: &[TreeEntry],
    repo_id: &str,
    hardware: Option<&HardwareProfile>,
) -> Vec<HfFile> {
    tree_entries
        .iter()
        .filter(|e| e.entry_type == "file" && e.path.ends_with(".gguf"))
        .map(|e| {
            let size_bytes = e.size;
            let size_gb = size_bytes as f64 / 1_073_741_824.0;
            let compatibility = hardware.map(|hw| CompatibilityBadge::compute(size_gb, hw));
            HfFile {
                filename: e.path.clone(),
                size_bytes,
                size_human: format_size(size_bytes),
                compatibility,
                download_url: format!("{HF_CDN_BASE}/{repo_id}/resolve/main/{}", e.path),
            }
        })
        .collect()
}

/// Re-evaluate `NoGgufFiles` from the authoritative tree data.
fn reevaluate_gguf_compat(card: &mut HfModelCard) {
    match &card.compatibility_issue {
        None if card.gguf_files.is_empty() => {
            card.compatibility_issue = Some(CompatIssue::NoGgufFiles);
        }
        Some(CompatIssue::NoGgufFiles) if !card.gguf_files.is_empty() => {
            card.compatibility_issue = None;
        }
        _ => {}
    }
}

/// Determine the `model_type`: prefer the cache, then `config.json`.
/// On a miss, populate the cache with the resolved value.
async fn resolve_model_type(
    cached_model_type: Option<Option<String>>,
    config_resp: Result<Option<reqwest::Response>, reqwest::Error>,
    type_cache: Option<&HfModelTypeCache>,
    repo_id: &str,
) -> Option<String> {
    if let Some(cached) = cached_model_type {
        return cached;
    }
    let mt = match config_resp {
        Ok(Some(r)) => extract_model_type(Ok(r)).await,
        _ => None,
    };
    if let Some(cache) = type_cache {
        cache.set(repo_id, mt.clone()).await;
    }
    mt
}

/// Build the `/api/models` search URL from the query and filters.
fn build_search_url(query: &str, filter: &HfSearchFilter) -> String {
    let sort = filter.sort.as_deref().unwrap_or("downloads");
    let tag = filter.filter.as_deref().unwrap_or("gguf");
    let limit = filter.limit.unwrap_or(50);

    let mut u = format!("{HF_API_BASE}/models?filter={tag}&sort={sort}&limit={limit}&full=false");
    if !query.is_empty() {
        // URL-encode the query to handle spaces and special chars.
        let encoded: String = query
            .chars()
            .flat_map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    vec![c]
                } else {
                    format!("%{:02X}", c as u32).chars().collect::<Vec<_>>()
                }
            })
            .collect();
        u.push_str(&format!("&search={encoded}"));
    }
    if let Some(pt) = filter.pipeline_tag.as_deref() {
        u.push_str(&format!("&pipeline_tag={pt}"));
    }
    if let Some(lang) = filter.language.as_deref() {
        u.push_str(&format!("&language={lang}"));
    }
    u
}

/// Extract the `rel="next"` URL from HuggingFace's HTTP `Link` header.
///
/// Format: `Link: <https://huggingface.co/api/models?...>; rel="next", <...>; rel="first"`
fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get("Link")?.to_str().ok()?;
    for part in link.split(',') {
        let part = part.trim();
        if part.contains(r#"rel="next""#) {
            if let (Some(start), Some(end)) = (part.find('<'), part.find('>')) {
                return Some(part[start + 1..end].to_string());
            }
        }
    }
    None
}

// ── Parsing helpers ───────────────────────────────────────────────────────

/// Extract the basic metadata of a model from the HF JSON response.
/// Does NOT include file sizes (available only via the tree API).
fn parse_model_list_item(
    json: &serde_json::Value,
    _hardware: Option<&HardwareProfile>,
) -> Option<HfModelCard> {
    let repo_id = json["modelId"]
        .as_str()
        .or_else(|| json["id"].as_str())?
        .to_string();
    let author = json["author"].as_str().map(String::from);
    let downloads = json["downloads"].as_u64().unwrap_or(0);
    let likes = json["likes"].as_u64().unwrap_or(0);
    let gated = json["gated"].as_bool().unwrap_or(false);
    let tags: Vec<String> = json["tags"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let pipeline_tag = json.get("pipeline_tag").and_then(|v| v.as_str());

    // License: try explicit fields first, then extract from "license:xxx" tags.
    let license = json["cardData"]["license"]
        .as_str()
        .or_else(|| json["license"].as_str())
        .map(String::from)
        .or_else(|| {
            tags.iter()
                .find(|t| t.starts_with("license:"))
                .map(|t| t["license:".len()..].to_string())
        });

    // In search results, siblings only carry rfilename (no LFS sizes).
    // Populate gguf_files with filenames only; sizes come from the tree API.
    let gguf_files: Vec<HfFile> = json["siblings"]
        .as_array()
        .map(|siblings| {
            siblings
                .iter()
                .filter_map(|f| {
                    let name = f["rfilename"].as_str()?;
                    if !name.ends_with(".gguf") {
                        return None;
                    }
                    Some(HfFile {
                        filename: name.to_string(),
                        size_bytes: 0,
                        size_human: String::new(),
                        compatibility: None,
                        download_url: format!("{HF_CDN_BASE}/{repo_id}/resolve/main/{name}"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut compatibility_issue = compat_from_pipeline_tag(pipeline_tag);
    if compatibility_issue.is_none() && gguf_files.is_empty() {
        compatibility_issue = Some(CompatIssue::NoGgufFiles);
    }

    Some(HfModelCard {
        repo_id,
        author,
        downloads,
        likes,
        license,
        tags,
        gated,
        total_size_bytes: None,
        gguf_files,
        generation_config: None,
        compatibility_issue,
        model_type: None,
    })
}

fn format_size(bytes: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base_model_from_card_data_string() {
        // GIVEN: a Bartowski-style payload with cardData.base_model
        let payload = json!({
            "cardData": { "base_model": "Qwen/Qwen2.5-Coder-7B-Instruct" },
            "tags": ["gguf"]
        });
        // THEN: extracted directly
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Qwen2.5-Coder-7B-Instruct")
        );
    }

    #[test]
    fn base_model_from_card_data_array_takes_first() {
        let payload = json!({
            "cardData": { "base_model": ["Qwen/A", "Meta/B"] },
        });
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/A")
        );
    }

    #[test]
    fn base_model_from_simple_tag_when_card_data_missing() {
        // GIVEN: no cardData.base_model but a clean tag
        let payload = json!({
            "tags": ["gguf", "base_model:Qwen/Qwen2.5-Coder-7B-Instruct"]
        });
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Qwen2.5-Coder-7B-Instruct")
        );
    }

    #[test]
    fn base_model_from_qualified_tag_as_fallback() {
        // GIVEN: only a qualified tag (Bartowski real-world payload)
        let payload = json!({
            "tags": [
                "gguf",
                "base_model:quantized:Qwen/Qwen2.5-Coder-7B-Instruct"
            ]
        });
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Qwen2.5-Coder-7B-Instruct")
        );
    }

    #[test]
    fn base_model_simple_tag_wins_over_qualified() {
        let payload = json!({
            "tags": [
                "base_model:quantized:Other/Wrong",
                "base_model:Qwen/Right"
            ]
        });
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Right")
        );
    }

    #[test]
    fn base_model_returns_none_when_neither_present() {
        let payload = json!({ "tags": ["gguf", "license:apache-2.0"] });
        assert!(extract_base_model_from_json(&payload).is_none());
    }

    #[test]
    fn base_model_ignores_malformed_tag_without_slash() {
        // GIVEN: a tag that's `base_model:something` but without org/name
        let payload = json!({ "tags": ["base_model:notavalidrepo"] });
        assert!(extract_base_model_from_json(&payload).is_none());
    }
}
