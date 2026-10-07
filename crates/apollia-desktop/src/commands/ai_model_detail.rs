//! Tauri IPC commands of the panel of a model, on the AI screen.
//!
//! - `get_installed_model_detail`: what the panel shows beyond the row of the
//!   list: the weights file, its quantisation and architecture, the category
//!   detected and the one chosen over it, the other weights files of the same
//!   model, the switches its card declares, and the draft models that can
//!   draft for it.
//! - `set_installed_model_category_override`: the category chosen over the
//!   detected one, or its removal.
//! - `set_installed_model_source_override`: another weights file of the same
//!   model to load in place of the indexed one, or its removal.
//! - `create_model_preset`, `update_model_preset`: `POST` and `PUT` on
//!   `/api/v1/llm/presets`.
//!
//! The first three read and write the index of the models folder, which the
//! daemon shares since it listens on this machine, as the pin does. The last
//! two are runtime routes and serve both modes through `attached/proxy.rs`.

use std::path::{Path, PathBuf};

use apollia_llm::gguf_probe::{probe_file, SCREEN_BYTES};
use apollia_llm::model_cards::{installed_card, LocalizedAuthorField};
use apollia_llm::recommend::{gguf_file_quantization, mlx_quantization};
use apollia_llm::{IndexedModel, ModelCategory, ModelFormat, ModelIndex};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::attached::RuntimeLink;

const PRESETS_PATH: &str = "/api/v1/llm/presets";

/// Another weights file of a model, which its source override may name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceCandidate {
    /// Absolute path of the file.
    pub path: String,
    /// File name.
    pub name: String,
    /// Quantisation its name declares, when it reads as one.
    pub quantization: Option<String>,
    /// Size on disk in bytes.
    pub size_bytes: u64,
}

/// A draft model the load settings of a model may name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftCandidate {
    /// Identifier in the model index, what the load settings store.
    pub id: String,
    /// Parameter count label read from its header, for example `"0.5B"`.
    pub parameters: Option<String>,
    /// Size on disk in bytes.
    pub size_bytes: u64,
}

/// What the panel of a model shows beyond its row.
#[derive(Debug, Clone, Serialize)]
pub struct ModelDetailView {
    /// Identifier in the model index, the name an agent uses.
    pub id: String,
    /// Name of the weights file in force: the source override when set.
    pub file: String,
    /// Storage format.
    pub format: ModelFormat,
    /// Quantisation of the weights in force, when it can be read.
    pub quantization: Option<String>,
    /// Architecture the weights declare, when it can be read.
    pub architecture: Option<String>,
    /// Category detected from the header.
    pub detected_category: ModelCategory,
    /// Category chosen over the detected one.
    pub category_override: Option<ModelCategory>,
    /// The weights file loaded in place of the indexed one.
    pub source_file_override: Option<String>,
    /// The other weights files of the same model on this machine.
    pub source_candidates: Vec<SourceCandidate>,
    /// The switches the card of the model declares, in the asked language.
    pub author_fields: Vec<LocalizedAuthorField>,
    /// Whether the engine of the model takes a draft model for speculative
    /// decoding: the GGUF engine does, the MLX engine does not.
    pub draft_setting: bool,
    /// The installed draft models whose vocabulary is the one of this model.
    pub compatible_drafts: Vec<DraftCandidate>,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The quantisation and the architecture of an MLX folder, from its
/// `config.json`.
fn mlx_facts(folder: &Path) -> (Option<String>, Option<String>) {
    let Ok(config) = std::fs::read_to_string(folder.join("config.json")) else {
        return (None, None);
    };
    let architecture = serde_json::from_str::<Value>(&config).ok().and_then(|v| {
        v.get("model_type")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    (
        mlx_quantization(&config).map(|(_, label)| label),
        architecture,
    )
}

/// The weights files beside a GGUF model that its source override accepts:
/// the same model under another quantisation, never a projector, a part of a
/// split file or the weights of another model. An MLX model is its folder, so
/// it has none.
fn source_candidates(index: &ModelIndex, model: &IndexedModel) -> Vec<SourceCandidate> {
    if model.format != ModelFormat::Gguf {
        return Vec::new();
    }
    let Some(folder) = model.path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut probe = index.clone();
    files
        .into_iter()
        .filter(|p| {
            probe
                .set_source_file_override(&model.id, Some(p.clone()))
                .is_ok()
        })
        .map(|p| {
            let name = file_name(&p);
            SourceCandidate {
                quantization: gguf_file_quantization(&name),
                size_bytes: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                path: p.display().to_string(),
                name,
            }
        })
        .collect()
}

/// The switches the card kept beside the model declares; none when the model
/// has no card or the card cannot be read.
fn author_fields(model: &IndexedModel, lang: &str) -> Vec<LocalizedAuthorField> {
    match installed_card(&model.path) {
        Ok(Some(card)) => card.localized(lang).author_fields,
        Ok(None) => Vec::new(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                id = %model.id,
                error = %error,
                detail = "the panel shows no author field",
                "model.detail.card_unreadable"
            );
            Vec::new()
        }
    }
}

/// The detail of the model `id` of the index of `models_dir`.
fn detail_in(models_dir: &Path, id: &str, lang: &str) -> Result<ModelDetailView, String> {
    let index = super::model_index::current(models_dir).map_err(|e| e.to_string())?;
    let model = index
        .get(id)
        .ok_or_else(|| format!("no installed model is named {id}"))?;
    let weights = model.source_file_override.as_deref().unwrap_or(&model.path);
    let file = file_name(weights);
    let (quantization, architecture) = match model.format {
        ModelFormat::Gguf => (
            gguf_file_quantization(&file),
            probe_file(weights, SCREEN_BYTES)
                .ok()
                .and_then(|facts| facts.architecture),
        ),
        ModelFormat::Mlx => mlx_facts(&model.path),
    };
    Ok(ModelDetailView {
        id: model.id.clone(),
        file,
        format: model.format,
        quantization,
        architecture,
        detected_category: model.category,
        category_override: model.category_override,
        source_file_override: model
            .source_file_override
            .as_ref()
            .map(|p| p.display().to_string()),
        source_candidates: source_candidates(&index, model),
        author_fields: author_fields(model, lang),
        draft_setting: model.format == ModelFormat::Gguf,
        compatible_drafts: index
            .compatible_drafts(&model.id)
            .into_iter()
            .map(|draft| DraftCandidate {
                id: draft.id.clone(),
                parameters: draft.parameters.clone(),
                size_bytes: draft.size_bytes,
            })
            .collect(),
    })
}

/// Writes the category chosen over the detected one, or removes it.
fn override_category_in(
    models_dir: &Path,
    id: &str,
    category: Option<ModelCategory>,
) -> Result<(), String> {
    let mut index = super::model_index::current(models_dir).map_err(|e| e.to_string())?;
    index
        .set_category_override(id, category)
        .map_err(|e| e.to_string())?;
    index
        .save(&super::model_index::index_path(models_dir))
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        id = %id,
        overridden = category.is_some(),
        "model.index.category_override"
    );
    Ok(())
}

/// Writes the weights file loaded in place of the indexed one, or removes it.
/// The index refuses a projector, a part and the weights of another model.
fn override_source_in(models_dir: &Path, id: &str, source: Option<&str>) -> Result<(), String> {
    let mut index = super::model_index::current(models_dir).map_err(|e| e.to_string())?;
    index
        .set_source_file_override(id, source.map(PathBuf::from))
        .map_err(|e| e.to_string())?;
    index
        .save(&super::model_index::index_path(models_dir))
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        id = %id,
        overridden = source.is_some(),
        "model.index.source_override"
    );
    Ok(())
}

