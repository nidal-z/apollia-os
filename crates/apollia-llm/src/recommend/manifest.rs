//! The model families a recommendation orders, and the entries pinned by digest.
//!
//! A family is a group of registry cards, a variant one card of it:
//! [`super::registry_source`] builds both from the model registry and gives
//! each variant the score it is ordered by before the machine is taken into
//! account. This module holds the types, checks that no two families share an
//! identifier, and answers the questions the resolver and the ranker ask.
//!
//! It also holds the errors of the pinned entries (`pinned_models.toml`: the
//! embeddings, dictation and character-recognition files), which load beside
//! it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The pinned entries as shipped in the binary.
pub(super) static EMBEDDED_TOML: &str = include_str!("pinned_models.toml");

/// Failure to load a set of families or the pinned entries.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The TOML itself is malformed or does not match the schema.
    #[error("the pinned entries did not parse: {0}")]
    Parse(#[from] toml::de::Error),

    /// Two entries share an id.
    #[error("id '{0}' is defined twice")]
    DuplicateId(String),

    /// An embeddings entry cannot be used: its digest is not a SHA-256, or its
    /// file has no size.
    #[error("embeddings entry '{id}' is invalid: {reason}")]
    InvalidEmbedding {
        /// The entry.
        id: String,
        /// What is wrong with it.
        reason: String,
    },

    /// A dictation entry cannot be used: its digest is not a SHA-256, or its
    /// file has no size.
    #[error("dictation entry '{id}' is invalid: {reason}")]
    InvalidDictation {
        /// The entry.
        id: String,
        /// What is wrong with it.
        reason: String,
    },

    /// A character-recognition entry cannot be used: its language code is not
    /// a file stem, its digest is not a SHA-256, or its file has no size.
    #[error("ocr entry '{language}' is invalid: {reason}")]
    InvalidOcr {
        /// The entry.
        language: String,
        /// What is wrong with it.
        reason: String,
    },

    /// A file was written for a different schema than this build reads.
    #[error("the file declares schema version {found}, this build reads {expected}")]
    SchemaVersion {
        /// Version declared in the file.
        found: u32,
        /// Version this build reads.
        expected: u32,
    },
}

/// Whether the models of a family call tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCalling {
    /// No card of the family declares tool calling.
    None,
    /// Calls have to be asked for in the prompt and parsed out of prose.
    Prompted,
    /// The chat template emits tool calls that the `--jinja` path parses.
    Native,
}

/// One model of a family: one registry card.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Variant {
    /// Parameter count in billions.
    pub params_b: f64,
    /// Name of the publisher's repository.
    pub hf_name: String,
    /// The name of the model as its registry card gives it.
    pub label: String,
    /// What the model is ordered by before the machine is taken into account:
    /// see [`super::registry_source::rule_score`].
    pub score: f64,
    /// GGUF repositories of this model, in preference order.
    #[serde(default)]
    pub gguf_repos: Vec<String>,
    /// MLX conversions of this model, in preference order. Empty when none
    /// exists.
    #[serde(default)]
    pub mlx_repos: Vec<String>,
}

/// One group of registry cards.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Family {
    /// Stable key: what comes before the `/` of its cards' identifiers.
    pub id: String,
    /// Human-readable name.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date of its most recent model.
    pub released: String,
    /// Whether its models call tools.
    pub tool_calling: ToolCalling,
    /// SPDX identifier, or the identifier the publisher declares.
    pub license: String,
    /// Its models.
    pub variants: Vec<Variant>,
}

/// A set of families, queryable by identifier.
#[derive(Debug, Clone)]
pub struct FamilyManifest {
    families: Vec<Family>,
    by_id: BTreeMap<String, usize>,
}

impl FamilyManifest {
    /// Check a set of families and make it queryable.
    ///
    /// # Errors
    /// [`ManifestError::DuplicateId`] when two families share an identifier.
    pub fn from_families(families: Vec<Family>) -> Result<Self, ManifestError> {
        let mut by_id = BTreeMap::new();
        for (idx, family) in families.iter().enumerate() {
            if by_id.insert(family.id.clone(), idx).is_some() {
                return Err(ManifestError::DuplicateId(family.id.clone()));
            }
        }
        Ok(Self { families, by_id })
    }

    /// Every family, in the order given.
    #[must_use]
    pub fn families(&self) -> &[Family] {
        &self.families
    }

    /// Look a family up by id.
    #[must_use]
    pub fn family(&self, id: &str) -> Option<&Family> {
        self.by_id.get(id).and_then(|idx| self.families.get(*idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(id: &str) -> Family {
        Family {
            id: id.to_owned(),
            label: id.to_owned(),
            publisher: "Ada".to_owned(),
            released: "2026-01-01".to_owned(),
            tool_calling: ToolCalling::Native,
            license: "apache-2.0".to_owned(),
            variants: Vec::new(),
        }
    }

    #[test]
    fn a_family_is_found_by_its_identifier() {
        // GIVEN two families
        let manifest =
            FamilyManifest::from_families(vec![family("a"), family("b")]).expect("distinct ids");

        // WHEN one is looked up, and one that does not exist
        // THEN the first is found, in the order given, and the other is not
        assert_eq!(manifest.families().len(), 2);
        assert_eq!(manifest.family("b").map(|f| f.id.as_str()), Some("b"));
        assert!(manifest.family("c").is_none());
    }

    #[test]
    fn two_families_of_one_identifier_are_refused() {
        // GIVEN two families sharing an identifier
        let twice = vec![family("a"), family("a")];

        // WHEN the set is checked
        let err = FamilyManifest::from_families(twice).expect_err("a duplicate id is refused");

        // THEN the failure names the identifier
        assert!(matches!(err, ManifestError::DuplicateId(id) if id == "a"));
    }
}
