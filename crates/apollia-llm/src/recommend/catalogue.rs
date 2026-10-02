//! The editorial selection: the curated table rendered as a catalogue.
//!
//! The ranker reads `families.toml` to order what the Hub returns. The same
//! table is also what an operator should see first, before any search: a short
//! list of generations Apollia has tested, with the repositories that carry
//! each size in GGUF and, where a conversion exists, in MLX.
//!
//! Everything here is computed from the embedded table, with no request, so the
//! selection is available on a machine with no network at all.

use serde::Serialize;

use super::manifest::{FamilyManifest, ToolCalling};

/// One size of a generation, with the repositories that carry it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectionVariant {
    /// Upstream model name.
    pub name: String,
    /// Parameter count in billions.
    pub params_b: f64,
    /// GGUF repositories, in the table's preference order.
    pub gguf_repos: Vec<String>,
    /// MLX repositories, in the table's preference order. Empty when no
    /// conversion exists.
    pub mlx_repos: Vec<String>,
}

/// One generation of the editorial selection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectionFamily {
    /// Stable key from the table.
    pub id: String,
    /// What the operator reads.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date of its first public release.
    pub released: String,
    /// Licence identifier.
    pub license: String,
    /// How its template handles tool calls.
    pub tool_calling: ToolCalling,
    /// Whether a newer generation in the table replaces it.
    pub superseded: bool,
    /// Its sizes, smallest first as the table lists them.
    pub variants: Vec<SelectionVariant>,
}

impl FamilyManifest {
    /// Render the whole table as the editorial selection, in file order.
    ///
    /// Superseded generations are kept and flagged rather than dropped: a
    /// machine already running one is better told what replaces it than shown
    /// a catalogue where its model has vanished.
    #[must_use]
    pub fn editorial_selection(&self) -> Vec<SelectionFamily> {
        self.families()
            .iter()
            .map(|family| SelectionFamily {
                id: family.id.clone(),
                label: family.label.clone(),
                publisher: family.publisher.clone(),
                released: family.released.clone(),
                license: family.license.clone(),
                tool_calling: family.tool_calling,
                superseded: self.is_superseded(&family.id),
                variants: family
                    .variants
                    .iter()
                    .map(|variant| SelectionVariant {
                        name: variant.hf_name.clone(),
                        params_b: variant.params_b,
                        gguf_repos: family.candidate_repos(variant),
                        mlx_repos: variant.mlx_repos.clone(),
                    })
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_of_the_table_is_rendered_with_a_repository() {
        // GIVEN the shipped table
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN the editorial selection is rendered
        let selection = manifest.editorial_selection();

        // THEN every family appears, in file order, and each carries at least
        // one repository on each of its sizes
        let table_ids: Vec<&str> = manifest.families().iter().map(|f| f.id.as_str()).collect();
        let rendered_ids: Vec<&str> = selection.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(rendered_ids, table_ids);
        for family in &selection {
            assert!(!family.label.is_empty(), "{} has no label", family.id);
            assert!(
                !family.publisher.is_empty(),
                "{} has no publisher",
                family.id
            );
            assert!(!family.variants.is_empty(), "{} has no size", family.id);
            for variant in &family.variants {
                assert!(
                    !variant.gguf_repos.is_empty() || !variant.mlx_repos.is_empty(),
                    "{} / {} carries no repository",
                    family.id,
                    variant.name
                );
            }
        }
    }

    #[test]
    fn mlx_repositories_are_carried_where_the_table_names_them() {
        // GIVEN the shipped table, whose Qwen3 4B has an official MLX release
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN the selection is rendered
        let selection = manifest.editorial_selection();
        let qwen3 = selection
            .iter()
            .find(|f| f.id == "qwen3")
            .expect("qwen3 is in the selection");
        let four_b = qwen3
            .variants
            .iter()
            .find(|v| v.name == "Qwen3-4B")
            .expect("the 4B size is in the selection");

        // THEN its MLX repositories come first from the publisher, and every
        // family of the table offers MLX on at least one size
        assert_eq!(
            four_b.mlx_repos.first().map(String::as_str),
            Some("Qwen/Qwen3-4B-MLX-4bit")
        );
        for family in &selection {
            assert!(
                family.variants.iter().any(|v| !v.mlx_repos.is_empty()),
                "{} offers no MLX conversion",
                family.id
            );
        }
    }

    #[test]
    fn a_superseded_generation_is_flagged_not_dropped() {
        // GIVEN the shipped table, where Qwen3 replaces Qwen2.5
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");

        // WHEN the selection is rendered
        let selection = manifest.editorial_selection();

        // THEN Qwen2.5 is still there, marked as replaced
        let old = selection
            .iter()
            .find(|f| f.id == "qwen2.5")
            .expect("qwen2.5 is kept");
        assert!(old.superseded);
        let head = selection
            .iter()
            .find(|f| f.id == "qwen3.5")
            .expect("qwen3.5 is in the selection");
        assert!(!head.superseded);
    }
}
