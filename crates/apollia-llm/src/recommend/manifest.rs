//! The curated generation table, parsed and made queryable.
//!
//! `families.toml` is embedded in the binary and its documentation lives in the
//! file itself. This module turns it into types, checks the two things a hand
//! edited table gets wrong, and answers the questions the ranker asks of it.
//!
//! The checks are: every `supersedes` id resolves, and the supersession graph
//! is acyclic. A dangling id silently drops a generation out of the ordering; a
//! cycle would make "newer than" meaningless and hang a naive walk. Both are
//! caught once, at load, rather than producing a subtly wrong ranking later.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The table as shipped in the binary.
static EMBEDDED_TOML: &str = include_str!("families.toml");

/// Failure to load or validate the generation table.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The TOML did not parse or did not match the schema.
    #[error("model family table did not parse: {0}")]
    Parse(#[from] toml::de::Error),

    /// A `supersedes` entry names a family the table does not define.
    #[error("family '{family}' supersedes '{missing}', which the table does not define")]
    DanglingSupersedes {
        /// The family carrying the bad reference.
        family: String,
        /// The id it named.
        missing: String,
    },

    /// Two families share an id.
    #[error("family id '{0}' is defined twice")]
    DuplicateId(String),

    /// The supersession graph loops.
    #[error("supersession cycle through family '{0}'")]
    Cycle(String),

    /// The table declares a schema version this build does not read.
    #[error("model family table declares schema version {found}, this build reads {expected}")]
    SchemaVersion {
        /// The version in the file.
        found: u32,
        /// The version this build understands.
        expected: u32,
    },
}

/// Schema version this build reads.
const SCHEMA_VERSION: u32 = 1;

/// How a family's published chat template handles tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCalling {
    /// No tool-call support in the template at all.
    None,
    /// Calls have to be asked for in the prompt and parsed out of prose.
    Prompted,
    /// The template emits tool calls that the `--jinja` path parses.
    Native,
}

impl ToolCalling {
    /// Weight applied to a family's agentic score, since an agent runtime is
    /// what this recommendation serves.
    fn weight(self) -> f64 {
        match self {
            Self::Native => 1.0,
            Self::Prompted => 0.75,
            Self::None => 0.45,
        }
    }
}

/// One size within a generation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Variant {
    /// Parameter count in billions.
    pub params_b: f64,
    /// Upstream repository name, expanded into the family's repo patterns.
    pub hf_name: String,
    /// Tool use and instruction following, 0 to 100. The ranking key.
    pub agentic: Option<u32>,
    /// General capability, 0 to 100. Breaks agentic ties.
    pub reasoning: Option<u32>,
    /// Where the two scores were read from.
    pub source: Option<String>,
}

/// One model generation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Family {
    /// Stable key, referenced by `supersedes`.
    pub id: String,
    /// What the operator reads.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date of the generation's first public release.
    pub released: String,
    /// `general.architecture` values its GGUF files carry.
    pub architectures: Vec<String>,
    /// Ids of the generations this one replaces.
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// How its template handles tool calls.
    pub tool_calling: ToolCalling,
    /// SPDX identifier, or the publisher's own licence name.
    pub license: String,
    /// The search string that finds this family's GGUF repositories.
    pub hf_query: String,
    /// GGUF publishers in preference order, with `{model}` as the placeholder.
    #[serde(default)]
    pub repo_patterns: Vec<String>,
    /// Free prose shown to the operator or read by whoever edits the table.
    #[serde(default)]
    pub notes: Option<String>,
    /// The sizes this generation ships.
    #[serde(default, rename = "variant")]
    pub variants: Vec<Variant>,
}

impl Family {
    /// Expand the repo patterns for one variant, in preference order.
    #[must_use]
    pub fn candidate_repos(&self, variant: &Variant) -> Vec<String> {
        self.repo_patterns
            .iter()
            .map(|p| p.replace("{model}", &variant.hf_name))
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    schema_version: u32,
    #[serde(default, rename = "family")]
    families: Vec<Family>,
}

/// The generation table, validated and indexed.
#[derive(Debug, Clone)]
pub struct FamilyManifest {
    families: Vec<Family>,
    by_id: BTreeMap<String, usize>,
    /// For each family, how many generations it stands above at the deepest.
    /// A family that supersedes nothing is 0.
    generation_depth: BTreeMap<String, u32>,
}

impl FamilyManifest {
    /// Load the table embedded in the binary.
    ///
    /// # Errors
    /// See [`ManifestError`]. A failure here is a defect in a file that ships
    /// with the binary, which is why the loader is also a test.
    pub fn embedded() -> Result<Self, ManifestError> {
        Self::from_toml(EMBEDDED_TOML)
    }

