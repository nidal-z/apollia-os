//! HuggingFace Hub API client for discovering and downloading GGUF models.
//!
//! Uses the public HF API (no authentication for public models). An optional
//! token allows access to gated models (Llama 3.1, etc.).
//!
//! Architecture:
//! - [`HfRegistryClient`]: main HTTP client
//! - [`HfModelCard`]: metadata for an HF model
//! - [`HfFile`]: a file in an HF repo (with size for the compatibility badges)
//! - [`BoundedSearch`]: a search bounded to the official publishers and the
//!   recognised converters, one request per organisation
//! - [`GeneratedCard`]: the card Apollia builds for each search result

#![cfg(feature = "cloud")]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hardware::{CompatibilityBadge, HardwareProfile};

mod bounded;
mod card;
mod compat;
mod generated;
mod parse;
mod variant;

pub use bounded::{
    search_authors, selection_repos, BoundedPage, BoundedSearch, BoundedSearchError, FoundModel,
    SearchAuthor, SearchContext, SearchFormat, SearchInputs, SearchSort,
};
pub use generated::GeneratedCard;

use compat::compat_from_model_type;
pub use compat::HfModelTypeCache;
use parse::{
    build_gguf_files, extract_base_model_from_json, lfs_sha256_by_path, parse_model_list_item,
    reevaluate_gguf_compat, resolve_config_facts,
};
use variant::plan_variant;
pub use variant::{VariantError, VariantFile, VariantPlan, VariantSelector};

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

    /// The repository tree publishes no SHA-256 for this file, so its
    /// download cannot be verified.
    #[error("no published sha256 for '{path}' in repository '{repo_id}'")]
    NoPublishedSha256 {
        /// Repository (`org/name`).
        repo_id: String,
        /// Path of the file inside the repository.
        path: String,
    },

    /// The repository tree holds no complete variant for the request.
    #[error(transparent)]
    Variant(#[from] VariantError),
}

/// Metadata card for a HuggingFace model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModelCard {
    /// Repo identifier (e.g. `"bartowski/Qwen3-30B-A3B-GGUF"`).
    pub repo_id: String,
    /// Author / organization.
    pub author: Option<String>,
    /// Whether the owner is an official publisher, a recognised converter, or
    /// neither (`None`), read from the lists embedded with the curated table.
    #[serde(default)]
    pub publisher_kind: Option<crate::recommend::PublisherKind>,
    /// Download count (rolling 30 days).
    pub downloads: u64,
    /// Number of likes.
    pub likes: u64,
    /// When the repository last changed, as the Hub reports it (RFC 3339).
    #[serde(default)]
    pub last_modified: Option<String>,
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
    /// What the model can do: tools and reasoning from its chat template,
    /// vision when the repository publishes a projector (GGUF) or declares a
    /// `vision_config` (MLX). A search result carries vision alone.
    #[serde(default)]
    pub capabilities: crate::capabilities::Capabilities,
    /// The short list of variants, at most one per level, each sized and
    /// graded for this machine. Empty on a search result, which has no sizes.
    #[serde(default)]
    pub variants: Vec<crate::recommend::ModelVariant>,
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
    /// SHA-256 of the file content, lowercase hex, as HuggingFace publishes it
    /// for every LFS file (`lfs.oid` in the repository tree). `None` for a
    /// file that is not stored through LFS, or when the tree was not read (a
    /// search result only lists names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Quantisation the file name declares (`Q4_K_M`, `BF16`), read here so
    /// no surface has to parse a file name. `None` for a projector and a name
    /// that declares none.
    #[serde(default)]
    pub quantization: Option<String>,
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
    /// Git object identifier: for a file committed outside LFS, the SHA-1 of
    /// the blob (`blob <size>\0` then the content); for an LFS file, the
    /// identifier of its pointer, never of its content.
    #[serde(default)]
    oid: Option<String>,
    /// Present only for a file stored through LFS.
    #[serde(default)]
    lfs: Option<TreeLfs>,
}

