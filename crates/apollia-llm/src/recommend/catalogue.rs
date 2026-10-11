//! The editorial selection: a set of families rendered as a catalogue.
//!
//! What an operator should see first, before any search: the models of the
//! registry, grouped by family, with the repositories that carry each one in
//! GGUF and, where a conversion exists, in MLX.
//!
//! Everything here is computed from the families given, with no request, so
//! the selection is available on a machine with no network at all.

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
                variants: family
                    .variants
                    .iter()
                    .map(|variant| SelectionVariant {
                        name: variant.hf_name.clone(),
                        params_b: variant.params_b,
                        gguf_repos: variant.gguf_repos.clone(),
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

    /// Every language model of the shipped registry, as families.
    fn registry() -> FamilyManifest {
        let index = crate::model_cards::shipped_index().expect("the shipped index loads");
        crate::recommend::selection_from_cards(&index.cards).expect("the cards group")
    }

    #[test]
    fn the_selection_that_fits_depends_on_the_machine() {
        // GIVEN the shipped selection and two machines
        let manifest = registry();
        let small = HardwareProfile {
            total_ram_gb: 8.0,
            available_ram_gb: 4.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 4,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: 4.8,
            devices: Vec::new(),
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
            devices: Vec::new(),
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
    fn every_family_of_the_registry_is_rendered_with_a_repository() {
        // GIVEN the language models of the shipped registry
        let manifest = registry();

        // WHEN the editorial selection is rendered
        let selection = manifest.editorial_selection();

        // THEN every family appears, in the order of the families, and each
        // carries at least one repository on each of its models
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
    fn mlx_repositories_are_carried_where_a_card_names_them() {
        // GIVEN the shipped registry, whose Qwen3 4B card names MLX conversions
        let manifest = registry();

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
            .expect("the 4B model is in the selection");

        // THEN its MLX repositories are carried apart from its GGUF ones
        assert!(!four_b.mlx_repos.is_empty());
        assert!(four_b
            .mlx_repos
            .iter()
            .all(|r| r.to_ascii_lowercase().contains("mlx")));
        assert!(four_b
            .gguf_repos
            .iter()
            .all(|r| r.to_ascii_lowercase().contains("gguf")));
    }
}