/// What the panel of an installed model shows beyond its row, the texts of
/// its card in `lang` (`fr` or `en`, English when the language is not
/// written).
#[tauri::command]
pub async fn get_installed_model_detail(
    id: String,
    lang: Option<String>,
) -> Result<ModelDetailView, String> {
    let dir = super::model_hub::models_dir();
    tokio::task::spawn_blocking(move || detail_in(&dir, &id, lang.as_deref().unwrap_or_default()))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Chooses a category over the one detected for an installed model, or
/// removes that choice with `None`.
#[tauri::command]
pub async fn set_installed_model_category_override(
    id: String,
    category: Option<ModelCategory>,
) -> Result<(), String> {
    let dir = super::model_hub::models_dir();
    tokio::task::spawn_blocking(move || override_category_in(&dir, &id, category))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Loads another weights file of the same model in place of the indexed one,
/// or removes that choice with `None`.
#[tauri::command]
pub async fn set_installed_model_source_override(
    id: String,
    path: Option<String>,
) -> Result<(), String> {
    let dir = super::model_hub::models_dir();
    tokio::task::spawn_blocking(move || override_source_in(&dir, &id, path.as_deref()))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

fn preset_path(id: i64) -> String {
    format!("{PRESETS_PATH}/{id}")
}

pub(crate) async fn create_preset_on(link: &RuntimeLink, preset: &Value) -> Result<Value, String> {
    link.post_json(PRESETS_PATH, preset).await
}

pub(crate) async fn update_preset_on(
    link: &RuntimeLink,
    id: i64,
    preset: &Value,
) -> Result<Value, String> {
    link.put_json(&preset_path(id), preset).await
}

/// Creates a preset of a model: its name, its model, its load and request
/// settings, and whether it is the default of that model.
#[tauri::command]
pub async fn create_model_preset(
    link: State<'_, RuntimeLink>,
    preset: Value,
) -> Result<Value, String> {
    create_preset_on(link.inner(), &preset).await
}

/// Replaces the name and the settings of the preset `id`, and says whether it
/// is the default of its model.
#[tauri::command]
pub async fn update_model_preset(
    link: State<'_, RuntimeLink>,
    id: i64,
    preset: Value,
) -> Result<Value, String> {
    update_preset_on(link.inner(), id, &preset).await
}

#[cfg(test)]
mod tests;
