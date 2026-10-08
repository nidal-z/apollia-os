//! Model cards: what Apollia says about each model of its catalogue.
//!
//! A card is short and written for a business user, in French and in English:
//! what the model is good at, what it asks of the machine, the sampling values
//! its publisher recommends, and the switches its chat template exposes. The
//! cards live in the repository under `registry/models/`, are gathered into one
//! index by the release pipeline, signed there in the minisign format with the
//! distribution key, and served as a static file.
//!
//! This module reads that index and nothing else:
//!
//! - [`fetch`] downloads it when the catalogue opens, checks its signature
//!   against the key compiled into the build, keeps a copy, and falls back on
//!   the mirror then on that copy;
//! - [`copy_card_beside`] writes the card of a model next to its weights when
//!   the model is downloaded, so it is read offline from then on
//!   ([`installed_card`]);
//! - [`readme`] reads the README of a repository on demand, as text, and keeps
//!   nothing on disk.
//!
//! What a card says about capabilities is informative. The capabilities an
//! installed model is used with are those the runtime computes from its chat
//! template ([`crate::capabilities`]); the guard of the card files refuses a
//! card that contradicts that computation on a recorded template. The author
//! fields of a card describe a switch and its default: none of them is ever
//! sent to an engine.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(feature = "cloud")]
pub mod avatar;
#[cfg(feature = "cloud")]
pub mod fetch;
#[cfg(feature = "cloud")]
pub mod readme;

/// The schema version of the index this build reads.
pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// Name of the file a card is copied to inside an MLX model folder.
pub const FOLDER_CARD_FILE_NAME: &str = "apollia-card.json";

/// Suffix appended to the weights file name of a single-file model to name the
/// copy of its card.
pub const FILE_CARD_SUFFIX: &str = ".card.json";

/// Name of the cached index inside the cache directory.
pub const CACHED_INDEX_FILE_NAME: &str = "index.json";

/// Name of the cached signature inside the cache directory.
pub const CACHED_SIGNATURE_FILE_NAME: &str = "index.json.minisig";

/// A text in the two languages of the product.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Localized {
    /// French.
    pub fr: String,
    /// English.
    pub en: String,
}

impl Localized {
    /// The text in `lang`, English when `lang` is not French or the French
    /// text is empty. Returns the text and the language it is in.
    #[must_use]
    pub fn pick(&self, lang: &str) -> (&str, &'static str) {
        if is_french(lang) && !self.fr.trim().is_empty() {
            (&self.fr, "fr")
        } else {
            (&self.en, "en")
        }
    }
}

fn is_french(lang: &str) -> bool {
    lang.trim().to_ascii_lowercase().starts_with("fr")
}

/// What kind of model a card describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    /// A language model, the one a conversation runs on.
    Language,
    /// A model that turns a text into a vector for search by meaning.
    Embeddings,
    /// A speech-to-text model.
    Dictation,
}

/// The licence of a model, with where it was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardLicense {
    /// SPDX identifier, or the identifier the publisher declares.
    pub id: String,
    /// Address the licence was read from.
    pub source: String,
}

/// The capabilities a card declares.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardCapabilities {
    /// The chat template carries tool definitions and tool calls.
    pub tools: bool,
    /// The chat template opens a reasoning block.
    pub reasoning: bool,
    /// The model reads images.
    pub vision: bool,
    /// Address the three facts were read from.
    pub source: String,
}

/// The memory a model asks for, as information shown to the user. Never a
/// criterion of decision: the estimator decides what fits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardMemory {
    /// Gigabytes (10^9 bytes), rounded up to the tenth.
    pub min_gb: f64,
    /// Address the size it is computed from was read from.
    pub source: String,
}

/// The mark of a model published to draft for a larger one in speculative
/// decoding, with where that is shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDraft {
    /// Address that shows the model published or used as a draft model.
    pub source: String,
}

/// Sampling values the publisher recommends, each optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardDefaults {
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p (nucleus) sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Min-p sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f32>,
    /// Repetition penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repetition_penalty: Option<f32>,
    /// Presence penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Address the values were read from.
    pub source: String,
}

/// The type of an author field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorFieldType {
    /// One value among a list.
    #[serde(rename = "liste")]
    List,
    /// On or off.
    #[serde(rename = "booleen")]
    Boolean,
}

