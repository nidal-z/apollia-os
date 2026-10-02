//! Parsing of the HuggingFace API payloads.
//!
//! Split out of `hf_registry.rs`: the client stays in the parent, the pure
//! functions that read a JSON model card, a repo tree, or a `Link` header and
//! turn them into the crate's types live here.

use crate::hardware::{CompatibilityBadge, HardwareProfile};
use crate::hf_registry::compat::{compat_from_pipeline_tag, extract_config_facts, ConfigFacts};
use crate::hf_registry::{
    CompatIssue, HfFile, HfModelCard, HfModelTypeCache, TreeEntry, HF_CDN_BASE,
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
            let compatibility = hardware.map(|hw| listed_file_badge(size_bytes, hw));
            HfFile {
                filename: e.path.clone(),
                size_bytes,
                size_human: format_size(size_bytes),
                compatibility,
                download_url: format!("{HF_CDN_BASE}/{repo_id}/resolve/main/{}", e.path),
                sha256: e.lfs.as_ref().and_then(|lfs| normalize_sha256(&lfs.oid)),
                quantization: file_quantization(&e.path),
            }
        })
        .collect()
}

/// The quantisation a file name declares, a projector never declaring one.
fn file_quantization(path: &str) -> Option<String> {
    if crate::model_index::is_projector_name(std::path::Path::new(path)) {
        return None;
    }
    crate::recommend::gguf_file_quantization(path)
}

/// The badge of a file the Hub lists by size alone.
///
/// No header has been read, so the estimator's proportional cache applies;
/// the verdict is still the one [`crate::recommend::fit::grade`] gives every
/// other surface for the same inputs.
pub(crate) fn listed_file_badge(size_bytes: u64, hardware: &HardwareProfile) -> CompatibilityBadge {
    crate::recommend::assess_fit(
        size_bytes,
        &crate::gguf_probe::GgufHeaderFacts::default(),
        hardware,
        &crate::recommend::RuntimeShape::default(),
    )
    .badge
}

/// Map every LFS file of a repository tree to its SHA-256.
///
/// HuggingFace publishes the content hash of an LFS file as `lfs.oid`. A file
/// committed outside LFS carries a git blob id instead (`oid` at the top
/// level, SHA-1 of the git object, not of the content) and is left out.
pub(super) fn lfs_sha256_by_path(
    tree_entries: &[TreeEntry],
) -> std::collections::HashMap<String, String> {
    tree_entries
        .iter()
        .filter(|e| e.entry_type == "file")
        .filter_map(|e| {
            let sha = normalize_sha256(&e.lfs.as_ref()?.oid)?;
            Some((e.path.clone(), sha))
        })
        .collect()
}

/// A SHA-256 as 64 lowercase hex digits, or `None` when `raw` is not one.
pub(super) fn normalize_sha256(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    (lower.len() == 64 && lower.bytes().all(|b| b.is_ascii_hexdigit())).then_some(lower)
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
/// Determine what `config.json` says: prefer the cache, then the response.
/// On a miss, populate the cache with the resolved value.
pub(super) async fn resolve_config_facts(
    cached: Option<Option<ConfigFacts>>,
    config_resp: Result<Option<reqwest::Response>, reqwest::Error>,
    type_cache: Option<&HfModelTypeCache>,
    repo_id: &str,
) -> Option<ConfigFacts> {
    if let Some(cached) = cached {
        return cached;
    }
    let facts = match config_resp {
        Ok(Some(r)) => extract_config_facts(Ok(r)).await,
        _ => None,
    };
    if let Some(cache) = type_cache {
        cache.set(repo_id, facts.clone()).await;
    }
    facts
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
    let last_modified = json["lastModified"].as_str().map(String::from);
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
                        sha256: None,
                        quantization: file_quantization(name),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut compatibility_issue = compat_from_pipeline_tag(pipeline_tag);
    if compatibility_issue.is_none() && gguf_files.is_empty() {
        compatibility_issue = Some(CompatIssue::NoGgufFiles);
    }

    let publisher_kind = crate::recommend::publisher_kind(&repo_id);
    // A search result lists names only: no template, so tools and reasoning
    // stay unknown, and vision is the projector a download would fetch.
    let capabilities = crate::capabilities::Capabilities::default().with_catalogue_vision(
        gguf_files
            .iter()
            .any(|f| crate::model_index::is_projector_name(std::path::Path::new(&f.filename))),
    );

    Some(HfModelCard {
        repo_id,
        author,
        publisher_kind,
        downloads,
        likes,
        last_modified,
        license,
        tags,
        gated,
        total_size_bytes: None,
        gguf_files,
        generation_config: None,
        compatibility_issue,
        model_type: None,
        capabilities,
        variants: Vec::new(),
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