    /// Load a table from TOML, for tests and for an operator override.
    ///
    /// # Errors
    /// See [`ManifestError`].
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let raw: RawManifest = toml::from_str(text)?;
        if raw.schema_version != SCHEMA_VERSION {
            return Err(ManifestError::SchemaVersion {
                found: raw.schema_version,
                expected: SCHEMA_VERSION,
            });
        }

        let mut by_id = BTreeMap::new();
        for (idx, family) in raw.families.iter().enumerate() {
            if by_id.insert(family.id.clone(), idx).is_some() {
                return Err(ManifestError::DuplicateId(family.id.clone()));
            }
        }

        for family in &raw.families {
            for target in &family.supersedes {
                if !by_id.contains_key(target) {
                    return Err(ManifestError::DanglingSupersedes {
                        family: family.id.clone(),
                        missing: target.clone(),
                    });
                }
            }
        }

        let generation_depth = compute_depths(&raw.families, &by_id)?;

        Ok(Self {
            families: raw.families,
            by_id,
            generation_depth,
        })
    }

    /// Every family in the table, in file order.
    #[must_use]
    pub fn families(&self) -> &[Family] {
        &self.families
    }

    /// Look one up by id.
    #[must_use]
    pub fn family(&self, id: &str) -> Option<&Family> {
        self.by_id.get(id).and_then(|idx| self.families.get(*idx))
    }

    /// How many generations this family stands above, transitively.
    ///
    /// This is what makes "Qwen3 beats Qwen2.5" a property of the data rather
    /// than of the order entries happen to sit in the file.
    #[must_use]
    pub fn generation_depth(&self, id: &str) -> u32 {
        self.generation_depth.get(id).copied().unwrap_or(0)
    }

    /// Whether some family in the table supersedes `id`.
    #[must_use]
    pub fn is_superseded(&self, id: &str) -> bool {
        self.families
            .iter()
            .any(|f| f.supersedes.iter().any(|s| s == id))
    }

    /// The agentic score for a variant, weighted by how its family calls tools.
    ///
    /// Returns `None` when the table records no score, which keeps an
    /// unscored entry out of the ranking rather than giving it a default that
    /// would place it arbitrarily.
    #[must_use]
    pub fn weighted_agentic(&self, family: &Family, variant: &Variant) -> Option<f64> {
        variant
            .agentic
            .map(|score| f64::from(score) * family.tool_calling.weight())
    }
}

/// Longest supersession chain below each family.
fn compute_depths(
    families: &[Family],
    by_id: &BTreeMap<String, usize>,
) -> Result<BTreeMap<String, u32>, ManifestError> {
    let mut depths = BTreeMap::new();
    for family in families {
        let mut visiting = BTreeSet::new();
        let depth = depth_of(&family.id, families, by_id, &mut visiting, &mut depths)?;
        depths.insert(family.id.clone(), depth);
    }
    Ok(depths)
}

