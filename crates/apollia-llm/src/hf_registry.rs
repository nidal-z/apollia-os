//! Client HuggingFace Hub API pour la découverte et le téléchargement de modèles GGUF.
//!
//! Utilise l'API publique HF (pas d'authentification pour les modèles publics).
//! Un token optionnel permet d'accéder aux modèles gated (Llama 3.1, etc.).
//!
//! Architecture :
//! - [`HfRegistryClient`] — client HTTP principal
//! - [`HfModelCard`] — métadonnées d'un modèle HF
//! - [`HfFile`] — fichier dans un repo HF (avec taille pour les badges de compatibilité)
//! - [`HfSearchFilter`] — filtres de recherche (format, sort, etc.)

#![cfg(feature = "cloud")]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hardware::{CompatibilityBadge, HardwareProfile};

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Erreurs du client HuggingFace.
#[derive(Debug, Error)]
pub enum HfError {
    /// Erreur réseau HTTP.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Erreur de désérialisation JSON.
    #[error("json parse error: {0}")]
    Json(String),

    /// Modèle introuvable sur HuggingFace.
    #[error("model not found: {0}")]
    NotFound(String),

    /// Le modèle est gated et nécessite un token HF.
    #[error("model '{0}' is gated — a HuggingFace token is required")]
    Gated(String),
}

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Carte de métadonnées d'un modèle HuggingFace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModelCard {
    /// Identifiant du repo (ex. `"bartowski/Qwen3-30B-A3B-GGUF"`).
    pub repo_id: String,
    /// Auteur / organisation.
    pub author: Option<String>,
    /// Nombre de téléchargements (30 jours glissants).
    pub downloads: u64,
    /// Nombre de likes.
    pub likes: u64,
    /// License SPDX (ex. `"apache-2.0"`, `"mit"`).
    pub license: Option<String>,
    /// Tags HF (ex. `["gguf", "qwen3", "text-generation"]`).
    pub tags: Vec<String>,
    /// `true` si le modèle nécessite un token HF pour être téléchargé.
    pub gated: bool,
    /// Taille totale du repo en bytes (somme de tous les fichiers).
    pub total_size_bytes: Option<u64>,
    /// Fichiers GGUF disponibles dans ce repo.
    pub gguf_files: Vec<HfFile>,
    /// Paramètres de génération recommandés (depuis `generation_config.json`).
    pub generation_config: Option<GenerationConfig>,
    /// Problème de compatibilité détecté (pipeline_tag ou model_type).
    pub compatibility_issue: Option<CompatIssue>,
    /// Type de modèle HF depuis `config.json` (ex. `"LlamaForCausalLM"`).
    pub model_type: Option<String>,
}

/// Raisons pour lesquelles un modèle est incompatible avec llama.cpp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatIssue {
    /// Modèle d'embedding ou de feature-extraction — pas adapté à la génération de texte.
    EmbeddingModel,
    /// Architecture dans `config.json` absente de la liste supportée par llama.cpp.
    UnknownArchitecture,
    /// Aucun fichier GGUF trouvé dans ce repo.
    NoGgufFiles,
}

/// Un fichier dans un repo HuggingFace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFile {
    /// Nom du fichier (ex. `"Qwen3-30B-A3B-Q4_K_M.gguf"`).
    pub filename: String,
    /// Taille du fichier en bytes.
    pub size_bytes: u64,
    /// Taille en format lisible (ex. `"17.2 GB"`).
    pub size_human: String,
    /// Badge de compatibilité calculé par rapport au hardware local.
    pub compatibility: Option<CompatibilityBadge>,
    /// URL de téléchargement directe.
    pub download_url: String,
}

/// Paramètres de génération recommandés depuis `generation_config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f64>,
    pub max_new_tokens: Option<u32>,
}

/// Filtres pour la recherche de modèles HF.
#[derive(Debug, Clone, Default)]
pub struct HfSearchFilter {
    /// Filtre par tag (ex. `"gguf"`).
    pub filter: Option<String>,
    /// Tri : `"downloads"` (défaut), `"likes"`, `"trending"`, `"createdAt"`.
    pub sort: Option<String>,
    /// Nombre maximum de résultats.
    pub limit: Option<u32>,
    /// Filtre par tâche HF (ex. `"text-generation"`).
    pub pipeline_tag: Option<String>,
    /// Filtre par langue (ex. `"fr"`, `"en"`, `"zh"`).
    pub language: Option<String>,
    /// Curseur "Load more" — URL complète retournée par l'API HF dans le header `Link: rel="next"`.
    /// Si renseigné, remplace tous les autres paramètres (la requête est déjà encodée dans l'URL).
    pub next_cursor: Option<String>,
}