/// A switch the chat template of a model exposes, described for the user.
///
/// Data only: the runtime never sends an author field to an engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorField {
    /// Name of the switch, as the template reads it.
    pub name: String,
    /// Its type.
    #[serde(rename = "type")]
    pub field_type: AuthorFieldType,
    /// The values a list accepts; empty for a boolean.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<serde_json::Value>,
    /// The value the template uses when nothing is sent.
    pub default: serde_json::Value,
    /// What the switch changes, written by Apollia.
    pub explanation: Localized,
    /// Address the name, type, values and default were read from.
    pub source: String,
}

/// One card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCard {
    /// Stable identifier: `family/upstream-name` for a language model, the
    /// entry identifier for an embeddings or dictation model.
    pub id: String,
    /// What kind of model.
    pub kind: CardKind,
    /// The name the user reads.
    pub label: String,
    /// The organisation that trained the model, in text.
    pub publisher: String,
    /// Its licence.
    pub license: CardLicense,
    /// The page of the original repository, `https://huggingface.co/<repo>`.
    pub source_url: String,
    /// What the model is good at, written by Apollia.
    pub description: Localized,
    /// What it can do.
    pub capabilities: CardCapabilities,
    /// What it asks of the machine.
    pub memory: CardMemory,
    /// The sampling values its publisher recommends.
    pub defaults: CardDefaults,
    /// The switches its chat template exposes.
    #[serde(default)]
    pub author_fields: Vec<AuthorField>,
    /// Repositories, `owner/name`, whose files this card describes: the
    /// original and its conversions.
    #[serde(default)]
    pub repos: Vec<String>,
    /// Set when the model is published to draft for a larger model of the
    /// same vocabulary. It stays a language model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<CardDraft>,
}

impl ModelCard {
    /// Whether this card describes the files of `repo_id`.
    #[must_use]
    pub fn describes(&self, repo_id: &str) -> bool {
        self.repos.iter().any(|r| r.eq_ignore_ascii_case(repo_id))
    }

    /// The card as one language reads it.
    #[must_use]
    pub fn localized(&self, lang: &str) -> LocalizedCard {
        let (description, language) = self.description.pick(lang);
        LocalizedCard {
            id: self.id.clone(),
            kind: self.kind,
            label: self.label.clone(),
            publisher: self.publisher.clone(),
            language: language.to_owned(),
            description: description.to_owned(),
            license: self.license.clone(),
            source_url: self.source_url.clone(),
            capabilities: self.capabilities.clone(),
            memory: self.memory.clone(),
            defaults: self.defaults.clone(),
            author_fields: self
                .author_fields
                .iter()
                .map(|f| LocalizedAuthorField {
                    name: f.name.clone(),
                    field_type: f.field_type,
                    values: f.values.clone(),
                    default: f.default.clone(),
                    explanation: f.explanation.pick(lang).0.to_owned(),
                    source: f.source.clone(),
                })
                .collect(),
            repos: self.repos.clone(),
        }
    }
}

/// An author field in one language.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalizedAuthorField {
    /// Name of the switch.
    pub name: String,
    /// Its type.
    #[serde(rename = "type")]
    pub field_type: AuthorFieldType,
    /// The values a list accepts.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<serde_json::Value>,
    /// The value the template uses when nothing is sent.
    pub default: serde_json::Value,
    /// What the switch changes.
    pub explanation: String,
    /// Address the facts were read from.
    pub source: String,
}

/// A card in one language, as the routes serve it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalizedCard {
    /// Stable identifier.
    pub id: String,
    /// What kind of model.
    pub kind: CardKind,
    /// The name the user reads.
    pub label: String,
    /// The organisation that trained the model.
    pub publisher: String,
    /// The language of the texts below: `fr` or `en`.
    pub language: String,
    /// What the model is good at.
    pub description: String,
    /// Its licence.
    pub license: CardLicense,
    /// The page of the original repository.
    pub source_url: String,
    /// What it can do.
    pub capabilities: CardCapabilities,
    /// What it asks of the machine.
    pub memory: CardMemory,
    /// The sampling values its publisher recommends.
    pub defaults: CardDefaults,
    /// The switches its chat template exposes.
    pub author_fields: Vec<LocalizedAuthorField>,
    /// Repositories this card describes.
    pub repos: Vec<String>,
}