fn depth_of(
    id: &str,
    families: &[Family],
    by_id: &BTreeMap<String, usize>,
    visiting: &mut BTreeSet<String>,
    memo: &mut BTreeMap<String, u32>,
) -> Result<u32, ManifestError> {
    if let Some(known) = memo.get(id) {
        return Ok(*known);
    }
    if !visiting.insert(id.to_owned()) {
        return Err(ManifestError::Cycle(id.to_owned()));
    }

    let family = by_id
        .get(id)
        .and_then(|idx| families.get(*idx))
        .ok_or_else(|| ManifestError::DanglingSupersedes {
            family: id.to_owned(),
            missing: id.to_owned(),
        })?;

    let mut depth = 0;
    for target in &family.supersedes {
        let below = depth_of(target, families, by_id, visiting, memo)?;
        depth = depth.max(below + 1);
    }

    visiting.remove(id);
    memo.insert(id.to_owned(), depth);
    Ok(depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_loads_and_validates() {
        // GIVEN the table shipped in the binary
        // WHEN it is loaded
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // THEN it holds families, each with at least one architecture and size
        assert!(!manifest.families().is_empty());
        for family in manifest.families() {
            assert!(
                !family.architectures.is_empty(),
                "{} declares no architecture",
                family.id
            );
            assert!(
                !family.variants.is_empty(),
                "{} declares no variant",
                family.id
            );
        }
    }

    #[test]
    fn every_scored_variant_cites_a_source() {
        // GIVEN the shipped table
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN each scored variant is inspected
        // THEN it names where the score came from, because a number no one can
        // trace ranks models with the authority of a measurement
        for family in manifest.families() {
            for variant in &family.variants {
                if variant.agentic.is_some() || variant.reasoning.is_some() {
                    assert!(
                        variant.source.is_some(),
                        "{} / {} is scored but cites no source",
                        family.id,
                        variant.hf_name
                    );
                }
            }
        }
    }

    #[test]
    fn a_newer_generation_outranks_the_one_it_supersedes() {
        // GIVEN the shipped table, whose Qwen chain runs 2.5, then 3, then 3.5
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN the depths along the chain are compared
        let newest = manifest.generation_depth("qwen3.5");
        let middle = manifest.generation_depth("qwen3");
        let oldest = manifest.generation_depth("qwen2.5");

        // THEN each stands above the one it replaces, and only the head of the
        // chain is unsuperseded. This is the ordering the Hub's own signals
        // would have inverted, since the oldest has the most downloads.
        assert!(
            newest > middle,
            "qwen3.5 depth {newest} did not exceed {middle}"
        );
        assert!(
            middle > oldest,
            "qwen3 depth {middle} did not exceed {oldest}"
        );
        assert!(manifest.is_superseded("qwen2.5"));
        assert!(manifest.is_superseded("qwen3"));
        assert!(!manifest.is_superseded("qwen3.5"));
    }

    #[test]
    fn one_architecture_can_belong_to_several_generations() {
        // GIVEN the shipped table, where Llama 3.1 and Mistral Small 3 both
        // report the llama architecture
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN the families declaring it are gathered
        let matches: Vec<&Family> = manifest
            .families()
            .iter()
            .filter(|f| f.architectures.iter().any(|a| a == "llama"))
            .collect();

        // THEN there are several, which is why a file is matched to its
        // generation by repository name and never by architecture alone
        assert!(
            matches.len() > 1,
            "expected several families on the llama architecture"
        );
    }

    #[test]
    fn a_dangling_supersedes_reference_is_refused_at_load() {
        // GIVEN a table naming a predecessor it does not define
        let text = r#"
schema_version = 1
[[family]]
id = "a"
label = "A"
publisher = "P"
released = "2025-01-01"
architectures = ["a"]
supersedes = ["ghost"]
tool_calling = "native"
license = "mit"
hf_query = "a"
[[family.variant]]
params_b = 7
hf_name = "A-7B"
"#;

        // WHEN it is loaded
        let err = FamilyManifest::from_toml(text).expect_err("a dangling id must be refused");

        // THEN the bad reference is named, rather than the family silently
        // dropping out of the ordering
        assert!(matches!(err, ManifestError::DanglingSupersedes { .. }));
    }

    #[test]
    fn a_supersession_cycle_is_refused_at_load() {
        // GIVEN two families that supersede each other
        let text = r#"
schema_version = 1
[[family]]
id = "a"
label = "A"
publisher = "P"
released = "2025-01-01"
architectures = ["a"]
supersedes = ["b"]
tool_calling = "native"
license = "mit"
hf_query = "a"
[[family.variant]]
params_b = 7
hf_name = "A-7B"
[[family]]
id = "b"
label = "B"
publisher = "P"
released = "2024-01-01"
architectures = ["b"]
supersedes = ["a"]
tool_calling = "native"
license = "mit"
hf_query = "b"
[[family.variant]]
params_b = 7
hf_name = "B-7B"
"#;

        // WHEN it is loaded
        let err = FamilyManifest::from_toml(text).expect_err("a cycle must be refused");

        // THEN the cycle is reported instead of the walk looping
        assert!(matches!(err, ManifestError::Cycle(_)));
    }

    #[test]
    fn a_prompted_family_is_weighted_below_a_native_one_at_equal_score() {
        // GIVEN a native family and a prompted one carrying the same raw score
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");
        let native = manifest.family("qwen3").expect("qwen3 is in the table");
        let prompted = manifest.family("gemma3").expect("gemma3 is in the table");
        let variant = Variant {
            params_b: 4.0,
            hf_name: "x".to_owned(),
            agentic: Some(60),
            reasoning: Some(60),
            source: None,
        };

        // WHEN both are weighted
        let native_score = manifest
            .weighted_agentic(native, &variant)
            .expect("a scored variant weighs");
        let prompted_score = manifest
            .weighted_agentic(prompted, &variant)
            .expect("a scored variant weighs");

        // THEN the native one wins, because tool calling is what the runtime does
        assert!(native_score > prompted_score);
    }

    #[test]
    fn repo_patterns_expand_to_the_publishers_in_preference_order() {
        // GIVEN the Qwen3 family and its 4B variant
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");
        let family = manifest.family("qwen3").expect("qwen3 is in the table");
        let variant = family
            .variants
            .iter()
            .find(|v| (v.params_b - 4.0).abs() < f64::EPSILON)
            .expect("the 4B variant is in the table");

        // WHEN the patterns are expanded
        let repos = family.candidate_repos(variant);

        // THEN the upstream publisher comes first and every entry names the model
        assert_eq!(
            repos.first().map(String::as_str),
            Some("Qwen/Qwen3-4B-GGUF")
        );
        assert!(repos.iter().all(|r| r.contains("Qwen3-4B")));
    }
}
