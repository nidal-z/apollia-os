//! Tauri IPC commands for the model cards of the catalogue.
//!
//! - [`list_model_cards`]: the catalogue opens, the signed index of cards is
//!   read (main address, mirror, kept copy) and every card is returned in the
//!   asked language
//! - [`get_publisher_avatars`]: the picture of each publisher, read from its
//!   organisation page once and kept in the data directory
//! - [`get_model_readme`]: the README of a repository, read at its source
//!   when the user asks for it and kept nowhere
//! - [`get_dictation_models`]: the dictation models of the registry, whether
//!   each fits this machine, and the one recommended for it
//!
//! [`list_model_cards`] never reads the runtime: it reads the registry, the
//! data directory, its `apollia.toml` and the organisation policy, which the
//! daemon shares when the application is attached to one, so it serves both
//! modes. [`get_publisher_avatars`] does the same, under the same switch. The
//! two others delegate to the runtime's HTTP API, embedded or
//! attached: `GET /api/v1/llm/registry/readme` and `GET /api/v1/llm/dictation`.

use apollia_llm::model_cards::avatar::{default_avatar_dir, publisher_avatar};
use apollia_llm::model_cards::fetch::{open_catalogue, CatalogueView, RegistrySource};
use apollia_llm::model_cards::readme::HUGGINGFACE_BASE;
use apollia_llm::model_cards::{compiled_public_key, default_cache_dir};
use tauri::State;

use crate::attached::RuntimeLink;

const README_PATH: &str = "/api/v1/llm/registry/readme";
const DICTATION_PATH: &str = "/api/v1/llm/dictation";

/// The route that reads the README of `repo`, an `owner/name` identifier.
fn readme_path(repo: &str) -> String {
    format!("{README_PATH}?repo={}", urlencoding::encode(repo))
}

/// The data directory of this installation.
pub(crate) fn data_dir() -> std::path::PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
}

/// Opens the catalogue: the cards in `lang` (`fr` or `en`, English when the
/// language is not written), or an empty catalogue with the reason.
#[tauri::command]
pub async fn list_model_cards(lang: Option<String>) -> Result<CatalogueView, String> {
    let data_dir = data_dir();
    let location = apollia_permissions::policy_location();
    let policy = apollia_permissions::load_policy(&location.dir, location.owner);
    let source =
        RegistrySource::from_config_file(&data_dir.join(apollia_core::paths::CONFIG_FILE_NAME))
            .under_policy(&policy.imposed_settings());
    open_catalogue(
        &source,
        &default_cache_dir(&data_dir),
        compiled_public_key(),
        lang.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(|e| e.to_string())
}

/// How many organisations one call may ask for.
const MAX_AVATAR_ORGS: usize = 48;

/// The picture of each organisation of `orgs`, as a `data:` address keyed by
/// the name asked. An organisation without a picture, or one that could not
/// be reached, is absent from the answer. A picture is asked at its source
/// once, then read from the copy kept in the data directory. Nothing is asked
/// when the model registry is turned off, by configuration or by policy.
#[tauri::command]
pub async fn get_publisher_avatars(
    orgs: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let data_dir = data_dir();
    let location = apollia_permissions::policy_location();
    let policy = apollia_permissions::load_policy(&location.dir, location.owner);
    let source =
        RegistrySource::from_config_file(&data_dir.join(apollia_core::paths::CONFIG_FILE_NAME))
            .under_policy(&policy.imposed_settings());
    if !source.enabled || source.disabled_by_policy {
        return Ok(std::collections::HashMap::new());
    }
    let cache_dir = default_avatar_dir(&data_dir);
    let asked = orgs.into_iter().take(MAX_AVATAR_ORGS).map(|org| {
        let cache_dir = cache_dir.clone();
        tokio::spawn(async move {
            let picture = publisher_avatar(HUGGINGFACE_BASE, &cache_dir, &org).await;
            (org, picture)
        })
    });
    let mut pictures = std::collections::HashMap::new();
    for task in asked.collect::<Vec<_>>() {
        if let Ok((org, Some(picture))) = task.await {
            pictures.insert(org, picture);
        }
    }
    Ok(pictures)
}

/// The README of `repo` as text, with the repository, its owner and the
/// licence it declares. Read at the source on each call; nothing of it is
/// written to disk. Rejected with the runtime's message when the network is
/// cut, the repository has no README, or the text passes the size cap.
#[tauri::command]
pub async fn get_model_readme(
    link: State<'_, RuntimeLink>,
    repo: String,
) -> Result<serde_json::Value, String> {
    link.get_json(&readme_path(&repo)).await
}

/// The dictation models of the registry, each with its footprint and whether
/// it fits this machine, and the one recommended beside the largest language
/// model installed. Answered without the network.
#[tauri::command]
pub async fn get_dictation_models(
    link: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    link.get_json(DICTATION_PATH).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_readme_route_carries_the_repository_as_one_encoded_query_value() {
        // GIVEN a repository identifier, and one that tries to add a query key
        let repo = "Qwen/Qwen3-8B";
        let hostile = "Qwen/Qwen3-8B&lang=fr";

        // WHEN the route is built for each
        let plain = readme_path(repo);
        let escaped = readme_path(hostile);

        // THEN the slash and the ampersand travel inside the one `repo` value,
        // which the route then refuses or accepts as a whole
        assert_eq!(plain, "/api/v1/llm/registry/readme?repo=Qwen%2FQwen3-8B");
        assert_eq!(
            escaped,
            "/api/v1/llm/registry/readme?repo=Qwen%2FQwen3-8B%26lang%3Dfr"
        );
    }
}
