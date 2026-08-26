//! Parsing of the HuggingFace API payloads.
//!
//! Split out of `hf_registry.rs`: the client stays in the parent, the pure
//! functions that read a JSON model card, a repo tree, or a `Link` header and
//! turn them into the crate's types live here.

use crate::hardware::{CompatibilityBadge, HardwareProfile};
use crate::hf_registry::compat::{compat_from_pipeline_tag, extract_model_type};
use crate::hf_registry::{
    CompatIssue, HfFile, HfModelCard, HfModelTypeCache, HfSearchFilter, TreeEntry, HF_API_BASE,
    HF_CDN_BASE,
};

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
pub(super) fn extract_base_model_from_json(json: &serde_json::Value) -> Option<String> {
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
pub(super) fn base_model_from_card_data(json: &serde_json::Value) -> Option<String> {
    let v = json.get("cardData").and_then(|c| c.get("base_model"))?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.as_array()
        .and_then(|arr| arr.iter().find_map(|x| x.as_str()))
        .map(str::to_string)
}
/// Pass 2a: unqualified `base_model:org/name` tag (points directly upstream).
pub(super) fn base_model_from_unqualified_tag(tags: &[&str]) -> Option<String> {
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
pub(super) fn base_model_from_qualified_tag(tags: &[&str]) -> Option<String> {
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
pub(super) fn build_gguf_files(
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
pub(super) fn reevaluate_gguf_compat(card: &mut HfModelCard) {
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
pub(super) async fn resolve_model_type(
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
pub(super) fn build_search_url(query: &str, filter: &HfSearchFilter) -> String {
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
pub(super) fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
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
/// Extract the basic metadata of a model from the HF JSON response.
/// Does NOT include file sizes (available only via the tree API).
pub(super) fn parse_model_list_item(
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
pub(super) fn format_size(bytes: u64) -> String {
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