/// An organisation that publishes models under its own name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialPublisher {
    /// The name the user reads.
    pub publisher: String,
    /// Its HuggingFace organisations.
    pub orgs: Vec<String>,
}

/// The two lists that classify the owner of a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublisherLists {
    /// Official publishers.
    #[serde(default)]
    pub official: Vec<OfficialPublisher>,
    /// Recognised converters.
    #[serde(default)]
    pub converters: Vec<String>,
}

/// The signed index: every card, and the publisher lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryIndex {
    /// Schema version, [`INDEX_SCHEMA_VERSION`] for this build.
    pub schema_version: u32,
    /// Every card.
    pub cards: Vec<ModelCard>,
    /// The publisher lists, so they follow the index without a new release.
    #[serde(default)]
    pub publishers: PublisherLists,
}

impl RegistryIndex {
    /// Parse an index, refusing a schema this build does not read.
    ///
    /// # Errors
    ///
    /// [`CardError::Invalid`] when the bytes are not an index, and
    /// [`CardError::UnsupportedSchema`] when its version is not this build's.
    pub fn parse(bytes: &[u8]) -> Result<Self, CardError> {
        let index: Self =
            serde_json::from_slice(bytes).map_err(|e| CardError::Invalid(e.to_string()))?;
        if index.schema_version != INDEX_SCHEMA_VERSION {
            return Err(CardError::UnsupportedSchema(index.schema_version));
        }
        Ok(index)
    }

    /// The card whose identifier is `id`.
    #[must_use]
    pub fn card(&self, id: &str) -> Option<&ModelCard> {
        self.cards.iter().find(|c| c.id == id)
    }

    /// The card that describes the files of `repo_id`.
    #[must_use]
    pub fn card_for_repo(&self, repo_id: &str) -> Option<&ModelCard> {
        self.cards.iter().find(|c| c.describes(repo_id))
    }
}

/// Errors raised while reading cards from disk or from an index.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CardError {
    /// The bytes are not an index or a card.
    #[error("invalid model card data: {0}")]
    Invalid(String),
    /// The index declares a schema version this build does not read.
    #[error("unsupported model card index schema version {0}")]
    UnsupportedSchema(u32),
    /// This build carries no distribution key, so no index is accepted.
    #[error("this build carries no distribution key, no model card index is accepted")]
    NoTrustedKey,
    /// The signature of the index does not hold for this content and key.
    #[error("the model card index signature is rejected")]
    Signature(#[source] apollia_signing::distribution::DistributionError),
    /// Reading or writing a file failed.
    #[error("model card file error at {path}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
}

/// The distribution key compiled into this build, the only key an index is
/// accepted from. `None` when the build carries none.
#[must_use]
pub fn compiled_public_key() -> Option<&'static str> {
    apollia_signing::distribution::distribution_public_key()
}

/// Where the card of the model at `model_path` is copied: inside the folder
/// of an MLX model, beside the weights file otherwise.
#[must_use]
pub fn card_path_for(model_path: &Path) -> PathBuf {
    if model_path.is_dir() {
        return model_path.join(FOLDER_CARD_FILE_NAME);
    }
    let mut name = model_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(FILE_CARD_SUFFIX);
    model_path.with_file_name(name)
}

/// The cache directory of the index under the data directory.
#[must_use]
pub fn default_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("model-registry")
}

/// Check `signature` over `content` with `public_key`, then parse the index.
///
/// # Errors
///
/// [`CardError::NoTrustedKey`] when `public_key` is `None`,
/// [`CardError::Signature`] when the signature does not hold, and the errors
/// of [`RegistryIndex::parse`].
pub fn verify_index(
    content: &[u8],
    signature: &str,
    public_key: Option<&str>,
) -> Result<RegistryIndex, CardError> {
    let key = public_key.ok_or(CardError::NoTrustedKey)?;
    apollia_signing::distribution::verify_minisign(content, signature, key)
        .map_err(CardError::Signature)?;
    RegistryIndex::parse(content)
}