/// The `lfs` object of a tree entry: `oid` is the SHA-256 of the content.
#[derive(Debug, Deserialize)]
pub(crate) struct TreeLfs {
    oid: String,
}

// ── Client ────────────────────────────────────────────────────────────────

pub(crate) const HF_CDN_BASE: &str = "https://huggingface.co";

/// Cap for every HuggingFace JSON answer. A model listing page is a few tens of
/// kilobytes and a repository tree a few hundred; 8 MiB is a ceiling, not a
/// budget, and it exists so a hostile or broken answer cannot be buffered whole.
const HF_MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// HuggingFace Hub client: public API plus an optional token for gated models.
pub struct HfRegistryClient {
    client: reqwest::Client,
    token: Option<String>,
    /// Root of the JSON API, `https://huggingface.co/api` unless overridden.
    api_base: String,
    /// Root of the file host, `https://huggingface.co` unless overridden.
    cdn_base: String,
}

impl HfRegistryClient {
    /// Create a new client. `token` is optional (only for gated models).
    pub fn new(token: Option<String>) -> Self {
        Self::with_hub_base(token, HF_CDN_BASE)
    }

    /// Create a client addressing a Hub at `base` rather than HuggingFace, for
    /// a mirror or a test server. `base` is the host root, without `/api`.
    pub fn with_hub_base(token: Option<String>, base: &str) -> Self {
        let base = base.trim_end_matches('/');
        let client = apollia_core::net::safe_client_builder()
            .user_agent("Apollia-OS/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            // SAFETY: the only failure `ClientBuilder::build` reports is a TLS
            // backend that will not initialise; every setting above it is a
            // literal fixed in this file.
            .expect("the TLS backend failed to initialise");
        Self {
            client,
            token,
            api_base: format!("{base}/api"),
            cdn_base: base.to_owned(),
        }
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
        let api = &self.api_base;
        let meta_url = format!("{api}/models/{repo_id}");
        let tree_url = format!("{api}/models/{repo_id}/tree/main?recursive=true");
        let config_url = format!("{}/{repo_id}/resolve/main/config.json", self.cdn_base);

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

        // What config.json says: prefer cache, then config_resp.
        let config = resolve_config_facts(cached_model_type, config_resp, type_cache, repo_id)
            .await
            .unwrap_or_default();

        card.model_type = config.model_type.clone();

        // Refine compat with model_type only if pipeline_tag didn't already flag an issue.
        if card.compatibility_issue.is_none() {
            if let Some(ref mt) = config.model_type {
                card.compatibility_issue = compat_from_model_type(mt);
            }
        }

        card::fill(&mut card, &meta_json, &tree_entries, &config, hardware);

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
        let url = format!("{}/models/{repo_id}", self.api_base);
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

    /// The SHA-256 HuggingFace publishes for one file of a repository.
    ///
    /// Reads the recursive tree of `main` and returns the `lfs.oid` of the
    /// entry whose path is `path`. This is the reference a downloaded file is
    /// checked against.
    ///
    /// # Errors
    /// - [`HfError::NotFound`] / [`HfError::Gated`] on a 404 / 403.
    /// - [`HfError::NoPublishedSha256`] when the file is absent from the tree
    ///   or not stored through LFS.
    pub async fn lfs_sha256(&self, repo_id: &str, path: &str) -> Result<String, HfError> {
        let entries = self.tree(repo_id).await?;
        lfs_sha256_by_path(&entries)
            .remove(path)
            .ok_or_else(|| HfError::NoPublishedSha256 {
                repo_id: repo_id.to_string(),
                path: path.to_string(),
            })
    }

    /// Every file of one variant of `repo_id`, with where each lands and the
    /// reference its download is checked against.
    ///
    /// # Errors
    /// The errors of reading the tree (as [`Self::lfs_sha256`]), and
    /// [`HfError::Variant`] when the tree holds no complete variant.
    pub async fn variant_plan(
        &self,
        repo_id: &str,
        selector: &VariantSelector,
    ) -> Result<VariantPlan, HfError> {
        let entries = self.tree(repo_id).await?;
        Ok(plan_variant(&self.cdn_base, repo_id, &entries, selector)?)
    }

    /// The recursive tree of `main`.
    async fn tree(&self, repo_id: &str) -> Result<Vec<TreeEntry>, HfError> {
        let url = format!(
            "{}/models/{repo_id}/tree/main?recursive=true",
            self.api_base
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;
        match resp.status() {
            reqwest::StatusCode::NOT_FOUND => return Err(HfError::NotFound(repo_id.to_string())),
            reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::UNAUTHORIZED => {
                return Err(HfError::Gated(repo_id.to_string()))
            }
            s if !s.is_success() => {
                return Err(HfError::Body(format!("tree of {repo_id} answered {s}")))
            }
            _ => {}
        }
        apollia_core::net::read_capped_json(resp, HF_MAX_JSON_BYTES)
            .await
            .map_err(|e| HfError::Body(e.to_string()))
    }

    /// Fetch the recommended generation parameters from `generation_config.json`.
    ///
    /// Returns `None` if the file is absent (model with no explicit generation config).
    pub async fn get_generation_config(&self, repo_id: &str) -> Option<GenerationConfig> {
        let url = format!(
            "{}/{repo_id}/resolve/main/generation_config.json",
            self.cdn_base
        );
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
    fn a_file_the_old_badge_called_tight_gets_the_estimator_verdict_in_the_hub() {
        // GIVEN a 16 GiB file on a 32 GB Apple Silicon machine (24 GB budget):
        // the old badge, 16 x 1.1 = 17.6 over 70 % of 24 = 16.8, said
        // "might fit"; the estimator, weights plus cache plus overhead, says
        // it fits
        let profile = HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 20.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: crate::hardware::AcceleratorProfile::AppleSilicon {
                chip: "M3 Max".to_owned(),
                generation: 3,
                vram_gb: 32.0,
            },
            memory_budget_gb: 24.0,
        };
        let size: u64 = 16 * 1024 * 1024 * 1024;
        let entries: Vec<TreeEntry> = serde_json::from_value(json!([
            { "type": "file", "path": "model-Q4_K_M.gguf", "size": size }
        ]))
        .expect("tree parses");

        // WHEN the Hub lists the file
        let files = build_gguf_files(&entries, "org/model-GGUF", Some(&profile));

        // THEN its badge is the estimator's, the same one the ranking and the
        // offload plan read for the same inputs
        let fit = crate::recommend::assess_fit(
            size,
            &crate::gguf_probe::GgufHeaderFacts::default(),
            &profile,
            &crate::recommend::RuntimeShape::default(),
        );
        assert_eq!(fit.badge, CompatibilityBadge::Fits);
        assert!(fit.plan.fully_offloaded);
        assert_eq!(files[0].compatibility, Some(CompatibilityBadge::Fits));
    }

    /// A tree recorded from `ggml-org/SmolVLM-256M-Instruct-GGUF`: two plain
    /// git files and four LFS files.
    const RECORDED_TREE: &str = include_str!("hf_registry/fixtures/tree_smolvlm_256m.json");

    #[test]
    fn tree_yields_the_lfs_sha256_of_every_lfs_file() {
        // GIVEN a repository tree recorded from the HuggingFace API
        let entries: Vec<TreeEntry> =
            serde_json::from_str(RECORDED_TREE).expect("recorded tree parses");

        // WHEN the tree is read
        let by_path = lfs_sha256_by_path(&entries);
        let files = build_gguf_files(&entries, "ggml-org/SmolVLM-256M-Instruct-GGUF", None);

        // THEN every LFS file carries its lfs.oid, and the plain git files none
        assert_eq!(by_path.len(), 4);
        assert_eq!(
            by_path
                .get("SmolVLM-256M-Instruct-Q8_0.gguf")
                .map(String::as_str),
            Some("2a31195d3769c0b0fd0a4906201666108834848db768af11de1d2cef7cd35e65")
        );
        assert_eq!(
            by_path
                .get("mmproj-SmolVLM-256M-Instruct-f16.gguf")
                .map(String::as_str),
            Some("0802360aca1748f112ea510b8ff277c65b1361c8ef30ed89b83c9c7a60d08e96")
        );
        assert!(!by_path.contains_key("README.md"));
        assert!(!by_path.contains_key(".gitattributes"));
        assert_eq!(files.len(), 4);
        for file in &files {
            assert_eq!(file.sha256.as_ref(), by_path.get(&file.filename));
        }
    }

    #[test]
    fn tree_entry_with_a_malformed_oid_has_no_sha256() {
        // GIVEN an LFS entry whose oid is not a SHA-256
        let entries: Vec<TreeEntry> = serde_json::from_value(json!([
            { "type": "file", "path": "a.gguf", "size": 3, "lfs": { "oid": "abc", "size": 3 } }
        ]))
        .expect("tree parses");

        // WHEN the tree is read
        let by_path = lfs_sha256_by_path(&entries);

        // THEN no reference is offered for that file
        assert!(by_path.is_empty());
    }

    #[test]
    fn base_model_ignores_malformed_tag_without_slash() {
        // GIVEN: a tag that's `base_model:something` but without org/name
        let payload = json!({ "tags": ["base_model:notavalidrepo"] });
        // WHEN the base model is extracted
        // THEN the malformed tag is ignored rather than taken for a repo id
        assert!(extract_base_model_from_json(&payload).is_none());
    }

    /// A search page recorded from the HuggingFace API on 2026-10-01:
    /// `models?search=Qwen3-4B&filter=gguf&sort=downloads&limit=20`.
    const RECORDED_SEARCH: &str = include_str!("hf_registry/fixtures/search_qwen3_4b_gguf.json");

    #[test]
    fn a_recorded_search_marks_official_and_converter_owners() {
        // GIVEN a search page recorded from the HuggingFace API, mixing the
        // publisher's own repository, recognised converters and other owners
        let page: serde_json::Value =
            serde_json::from_str(RECORDED_SEARCH).expect("recorded search parses");
        let items = page.as_array().expect("a search page is an array");

        // WHEN each item is read as a model card
        let cards: Vec<HfModelCard> = items
            .iter()
            .filter_map(|m| parse_model_list_item(m, None))
            .collect();
        let kind = |repo: &str| {
            cards
                .iter()
                .find(|c| c.repo_id == repo)
                .expect("the repository is in the recorded page")
                .publisher_kind
        };

        // THEN every item parses, and each carries the kind of its owner
        assert_eq!(cards.len(), items.len());
        assert_eq!(
            kind("Qwen/Qwen3-4B-GGUF"),
            Some(crate::recommend::PublisherKind::Official)
        );
        assert_eq!(
            kind("unsloth/Qwen3-4B-GGUF"),
            Some(crate::recommend::PublisherKind::Converter)
        );
        assert_eq!(
            kind("bartowski/Qwen_Qwen3-4B-GGUF"),
            Some(crate::recommend::PublisherKind::Converter)
        );
        assert_eq!(
            kind("lmstudio-community/Qwen3.5-4B-GGUF"),
            Some(crate::recommend::PublisherKind::Converter)
        );
        assert_eq!(kind("MaziyarPanahi/Qwen3-4B-GGUF"), None);
        let unmarked = cards.iter().filter(|c| c.publisher_kind.is_none()).count();
        assert!(unmarked > 0, "the page holds owners on neither list");
    }
}
