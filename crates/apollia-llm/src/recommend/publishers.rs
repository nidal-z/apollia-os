//! Which HuggingFace organisations Apollia recognises, and as what.
//!
//! `publishers.toml` is embedded beside `families.toml` and holds two lists:
//! the organisations that train models (official publishers) and the short
//! list of converters whose republished files the runtime trusts to load. A
//! repository is classified by its owner alone, before any file is read, so a
//! search result can carry the distinction without a second request.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use super::manifest::ManifestError;

/// The lists as shipped in the binary.
static EMBEDDED_TOML: &str = include_str!("publishers.toml");

/// Schema version this build reads.
const SCHEMA_VERSION: u32 = 1;

/// Who published a repository, relative to the model it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherKind {
    /// The organisation that trained the model.
    Official,
    /// A recognised converter republishing someone else's model.
    Converter,
}

/// One training organisation and the HuggingFace handles it publishes under.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OfficialPublisher {
    /// The name `families.toml` uses in its `publisher` field.
    pub publisher: String,
    /// Its HuggingFace organisations, exact handles.
    pub orgs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawPublishers {
    schema_version: u32,
    #[serde(default)]
    converters: Vec<String>,
    #[serde(default)]
    official: Vec<OfficialPublisher>,
}

/// The two lists, validated.
#[derive(Debug, Clone)]
pub struct PublisherLists {
    official: Vec<OfficialPublisher>,
    converters: Vec<String>,
}

impl PublisherLists {
    /// Load the lists embedded in the binary.
    ///
    /// # Errors
    /// [`ManifestError::Parse`] or [`ManifestError::SchemaVersion`]. A failure
    /// here is a defect in a file that ships with the binary.
    pub fn embedded() -> Result<Self, ManifestError> {
        Self::from_toml(EMBEDDED_TOML)
    }

    /// Load lists from TOML.
    ///
    /// # Errors
    /// See [`PublisherLists::embedded`].
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let raw: RawPublishers = toml::from_str(text)?;
        if raw.schema_version != SCHEMA_VERSION {
            return Err(ManifestError::SchemaVersion {
                found: raw.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(Self {
            official: raw.official,
            converters: raw.converters,
        })
    }

    /// The official publishers, in file order.
    #[must_use]
    pub fn official(&self) -> &[OfficialPublisher] {
        &self.official
    }

    /// The recognised converters, in file order.
    #[must_use]
    pub fn converters(&self) -> &[String] {
        &self.converters
    }

    /// Classify a repository id (`org/name`) or a bare owner handle.
    ///
    /// The Hub treats handles without regard to case, so the comparison does
    /// too: `qwen/Qwen3-8B` is the same owner as `Qwen/Qwen3-8B`.
    #[must_use]
    pub fn kind_of(&self, repo_or_owner: &str) -> Option<PublisherKind> {
        let owner = repo_or_owner.split('/').next().unwrap_or(repo_or_owner);
        if owner.is_empty() {
            return None;
        }
        let official = self
            .official
            .iter()
            .flat_map(|p| p.orgs.iter())
            .any(|org| org.eq_ignore_ascii_case(owner));
        if official {
            return Some(PublisherKind::Official);
        }
        self.converters
            .iter()
            .any(|c| c.eq_ignore_ascii_case(owner))
            .then_some(PublisherKind::Converter)
    }
}

/// The embedded lists, parsed once. `None` only if the shipped file is broken,
/// which the unit tests below rule out for any build that passed them.
static EMBEDDED: LazyLock<Option<PublisherLists>> =
    LazyLock::new(|| match PublisherLists::embedded() {
        Ok(lists) => Some(lists),
        Err(err) => {
            event!(Level::ERROR, error = %err, "recommend.publishers.invalid");
            None
        }
    });

/// Classify a repository or owner against the embedded lists.
#[must_use]
pub fn publisher_kind(repo_or_owner: &str) -> Option<PublisherKind> {
    EMBEDDED.as_ref()?.kind_of(repo_or_owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recommend::FamilyManifest;

    #[test]
    fn the_embedded_lists_name_the_required_publishers_and_converters() {
        // GIVEN the lists shipped in the binary
        // WHEN they are loaded
        let lists = PublisherLists::embedded().expect("the shipped lists must be valid");

        // THEN the seven required publishers and the four converters are there
        let names: Vec<&str> = lists
            .official()
            .iter()
            .map(|p| p.publisher.as_str())
            .collect();
        for required in [
            "Qwen",
            "Mistral AI",
            "Google",
            "Meta",
            "Microsoft",
            "NVIDIA",
            "IBM",
        ] {
            assert!(names.contains(&required), "{required} is missing");
        }
        for required in [
            "lmstudio-community",
            "mlx-community",
            "unsloth",
            "bartowski",
        ] {
            assert!(
                lists.converters().iter().any(|c| c == required),
                "{required} is missing"
            );
        }
    }

    #[test]
    fn every_family_publisher_is_an_official_publisher() {
        // GIVEN the curated generation table and the publisher lists
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");
        let lists = PublisherLists::embedded().expect("the shipped lists must be valid");

        // WHEN each family's publisher is looked up
        // THEN it is listed, so a selection entry never names an unknown trainer
        for family in manifest.families() {
            assert!(
                lists
                    .official()
                    .iter()
                    .any(|p| p.publisher == family.publisher),
                "{} names publisher {}, absent from publishers.toml",
                family.id,
                family.publisher
            );
        }
    }

    #[test]
    fn an_owner_is_classified_without_regard_to_case() {
        // GIVEN the embedded lists
        // WHEN repositories under official, converter and unknown owners are classified
        let official = publisher_kind("qwen/Qwen3-8B-GGUF");
        let converter = publisher_kind("Bartowski/Qwen_Qwen3.5-35B-A3B-GGUF");
        let unknown = publisher_kind("someone/Qwen3-8B-GGUF");
        let empty = publisher_kind("");

        // THEN the first two carry their kind and the others none
        assert_eq!(official, Some(PublisherKind::Official));
        assert_eq!(converter, Some(PublisherKind::Converter));
        assert_eq!(unknown, None);
        assert_eq!(empty, None);
    }
}