/// Résultat d'une page de recherche HF.
#[derive(Debug, Clone, Serialize)]
pub struct SearchPage {
    /// Modèles de cette page.
    pub models: Vec<HfModelCard>,
    /// URL de la prochaine page (header `Link: rel="next"`) — `None` si dernière page.
    pub next_cursor: Option<String>,
}

// ─────────────────────────────────────────────
// Compatibility detection
// ─────────────────────────────────────────────

/// Architectures `model_type` dont llama.cpp garantit le support.
/// Source : `src/llama.cpp` (LLAMA_MODEL_ARCH) — mise à jour à chaque bump de crate.
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
    // Suffixes qui indiquent un modèle non-génératif
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
        return None; // architecture connue et supportée
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

// ─────────────────────────────────────────────
// Model type cache (session-level, TTL 24h)
// ─────────────────────────────────────────────

struct CachedModelType {
    model_type: Option<String>,
    cached_at: std::time::Instant,
}

/// Cache session des types de modèle HF (depuis `config.json`).
///
/// Évite de refetcher `config.json` à chaque ouverture d'une card.
/// TTL 24h — enregistré comme état Tauri (`Arc<HfModelTypeCache>`).
pub struct HfModelTypeCache {
    entries: tokio::sync::RwLock<std::collections::HashMap<String, CachedModelType>>,
}

const MODEL_TYPE_CACHE_TTL_SECS: u64 = 86_400;

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

// ─────────────────────────────────────────────
// Internal types
// ─────────────────────────────────────────────

/// Entrée retournée par `/api/models/{id}/tree/main`.
#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    size: u64,
    #[serde(rename = "type")]
    entry_type: String,
}

// ─────────────────────────────────────────────
// Client
// ─────────────────────────────────────────────

const HF_API_BASE: &str = "https://huggingface.co/api";
const HF_CDN_BASE: &str = "https://huggingface.co";

/// Client HuggingFace Hub — API publique + token optionnel pour les modèles gated.
pub struct HfRegistryClient {
    client: reqwest::Client,
    token: Option<String>,
}

impl HfRegistryClient {
    /// Crée un nouveau client. `token` est optionnel (uniquement pour les modèles gated).
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

    /// Recherche des modèles GGUF sur HuggingFace.
    ///
    /// Retourne une [`SearchPage`] avec les modèles et un curseur `next_cursor` pour la pagination.
    /// Le curseur est extrait du header HTTP `Link: rel="next"` — il contient l'URL complète
    /// de la page suivante. Passer ce curseur dans `filter.next_cursor` pour charger la page suivante.
    ///
    /// # Errors
    /// [`HfError::Http`] si la requête échoue.
    pub async fn search(
        &self,
        query: &str,
        filter: HfSearchFilter,
        hardware: Option<&HardwareProfile>,
    ) -> Result<SearchPage, HfError> {
        // Si un curseur est fourni, on l'utilise directement (l'URL encode déjà tous les filtres).
        let url = if let Some(ref cursor) = filter.next_cursor {
            cursor.clone()
        } else {
            let sort = filter.sort.as_deref().unwrap_or("downloads");
            let tag = filter.filter.as_deref().unwrap_or("gguf");
            let limit = filter.limit.unwrap_or(50);

            let mut u =
                format!("{HF_API_BASE}/models?filter={tag}&sort={sort}&limit={limit}&full=false");
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
        };

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        // Extrait le curseur de pagination depuis le header `Link: <url>; rel="next"`.
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

    /// Récupère les métadonnées complètes d'un modèle (y compris la liste des fichiers GGUF).
    ///
    /// Fait deux requêtes en parallèle :
    /// 1. `/api/models/{id}` — métadonnées (downloads, likes, tags, license, gated)
    /// 2. `/api/models/{id}/tree/main` — tailles réelles des fichiers LFS
    ///
    /// # Errors
    /// - [`HfError::NotFound`] si le repo n'existe pas.
    /// - [`HfError::Gated`] si le modèle est gated et qu'aucun token n'est fourni.
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
        card.gguf_files = tree_entries
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
            .collect();

        // Re-evaluate NoGgufFiles based on authoritative tree data.
        match &card.compatibility_issue {
            None if card.gguf_files.is_empty() => {
                card.compatibility_issue = Some(CompatIssue::NoGgufFiles);
            }
            Some(CompatIssue::NoGgufFiles) if !card.gguf_files.is_empty() => {
                card.compatibility_issue = None;
            }
            _ => {}
        }

        // Determine model_type: prefer cache, then config_resp.
        let model_type: Option<String> = if let Some(ref cached) = cached_model_type {
            cached.clone()
        } else {
            let mt = match config_resp {
                Ok(Some(r)) => extract_model_type(Ok(r)).await,
                _ => None,
            };
            if let Some(cache) = type_cache {
                cache.set(repo_id, mt.clone()).await;
            }
            mt
        };

        card.model_type = model_type.clone();

        // Refine compat with model_type only if pipeline_tag didn't already flag an issue.
        if card.compatibility_issue.is_none() {
            if let Some(ref mt) = model_type {
                card.compatibility_issue = compat_from_model_type(mt);
            }
        }

        Ok(card)
    }

