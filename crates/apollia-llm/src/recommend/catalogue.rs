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

use super::fit::{assess_fit, RuntimeShape};
use super::manifest::{FamilyManifest, ToolCalling};
use super::quant::planning_candidates;
use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::{CompatibilityBadge, HardwareProfile};

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

impl SelectionVariant {
    /// Whether this size has a variant recommended on this machine.
    ///
    /// The selection is read with no request, so no file has been sized. The
    /// smallest variant a repository is offered in is the four-bit level, and
    /// a variant is recommended as soon as one fits; so the size fits when its
    /// planning-ceiling quantisation, estimated from the parameter count, gets
    /// a verdict other than too large from the one estimator.
    #[must_use]
    pub fn fits(&self, profile: &HardwareProfile) -> bool {
        let Some(ceiling) = planning_candidates().next() else {
            return false;
        };
        let fit = assess_fit(
            ceiling.estimated_bytes(self.params_b),
            &GgufHeaderFacts::default(),
            profile,
            &RuntimeShape::default(),
        );
        matches!(
            fit.badge,
            CompatibilityBadge::Fits | CompatibilityBadge::MightFit
        )
    }
}

/// Keep, in each generation, the sizes that fit this machine, and drop the
/// generations left with none.
pub fn retain_fitting(selection: &mut Vec<SelectionFamily>, profile: &HardwareProfile) {
    for family in selection.iter_mut() {
        family.variants.retain(|v| v.fits(profile));
    }
    selection.retain(|family| !family.variants.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::AcceleratorProfile;

    #[test]
    fn the_selection_that_fits_depends_on_the_machine() {
        // GIVEN the shipped selection and two machines
        let manifest = FamilyManifest::embedded().expect("the shipped table must be valid");
        let small = HardwareProfile {
            total_ram_gb: 8.0,
            available_ram_gb: 4.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 4,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: 4.8,
        };
        let large = HardwareProfile {
            total_ram_gb: 192.0,
            available_ram_gb: 160.0,
            cpu_model: "Apple M2 Ultra".to_owned(),
            cpu_cores: 24,
            accelerator: AcceleratorProfile::AppleSilicon {
                chip: "M2 Ultra".to_owned(),
                generation: 2,
                vram_gb: 192.0,
            },
            memory_budget_gb: 144.0,
        };
        let sizes = |selection: &[SelectionFamily]| -> Vec<f64> {
            selection
                .iter()
                .flat_map(|f| f.variants.iter().map(|v| v.params_b))
                .collect()
        };

        for (profile, smallest_kept, largest_kept) in [(small, true, false), (large, true, true)] {
            // WHEN the selection is cut to what fits
            let mut selection = manifest.editorial_selection();
            let all = sizes(&selection);
            retain_fitting(&mut selection, &profile);
            let kept = sizes(&selection);

            // THEN the smallest size stays, the largest only on the large
            // machine, and no generation is left empty
            let smallest = all.iter().copied().fold(f64::INFINITY, f64::min);
            let largest = all.iter().copied().fold(0.0, f64::max);
            assert_eq!(kept.contains(&smallest), smallest_kept);
            assert_eq!(kept.contains(&largest), largest_kept);
            assert!(selection.iter().all(|f| !f.variants.is_empty()));
            assert!(kept.len() <= all.len());
        }
    }

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
