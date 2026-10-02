//! The card Apollia generates for a search result.
//!
//! A repository found by the bounded search has no card of its own in the
//! signed index. Its card is built when the result is read, and lives as long
//! as the answer that carries it:
//!
//! - when the model it carries has an Apollia card (its `base_model`, or the
//!   repository itself listed among the repositories a card describes), the
//!   generated card inherits the description, the author fields and the
//!   sampling defaults, and adds what belongs to the repository: who converted
//!   it, its format, its variants, its size and its date;
//! - otherwise it is made of facts only: one sentence Apollia builds from the
//!   names, the parameter count, the architecture, the context length, the
//!   capabilities computed from the chat template, and the declared licence.
//!
//! Nothing a repository writes in prose (its README, its description) is ever
//! copied into a generated card.

use serde::Serialize;

use crate::capabilities::Capabilities;
use crate::hf_registry::parse::extract_base_model_from_json;
use crate::hf_registry::{HfModelCard, SearchFormat};
use crate::model_cards::{AuthorField, CardDefaults, Localized, RegistryIndex};
use crate::recommend::{ModelVariant, PublisherKind};

/// The card of one search result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeneratedCard {
    /// The repository, `owner/name`.
    pub repo_id: String,
    /// Its page, `https://huggingface.co/<repo>`.
    pub source_url: String,
    /// The owner of the repository.
    pub owner: String,
    /// Whether the owner is an official publisher or a recognised converter.
    pub publisher_kind: Option<PublisherKind>,
    /// The model the repository carries, when it declares one.
    pub origin_repo: Option<String>,
    /// The Apollia card this card inherits from, when one describes the model.
    pub origin_card_id: Option<String>,
    /// The format of the files.
    pub format: SearchFormat,
    /// What the model is: the inherited description, or the sentence Apollia
    /// builds from the names.
    pub description: Localized,
    /// The switches of the chat template, inherited from the Apollia card.
    pub author_fields: Vec<AuthorField>,
    /// The sampling values, inherited from the Apollia card.
    pub defaults: Option<CardDefaults>,
    /// Parameter count, as the Hub reads it from the weights.
    pub parameters: Option<u64>,
    /// Architecture the weights declare.
    pub architecture: Option<String>,
    /// Context length the weights declare.
    pub context_length: Option<u64>,
    /// What the model can do, computed from its chat template and files.
    pub capabilities: Capabilities,
    /// The licence the repository declares.
    pub license: Option<String>,
    /// The short list of variants, each sized and graded for this machine.
    pub variants: Vec<ModelVariant>,
    /// Bytes of the recommended variant, else of the smallest one.
    pub size_bytes: Option<u64>,
    /// When the repository last changed (RFC 3339).
    pub last_modified: Option<String>,
}

impl GeneratedCard {
    /// Whether a variant of this card is recommended for this machine.
    #[must_use]
    pub fn has_recommendation(&self) -> bool {
        self.variants.iter().any(|v| v.recommended)
    }
}

/// Build the card of `model`, read from the search item `item`, against the
/// cards of `index`. The variants of `model` move into the card.
pub(super) fn generate(
    model: &mut HfModelCard,
    item: &serde_json::Value,
    format: SearchFormat,
    index: Option<&RegistryIndex>,
) -> GeneratedCard {
    let owner = model.author.clone().unwrap_or_else(|| {
        model
            .repo_id
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    });
    let origin_repo = extract_base_model_from_json(item)
        .filter(|origin| !origin.eq_ignore_ascii_case(&model.repo_id));
    let origin_card = index.and_then(|index| {
        origin_repo
            .as_deref()
            .and_then(|origin| index.card_for_repo(origin))
            .or_else(|| index.card_for_repo(&model.repo_id))
    });

    let variants = std::mem::take(&mut model.variants);
    let size_bytes = variants
        .iter()
        .find(|v| v.recommended)
        .or_else(|| variants.iter().min_by_key(|v| v.size_bytes))
        .map(|v| v.size_bytes);

    let (description, author_fields, defaults, origin_card_id) = match origin_card {
        Some(card) => (
            card.description.clone(),
            card.author_fields.clone(),
            Some(card.defaults.clone()),
            Some(card.id.clone()),
        ),
        None => (
            built_sentence(format, &owner, origin_repo.as_deref()),
            Vec::new(),
            None,
            None,
        ),
    };

    GeneratedCard {
        source_url: format!("https://huggingface.co/{}", model.repo_id),
        repo_id: model.repo_id.clone(),
        owner,
        publisher_kind: model.publisher_kind,
        origin_repo,
        origin_card_id,
        format,
        description,
        author_fields,
        defaults,
        parameters: parameters(item),
        architecture: architecture(item),
        context_length: item
            .get("gguf")
            .and_then(|g| g.get("context_length"))
            .and_then(serde_json::Value::as_u64),
        capabilities: model.capabilities.clone(),
        license: model.license.clone(),
        variants,
        size_bytes,
        last_modified: model.last_modified.clone(),
    }
}

/// The two sentences a card made of facts is written with, one per locale,
/// kept in a data file so this module holds no prose of either language.
#[derive(Debug, serde::Deserialize)]
struct Sentences {
    /// A repository that carries another repository's model.
    conversion: Localized,
    /// A repository that declares no origin.
    publication: Localized,
}

static SENTENCES: std::sync::LazyLock<Option<Sentences>> = std::sync::LazyLock::new(|| {
    match serde_json::from_str(include_str!("generated_sentences.json")) {
        Ok(sentences) => Some(sentences),
        Err(err) => {
            tracing::event!(tracing::Level::ERROR, error = %err, "llm.hf_search.sentences_invalid");
            None
        }
    }
});

/// The sentence of a card made of facts: what the files are, and by whom.
fn built_sentence(format: SearchFormat, owner: &str, origin: Option<&str>) -> Localized {
    let Some(sentences) = SENTENCES.as_ref() else {
        return Localized::default();
    };
    let template = if origin.is_some() {
        &sentences.conversion
    } else {
        &sentences.publication
    };
    let fill = |text: &str| {
        text.replace("{format}", format.label())
            .replace("{origin}", origin.unwrap_or_default())
            .replace("{owner}", owner)
    };
    Localized {
        fr: fill(&template.fr),
        en: fill(&template.en),
    }
}

/// Parameter count: the GGUF header first, then the safetensors index.
fn parameters(item: &serde_json::Value) -> Option<u64> {
    item.get("gguf")
        .and_then(|g| g.get("total"))
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            item.get("safetensors")
                .and_then(|s| s.get("total"))
                .and_then(serde_json::Value::as_u64)
        })
}

/// Architecture: the GGUF header first, then `config.json`.
fn architecture(item: &serde_json::Value) -> Option<String> {
    item.get("gguf")
        .and_then(|g| g.get("architecture"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            item.get("config")
                .and_then(|c| c.get("model_type"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_owned)
}