    /// Résout le `base_model` déclaré par un repo dérivé (quantisations
    /// Bartowski/Unsloth/mradermacher, fine-tunes, merges).
    ///
    /// Retourne `Some("org/name")` quand le repo déclare un parent via
    /// `cardData.base_model` (champ structuré) ou via un tag
    /// `base_model:org/name`. `None` si le repo est de premier ordre ou
    /// si le champ est absent.
    ///
    /// Indispensable pour récupérer un `generation_config.json` quand le
    /// repo téléchargé est une dérivation GGUF — les quanteurs ne
    /// republient pas le fichier de config (il vient du repo amont).
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

    /// Récupère les paramètres de génération recommandés depuis `generation_config.json`.
    ///
    /// Retourne `None` si le fichier est absent (modèle sans config de génération explicite).
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

// ─────────────────────────────────────────────
// Pagination helper
// ─────────────────────────────────────────────

/// Extrait le `base_model` (org/name) depuis la réponse `/api/models/{id}`.
///
/// Stratégie en deux passes :
/// 1. `cardData.base_model` (string ou tableau) — c'est le champ structuré
///    que HF expose quand le model card YAML déclare `base_model: foo/bar`.
/// 2. Tags `base_model:org/name` — fallback pour les repos qui ne mettent
///    que les tags. On préfère un tag *non qualifié* (`base_model:Qwen/X`)
///    car il pointe directement vers l'amont. Si seul un tag qualifié
///    existe (`base_model:quantized:Qwen/X`), on extrait l'org/name après
///    le second `:`.
fn extract_base_model_from_json(json: &serde_json::Value) -> Option<String> {
    if let Some(v) = json.get("cardData").and_then(|c| c.get("base_model")) {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        if let Some(arr) = v.as_array() {
            if let Some(s) = arr.iter().find_map(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
    }

    let tags: Vec<&str> = json
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();

    for t in &tags {
        if let Some(rest) = t.strip_prefix("base_model:") {
            if !rest.contains(':') && rest.contains('/') {
                return Some(rest.to_string());
            }
        }
    }
    for t in &tags {
        if let Some(rest) = t.strip_prefix("base_model:") {
            if let Some(colon) = rest.find(':') {
                let upstream = &rest[colon + 1..];
                if upstream.contains('/') {
                    return Some(upstream.to_string());
                }
            }
        }
    }
    None
}

/// Extrait l'URL `rel="next"` depuis le header HTTP `Link` de HuggingFace.
///
/// Format : `Link: <https://huggingface.co/api/models?...>; rel="next", <...>; rel="first"`
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

// ─────────────────────────────────────────────
// Parsing helpers
// ─────────────────────────────────────────────

/// Extrait les métadonnées de base d'un modèle depuis la réponse JSON HF.
/// N'inclut PAS les tailles de fichiers (disponibles uniquement via l'API tree).
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
    // Populate gguf_files with filenames only — sizes come from the tree API.
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
