//! Tauri IPC command for the model cards of the catalogue.
//!
//! - [`list_model_cards`]: the catalogue opens, the signed index of cards is
//!   read (main address, mirror, kept copy) and every card is returned in the
//!   asked language
//!
//! The command never reads the runtime: it reads the registry, the data
//! directory and its `apollia.toml`, which the daemon shares when the
//! application is attached to one, so it serves both modes.

use apollia_llm::model_cards::fetch::{open_catalogue, CatalogueView, RegistrySource};
use apollia_llm::model_cards::{compiled_public_key, default_cache_dir};

/// The data directory of this installation.
fn data_dir() -> std::path::PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
}

/// Opens the catalogue: the cards in `lang` (`fr` or `en`, English when the
/// language is not written), or an empty catalogue with the reason.
#[tauri::command]
pub async fn list_model_cards(lang: Option<String>) -> Result<CatalogueView, String> {
    let data_dir = data_dir();
    let source = RegistrySource::from_config_file(&data_dir.join("apollia.toml"));
    open_catalogue(
        &source,
        &default_cache_dir(&data_dir),
        compiled_public_key(),
        lang.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Copy the card of `repo_id`, from the kept index, next to the model just
/// downloaded at `model_path`, so the card reads offline from then on. A
/// failure is logged and never fails the download.
pub(crate) fn copy_card_for_download(model_path: &std::path::Path, repo_id: Option<&str>) {
    let Some(repo_id) = repo_id else {
        return;
    };
    let cache_dir = default_cache_dir(&data_dir());
    if let Err(error) = apollia_llm::model_cards::copy_card_beside(
        &cache_dir,
        model_path,
        repo_id,
        compiled_public_key(),
    ) {
        tracing::event!(
            tracing::Level::WARN,
            repo = %repo_id,
            path = %model_path.display(),
            error = %error,
            detail = "the download itself is unaffected",
            "desktop.model_cards.copy_failed"
        );
    }
}