/// The index kept in `cache_dir`, checked again against `public_key`.
/// `Ok(None)` when no copy is kept.
///
/// # Errors
///
/// The errors of [`verify_index`], and [`CardError::Io`] when a cached file
/// exists but cannot be read.
pub fn cached_index(
    cache_dir: &Path,
    public_key: Option<&str>,
) -> Result<Option<RegistryIndex>, CardError> {
    let index_path = cache_dir.join(CACHED_INDEX_FILE_NAME);
    let signature_path = cache_dir.join(CACHED_SIGNATURE_FILE_NAME);
    let Some(content) = read_if_present(&index_path)? else {
        return Ok(None);
    };
    let Some(signature) = read_if_present(&signature_path)? else {
        return Ok(None);
    };
    let signature = String::from_utf8_lossy(&signature);
    verify_index(&content, &signature, public_key).map(Some)
}

/// The index built from the cards of this repository when this build was made.
const SHIPPED_INDEX: &[u8] = include_bytes!("shipped-index.json");

/// The index that ships with the application: what the catalogue reads when
/// no signed index was ever accepted on this machine. It needs no signature,
/// being part of the binary, and it is as old as the build.
///
/// # Errors
///
/// The errors of [`RegistryIndex::parse`].
pub fn shipped_index() -> Result<RegistryIndex, CardError> {
    RegistryIndex::parse(SHIPPED_INDEX)
}

/// The index to read without asking the network: the signed copy kept in
/// `cache_dir` when there is one, the index that ships with the application
/// otherwise.
///
/// # Errors
///
/// The errors of [`cached_index`] when a kept copy is refused, and those of
/// [`shipped_index`].
pub fn kept_or_shipped_index(
    cache_dir: &Path,
    public_key: Option<&str>,
) -> Result<RegistryIndex, CardError> {
    if public_key.is_some() {
        if let Some(index) = cached_index(cache_dir, public_key)? {
            return Ok(index);
        }
    }
    shipped_index()
}

/// Keep `content` and its `signature` in `cache_dir`, each written whole
/// before it replaces the previous copy.
///
/// # Errors
///
/// [`CardError::Io`] when a file cannot be written.
pub fn store_index(cache_dir: &Path, content: &[u8], signature: &str) -> Result<(), CardError> {
    std::fs::create_dir_all(cache_dir).map_err(|source| CardError::Io {
        path: cache_dir.to_path_buf(),
        source,
    })?;
    write_whole(&cache_dir.join(CACHED_INDEX_FILE_NAME), content)?;
    write_whole(
        &cache_dir.join(CACHED_SIGNATURE_FILE_NAME),
        signature.as_bytes(),
    )
}

/// Copy the card of the repository `repo_id` next to the model at
/// `model_path`, from the index kept in `cache_dir`, or from the one that
/// ships with the application.
///
/// Returns where the card was written, or `None` when no card describes that
/// repository.
///
/// # Errors
///
/// The errors of [`kept_or_shipped_index`], and [`CardError::Io`] when the
/// card cannot be written.
pub fn copy_card_beside(
    cache_dir: &Path,
    model_path: &Path,
    repo_id: &str,
    public_key: Option<&str>,
) -> Result<Option<PathBuf>, CardError> {
    let index = kept_or_shipped_index(cache_dir, public_key)?;
    let Some(card) = index.card_for_repo(repo_id) else {
        return Ok(None);
    };
    let path = card_path_for(model_path);
    let json = serde_json::to_vec_pretty(card).map_err(|e| CardError::Invalid(e.to_string()))?;
    write_whole(&path, &json)?;
    tracing::event!(
        tracing::Level::INFO,
        card = %card.id,
        repo = %repo_id,
        path = %path.display(),
        "llm.model_cards.copied"
    );
    Ok(Some(path))
}

/// The card copied next to the model at `model_path`, read from disk only.
/// `Ok(None)` when the model has no card.
///
/// # Errors
///
/// [`CardError::Io`] when the copy exists but cannot be read, and
/// [`CardError::Invalid`] when it is not a card.
pub fn installed_card(model_path: &Path) -> Result<Option<ModelCard>, CardError> {
    let path = card_path_for(model_path);
    let Some(bytes) = read_if_present(&path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| CardError::Invalid(e.to_string()))
}

fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, CardError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CardError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), CardError> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|source| CardError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| CardError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests;
