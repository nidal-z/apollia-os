//! Download the signed index of model cards, when the catalogue opens.
//!
//! One plain `GET` for the index and one for its signature, with no body, no
//! query, no cookie and no identifying header: the registry is a static file,
//! and it learns nothing about who reads it. The main address is tried first,
//! the mirror when the main one does not answer, then the copy kept from the
//! last successful read. A signature that does not hold is a refusal, never a
//! reason to try elsewhere.

use std::path::Path;
use std::time::Duration;

use apollia_core::config::PolicySettings;
use apollia_core::ModelsConfig;

use super::{cached_index, store_index, verify_index, CardError, RegistryIndex};

/// Largest index accepted, in bytes.
pub const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;

/// Largest signature accepted, in bytes.
pub const MAX_SIGNATURE_BYTES: u64 = 4 * 1024;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Where the index is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrySource {
    /// Main address of the index; its signature is at the same address with
    /// `.minisig` appended.
    pub url: String,
    /// Address tried when the main one does not answer.
    pub mirror_url: Option<String>,
    /// `false` when the registry is turned off.
    pub enabled: bool,
    /// `true` when the organisation policy turned the registry off, so the
    /// catalogue says the choice is not the user's.
    pub disabled_by_policy: bool,
}

impl From<&ModelsConfig> for RegistrySource {
    fn from(config: &ModelsConfig) -> Self {
        let mirror = config.registry_mirror_url.trim();
        Self {
            url: config.registry_url.trim().to_owned(),
            mirror_url: (!mirror.is_empty()).then(|| mirror.to_owned()),
            enabled: config.registry_enabled,
            disabled_by_policy: false,
        }
    }
}

/// The settings of `[models]` an organisation policy may fix.
const URL_SETTING: &str = "models.registry_url";
const MIRROR_SETTING: &str = "models.registry_mirror_url";
const ENABLED_SETTING: &str = "models.registry_enabled";

impl RegistrySource {
    /// This source with the registry settings the organisation policy fixes
    /// laid over it: an internal mirror in place of the address, no fallback,
    /// or the registry turned off.
    #[must_use]
    pub fn under_policy(mut self, imposed: &PolicySettings) -> Self {
        let text = |key| {
            imposed
                .imposed(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
        };
        if let Some(url) = text(URL_SETTING) {
            url.clone_into(&mut self.url);
        }
        if let Some(mirror) = text(MIRROR_SETTING) {
            self.mirror_url = (!mirror.is_empty()).then(|| mirror.to_owned());
        }
        if let Some(enabled) = imposed
            .imposed(ENABLED_SETTING)
            .and_then(serde_json::Value::as_bool)
        {
            self.enabled = enabled;
            self.disabled_by_policy = !enabled;
        }
        self
    }

    /// The source an `apollia.toml` text configures: its `[models]` section,
    /// or the defaults when the section is absent.
    ///
    /// # Errors
    ///
    /// The TOML error when the text does not parse or the section does not
    /// have the expected shape.
    pub fn from_toml_str(content: &str) -> Result<Self, toml::de::Error> {
        #[derive(serde::Deserialize)]
        struct Sections {
            models: Option<ModelsConfig>,
        }
        let config = toml::from_str::<Sections>(content)?
            .models
            .unwrap_or_default();
        Ok(Self::from(&config))
    }
}

/// Where the index that was returned came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexOrigin {
    /// The main address.
    Primary,
    /// The mirror.
    Mirror,
    /// The copy kept from the last successful read, no address answering.
    Cache,
    /// The index that ships with the application, no signed index having
    /// ever been accepted on this machine.
    Shipped,
}

/// An index, checked, with where it came from.
#[derive(Debug, Clone)]
pub struct LoadedIndex {
    /// The index.
    pub index: RegistryIndex,
    /// Where it came from.
    pub origin: IndexOrigin,
}

