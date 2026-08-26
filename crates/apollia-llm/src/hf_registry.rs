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

mod compat;
mod parse;

use compat::compat_from_model_type;
pub use compat::HfModelTypeCache;
use parse::{
    build_gguf_files, build_search_url, extract_base_model_from_json, parse_model_list_item,
    parse_next_link, reevaluate_gguf_compat, resolve_model_type,
};

/// Errors of the HuggingFace client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HfError {
    /// HTTP network error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialization error.
    #[error("json parse error: {0}")]
    Json(String),

    /// The response body was refused before being buffered.
    #[error("body error: {0}")]
    Body(String),

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

// ── Model type cache (session-level, TTL 24h) ─────────────────────────────

// ── Internal types ────────────────────────────────────────────────────────

/// Entry returned by `/api/models/{id}/tree/main`.
#[derive(Debug, Deserialize)]
pub(crate) struct TreeEntry {
    path: String,
    size: u64,
    #[serde(rename = "type")]
    entry_type: String,
}

// ── Client ────────────────────────────────────────────────────────────────

pub(crate) const HF_API_BASE: &str = "https://huggingface.co/api";
pub(crate) const HF_CDN_BASE: &str = "https://huggingface.co";

/// Cap for every HuggingFace JSON answer. A model listing page is a few tens of
/// kilobytes and a repository tree a few hundred; 8 MiB is a ceiling, not a
/// budget, and it exists so a hostile or broken answer cannot be buffered whole.
const HF_MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// HuggingFace Hub client: public API plus an optional token for gated models.
pub struct HfRegistryClient {
    client: reqwest::Client,
    token: Option<String>,
}

impl HfRegistryClient {
    /// Create a new client. `token` is optional (only for gated models).
    pub fn new(token: Option<String>) -> Self {
        let client = apollia_core::net::safe_client_builder()
            .user_agent("Apollia-OS/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            // SAFETY: the only failure `ClientBuilder::build` reports is a TLS
            // backend that will not initialise; every setting above it is a
            // literal fixed in this file.
            .expect("the TLS backend failed to initialise");
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

        let json: serde_json::Value = apollia_core::net::read_capped_json(resp, HF_MAX_JSON_BYTES)
            .await
            .map_err(|e| HfError::Body(e.to_string()))?;
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
        let meta_json: serde_json::Value =
            apollia_core::net::read_capped_json(meta_resp, HF_MAX_JSON_BYTES)
                .await
                .map_err(|e| HfError::Body(e.to_string()))?;

        // Tree entries: path → size. Best-effort (empty on failure).
        let tree_entries: Vec<TreeEntry> = match tree_resp {
            Ok(r) if r.status().is_success() => {
                apollia_core::net::read_capped_json(r, HF_MAX_JSON_BYTES)
                    .await
                    .unwrap_or_default()
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
        let json: serde_json::Value = apollia_core::net::read_capped_json(resp, HF_MAX_JSON_BYTES)
            .await
            .ok()?;
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

        let json: serde_json::Value = apollia_core::net::read_capped_json(resp, HF_MAX_JSON_BYTES)
            .await
            .ok()?;
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

// ── Parsing helpers ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base_model_from_card_data_string() {
        // GIVEN a payload carrying cardData.base_model as a string
        let payload = json!({
            "cardData": { "base_model": "Qwen/Qwen2.5-Coder-7B-Instruct" },
            "tags": ["gguf"]
        });
        // WHEN the base model is extracted
        // THEN the card data value is used directly
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Qwen2.5-Coder-7B-Instruct")
        );
    }

    #[test]
    fn base_model_from_card_data_array_takes_first() {
        // GIVEN a payload whose cardData.base_model is an array
        let payload = json!({
            "cardData": { "base_model": ["Qwen/A", "Meta/B"] },
        });
        // WHEN the base model is extracted
        // THEN the first entry is taken
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
        // WHEN the base model is extracted
        // THEN the tag is read instead of the missing card data
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
        // WHEN the base model is extracted
        // THEN the qualified tag is read, and its qualifier dropped
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Qwen2.5-Coder-7B-Instruct")
        );
    }

    #[test]
    fn base_model_simple_tag_wins_over_qualified() {
        // GIVEN a payload carrying both a qualified tag and a plain one
        let payload = json!({
            "tags": [
                "base_model:quantized:Other/Wrong",
                "base_model:Qwen/Right"
            ]
        });
        // WHEN the base model is extracted
        // THEN the plain tag wins over the qualified one
        assert_eq!(
            extract_base_model_from_json(&payload).as_deref(),
            Some("Qwen/Right")
        );
    }

    #[test]
    fn base_model_returns_none_when_neither_present() {
        // GIVEN a payload with neither card data nor a base model tag
        let payload = json!({ "tags": ["gguf", "license:apache-2.0"] });
        // WHEN the base model is extracted
        // THEN nothing comes back
        assert!(extract_base_model_from_json(&payload).is_none());
    }

    #[test]
    fn base_model_ignores_malformed_tag_without_slash() {
        // GIVEN: a tag that's `base_model:something` but without org/name
        let payload = json!({ "tags": ["base_model:notavalidrepo"] });
        // WHEN the base model is extracted
        // THEN the malformed tag is ignored rather than taken for a repo id
        assert!(extract_base_model_from_json(&payload).is_none());
    }
}
