//! Import of a model from a disk or a USB key into `~/.apollia/models/`.
//!
//! The import checks the model before copying it (`apollia_llm::check_import`):
//! its structure, then its SHA-256 against the digests the index already
//! holds as verified and the digests the registry pins for its embeddings
//! models. A match marks the origin verified; otherwise the model
//! enters with an unverified origin, unless the `models.import.require_verified`
//! setting asks for a refusal. The setting is read through the settings
//! contract: the organisation policy fixes it (`inference.require_model_hash`
//! or `[settings]`) over `apollia.toml`, and a refusal it causes names the
//! policy.

use std::path::{Path, PathBuf};

use apollia_core::config::{PolicySettings, SettingSource};
use apollia_llm::{ImportCheckError, ModelImportRule};

/// Dotted path of the setting, in `apollia.toml`.
pub(crate) const REQUIRE_VERIFIED_KEY: &str = "models.import.require_verified";

/// Error raised while importing a user-selected model into the managed
/// models directory.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportModelError {
    /// The chosen source path is neither a regular file nor a folder.
    #[error("source not found: {0}")]
    NotFound(String),
    /// The file extension is not among the accepted ones.
    #[error("unsupported file type '{name}' (expected: {expected})")]
    BadExtension {
        /// Basename of the rejected file.
        name: String,
        /// Comma-separated list of accepted extensions.
        expected: String,
    },
    /// The source path has no usable file name component.
    #[error("invalid source file name")]
    BadFileName,
    /// The structure or the origin check refused the model.
    #[error(transparent)]
    Check(#[from] ImportCheckError),
    /// The origin check refused the model under a rule the organisation
    /// policy fixes.
    #[error(
        "{path} was refused by the organisation policy: its origin cannot be verified and the \
         policy requires a known model digest"
    )]
    RefusedByPolicy {
        /// Rejected file or folder.
        path: String,
    },
    /// The models directory could not be created.
    #[error("failed to create models directory: {0}")]
    CreateDir(#[source] std::io::Error),
    /// The copy into the models directory failed.
    #[error("failed to copy model: {0}")]
    Copy(#[source] std::io::Error),
}

/// Checks, then copies, a user-selected model into `~/.apollia/models/`.
///
/// `source_path` is a GGUF or whisper.cpp file, or an MLX folder. The native
/// picker lives on the frontend (Tauri dialog plugin); `allowed_extensions` is
/// what the caller expects of a file (`gguf` for an LLM, `bin`/`gguf` for a
/// Whisper model). The copy leaves the source in place; a source already in
/// the models directory is not copied. Returns the destination path so the
/// caller can select it after a re-scan.
#[tauri::command]
pub async fn import_model_file(
    source_path: String,
    allowed_extensions: Vec<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let location = apollia_permissions::policy_location();
        let policy = apollia_permissions::load_policy(&location.dir, location.owner);
        let rule = import_rule(
            &apollia_core::paths::data_dir_under(super::model_hub::home_dir()),
            &policy.imposed_settings(),
        );
        import_model_file_under(
            &source_path,
            &allowed_extensions,
            &super::model_hub::models_dir(),
            rule,
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// The import rule in force, and whether the organisation policy fixed it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RuleInForce {
    /// The rule.
    pub(crate) rule: ModelImportRule,
    /// Whether the value comes from the organisation policy.
    pub(crate) fixed_by_policy: bool,
}

/// The import rule: the value the policy fixes in `imposed`, else the one set
/// in `<data_dir>/apollia.toml`. A missing file or key leaves the default:
/// unverified imports are accepted.
pub(crate) fn import_rule(data_dir: &Path, imposed: &PolicySettings) -> RuleInForce {
    let path = data_dir.join("apollia.toml");
    match apollia_core::config::resolve_setting(&path, REQUIRE_VERIFIED_KEY, imposed) {
        Ok(resolved) => RuleInForce {
            rule: ModelImportRule {
                require_verified: resolved.value.as_bool().unwrap_or(false),
            },
            fixed_by_policy: resolved.source == SettingSource::Policy,
        },
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                key = REQUIRE_VERIFIED_KEY,
                error = %e,
                "model.import.rule.unreadable"
            );
            RuleInForce::default()
        }
    }
}

/// [`import_model_file_into`] under the rule in force: a refusal of an
/// unverified origin under a rule the policy fixed names the policy.
pub(crate) fn import_model_file_under(
    source_path: &str,
    allowed_extensions: &[String],
    dir: &Path,
    in_force: RuleInForce,
) -> Result<String, ImportModelError> {
    match import_model_file_into(source_path, allowed_extensions, dir, in_force.rule) {
        Err(ImportModelError::Check(ImportCheckError::Unverified { path }))
            if in_force.fixed_by_policy =>
        {
            Err(ImportModelError::RefusedByPolicy {
                path: path.display().to_string(),
            })
        }
        other => other,
    }
}

pub(crate) fn import_model_file_into(
    source_path: &str,
    allowed_extensions: &[String],
    dir: &Path,
    rule: ModelImportRule,
) -> Result<String, ImportModelError> {
    let source = PathBuf::from(source_path);
    let is_folder = source.is_dir();
    if !is_folder && !source.is_file() {
        return Err(ImportModelError::NotFound(source_path.to_string()));
    }
    if !is_folder {
        check_extension(&source, allowed_extensions)?;
    }

    let mut known = super::model_index::current(dir)
        .map(|index| {
            index
                .verified_digests()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    known.extend(registry_digests());
    let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
    let check = apollia_llm::check_import(&source, &known_refs, rule)?;

    let name = source.file_name().ok_or(ImportModelError::BadFileName)?;
    std::fs::create_dir_all(dir).map_err(ImportModelError::CreateDir)?;
    let dest = dir.join(name);
    if dest != source {
        if is_folder {
            copy_folder(&source, &dest).map_err(ImportModelError::Copy)?;
        } else {
            // `std::fs::copy` streams the file rather than buffering it, which
            // matters for multi-gigabyte GGUF weights.
            std::fs::copy(&source, &dest).map_err(ImportModelError::Copy)?;
        }
    }

    tracing::event!(
        tracing::Level::INFO,
        dest = %dest.display(),
        origin_verified = check.origin_verified,
        "model.file.imported"
    );
    if let Err(e) =
        super::model_index::record_import(dir, &dest, check.sha256, check.origin_verified)
    {
        tracing::event!(
            tracing::Level::WARN,
            path = %dest.display(),
            error = %e,
            "model.index.record.failed"
        );
    }
    Ok(dest.display().to_string())
}

fn check_extension(source: &Path, allowed_extensions: &[String]) -> Result<(), ImportModelError> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if allowed_extensions
        .iter()
        .any(|a| a.eq_ignore_ascii_case(&ext))
    {
        return Ok(());
    }
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Err(ImportModelError::BadExtension {
        name,
        expected: allowed_extensions.join(", "),
    })
}

/// The digests the registry pins for its embeddings models: a copy of one of
/// those files brought on a key is the file the registry would have fetched.
fn registry_digests() -> Vec<String> {
    match apollia_llm::recommend::EmbeddingCatalogue::embedded() {
        Ok(catalogue) => catalogue
            .models()
            .iter()
            .map(|m| m.sha256.clone())
            .collect(),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "model.import.registry.unreadable"
            );
            Vec::new()
        }
    }
}

/// Copy every file of an MLX folder, sub-folders included. Symbolic links are
/// not followed: a link inside a model folder has no business pointing out.
fn copy_folder(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