/// Why no index was returned.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// The registry is turned off by configuration or policy.
    #[error("the model registry is turned off")]
    Disabled,
    /// No address answered and no copy is kept.
    #[error("the model registry is unreachable and no copy is kept: {detail}")]
    Offline {
        /// What the last attempt reported.
        detail: String,
    },
    /// The index or its copy was refused.
    #[error(transparent)]
    Card(#[from] CardError),
    /// The HTTP client could not be built.
    #[error("the http client could not be built")]
    Client(#[source] reqwest::Error),
}

enum Attempt {
    Fetched { content: Vec<u8>, signature: String },
    Unanswered(String),
}

/// Read the index for the catalogue: main address, then mirror, then the copy
/// kept in `cache_dir`, then the index that ships with the application. A
/// fetched index is checked against `public_key`, then kept.
///
/// # Errors
///
/// [`RegistryError::Disabled`] when the source is turned off, and
/// [`RegistryError::Card`] when a signature does not hold.
pub async fn load_index(
    source: &RegistrySource,
    cache_dir: &Path,
    public_key: Option<&str>,
) -> Result<LoadedIndex, RegistryError> {
    if !source.enabled {
        return Err(RegistryError::Disabled);
    }
    if public_key.is_none() {
        // No key, so no fetched index could be accepted: nothing is asked.
        return shipped();
    }
    let mut detail = String::new();
    let candidates = std::iter::once((source.url.as_str(), IndexOrigin::Primary)).chain(
        source
            .mirror_url
            .as_deref()
            .map(|m| (m, IndexOrigin::Mirror)),
    );
    for (url, origin) in candidates {
        match fetch_pair(url).await? {
            Attempt::Fetched { content, signature } => {
                let index = verify_index(&content, &signature, public_key)?;
                if let Err(error) = store_index(cache_dir, &content, &signature) {
                    tracing::event!(
                        tracing::Level::WARN,
                        error = %error,
                        "llm.model_cards.cache_write_failed"
                    );
                }
                tracing::event!(
                    tracing::Level::INFO,
                    url = %url,
                    cards = index.cards.len(),
                    "llm.model_cards.index_fetched"
                );
                return Ok(LoadedIndex { index, origin });
            }
            Attempt::Unanswered(why) => {
                tracing::event!(
                    tracing::Level::INFO,
                    url = %url,
                    detail = %why,
                    "llm.model_cards.address_unanswered"
                );
                detail = why;
            }
        }
    }
    match cached_index(cache_dir, public_key)? {
        Some(index) => Ok(LoadedIndex {
            index,
            origin: IndexOrigin::Cache,
        }),
        None => {
            tracing::event!(
                tracing::Level::INFO,
                detail = %detail,
                "llm.model_cards.shipped_index_read"
            );
            shipped()
        }
    }
}

fn shipped() -> Result<LoadedIndex, RegistryError> {
    Ok(LoadedIndex {
        index: super::shipped_index()?,
        origin: IndexOrigin::Shipped,
    })
}

async fn fetch_pair(url: &str) -> Result<Attempt, RegistryError> {
    let client = apollia_core::net::configured_endpoint_client_builder_for(url)
        .timeout(TIMEOUT)
        .build()
        .map_err(RegistryError::Client)?;
    let content = match fetch_capped(&client, url, MAX_INDEX_BYTES).await {
        Ok(bytes) => bytes,
        Err(why) => return Ok(Attempt::Unanswered(why)),
    };
    let signature_url = format!("{url}.minisig");
    let signature = match fetch_capped(&client, &signature_url, MAX_SIGNATURE_BYTES).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(why) => return Ok(Attempt::Unanswered(why)),
    };
    Ok(Attempt::Fetched { content, signature })
}

async fn fetch_capped(client: &reqwest::Client, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url} answered {status}"));
    }
    apollia_core::net::read_capped_bytes(response, limit)
        .await
        .map_err(|e| e.to_string())
}

impl RegistrySource {
    /// The source the `apollia.toml` at `path` configures. A missing file or
    /// an unreadable section gives the defaults, the second with a warning.
    #[must_use]
    pub fn from_config_file(path: &Path) -> Self {
        let defaults = || Self::from(&ModelsConfig::default());
        let Ok(content) = std::fs::read_to_string(path) else {
            return defaults();
        };
        match Self::from_toml_str(&content) {
            Ok(source) => source,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %path.display(),
                    error = %error,
                    detail = "the [models] section is unreadable, the defaults apply",
                    "llm.model_cards.config_unreadable"
                );
                defaults()
            }
        }
    }
}

/// What the catalogue shows when it opens: the cards in one language, or an
/// empty list with the reason and a sentence for the user.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogueView {
    /// `ok`, `offline`, `disabled` or `no_trusted_key`.
    pub status: String,
    /// Where the index came from when `status` is `ok`.
    pub origin: Option<IndexOrigin>,
    /// A sentence for the user when the catalogue is empty.
    pub message: Option<String>,
    /// Every card, in the asked language.
    pub cards: Vec<super::LocalizedCard>,
    /// The official publishers and the recognised converters.
    pub publishers: super::PublisherLists,
}

impl CatalogueView {
    fn empty(status: &str, message: &str) -> Self {
        Self {
            status: status.to_owned(),
            origin: None,
            message: Some(message.to_owned()),
            cards: Vec::new(),
            publishers: super::PublisherLists::default(),
        }
    }
}

/// Open the catalogue: [`load_index`], then every card in `lang`. Offline, a
/// registry turned off and a build without a key give an empty catalogue that
/// says why; a refused index stays an error.
///
/// # Errors
///
/// [`RegistryError::Card`] when the index or its copy is refused for another
/// reason than a missing key, and [`RegistryError::Client`].
pub async fn open_catalogue(
    source: &RegistrySource,
    cache_dir: &Path,
    public_key: Option<&str>,
    lang: &str,
) -> Result<CatalogueView, RegistryError> {
    if source.disabled_by_policy {
        return Ok(CatalogueView::empty(
            "disabled",
            "The model catalogue is turned off, imposed by the organisation policy. A model can still be imported from a disk.",
        ));
    }
    match load_index(source, cache_dir, public_key).await {
        Ok(loaded) => Ok(CatalogueView {
            status: "ok".to_owned(),
            origin: Some(loaded.origin),
            message: None,
            cards: loaded
                .index
                .cards
                .iter()
                .map(|c| c.localized(lang))
                .collect(),
            publishers: loaded.index.publishers,
        }),
        Err(RegistryError::Offline { .. }) => Ok(CatalogueView::empty(
            "offline",
            "The model catalogue could not be reached and no copy is kept. A model can still be imported from a disk.",
        )),
        Err(RegistryError::Disabled) => Ok(CatalogueView::empty(
            "disabled",
            "The model catalogue is turned off on this machine. A model can still be imported from a disk.",
        )),
        Err(RegistryError::Card(CardError::NoTrustedKey)) => Ok(CatalogueView::empty(
            "no_trusted_key",
            "This build carries no distribution key, so no model catalogue is accepted.",
        )),
        Err(other) => Err(other),
    }
}
