//! Quantisations, and what trading precision for size is worth.
//!
//! The planner used to assume every candidate was `Q4_K_M` and pick a different
//! quantisation only afterwards, from whatever the chosen repository happened
//! to publish. That makes one comparison impossible to express, and it is the
//! comparison that matters most on a constrained machine: a larger model at
//! lower precision against a smaller one at higher precision.
//!
//! The evidence is consistent that the larger model usually wins. Dropping from
//! `Q4_K_M` to `Q3_K_M` costs a few percent of capability; dropping a size
//! class costs considerably more. The trade stops paying below roughly three
//! bits, where degradation turns sharp, which is why nothing under `Q3_K_S`
//! appears in [`OFFERED`].
//!
//! # On the numbers
//!
//! `bits_per_weight` is arithmetic: the block formats are fixed, so the figures
//! are what the format produces, and they are checked against published file
//! sizes by `bits_per_weight_predicts_real_file_sizes`.
//!
//! `quality` is a judgement, informed by the perplexity tables the llama.cpp
//! project publishes for its own quantisations. It is a retention factor
//! against the unquantised model, so `Q4_K_M` at 0.985 means "keeps about
//! 98.5% of what the weights could do". Like the scores in `families.toml`,
//! these want verifying against a measured table before a release; unlike a
//! size, no arithmetic settles them.

use serde::{Deserialize, Serialize};

/// One quantisation format.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quant {
    /// Name as publishers write it in a file name, upper case.
    pub name: &'static str,
    /// Average bits stored per weight, including block scales.
    pub bits_per_weight: f64,
    /// Share of the unquantised model's capability this retains.
    pub quality: f64,
}

impl Quant {
    /// Bytes this quantisation needs for a model of `params_b` billion weights.
    ///
    /// An estimate for the planner, which has to choose what to ask the Hub
    /// about before it knows any real file size.
    #[must_use]
    pub fn estimated_bytes(&self, params_b: f64) -> u64 {
        (params_b * 1e9 * self.bits_per_weight / 8.0) as u64
    }
}

/// The quantisations the recommender will offer, most faithful first.
///
/// Anything below `Q3_K_S` is absent deliberately: at two-and-a-bit bits the
/// loss stops being a few percent and starts being the difference between a
/// model that follows instructions and one that does not, at which point the
/// next size class down is the better answer.
pub static OFFERED: &[Quant] = &[
    Quant {
        name: "Q8_0",
        bits_per_weight: 8.50,
        quality: 0.999,
    },
    Quant {
        name: "Q6_K",
        bits_per_weight: 6.56,
        quality: 0.998,
    },
    Quant {
        name: "Q5_K_M",
        bits_per_weight: 5.69,
        quality: 0.995,
    },
    Quant {
        name: "Q5_K_S",
        bits_per_weight: 5.52,
        quality: 0.993,
    },
    Quant {
        name: "Q5_0",
        bits_per_weight: 5.54,
        quality: 0.990,
    },
    Quant {
        name: "Q4_K_M",
        bits_per_weight: 4.85,
        quality: 0.985,
    },
    Quant {
        name: "Q4_K_S",
        bits_per_weight: 4.58,
        quality: 0.978,
    },
    Quant {
        name: "Q4_0",
        bits_per_weight: 4.55,
        quality: 0.965,
    },
    Quant {
        name: "IQ4_NL",
        bits_per_weight: 4.50,
        quality: 0.977,
    },
    Quant {
        name: "IQ4_XS",
        bits_per_weight: 4.25,
        quality: 0.975,
    },
    Quant {
        name: "Q3_K_L",
        bits_per_weight: 4.27,
        quality: 0.950,
    },
    Quant {
        name: "Q3_K_M",
        bits_per_weight: 3.91,
        quality: 0.930,
    },
    Quant {
        name: "IQ3_M",
        bits_per_weight: 3.66,
        quality: 0.920,
    },
    Quant {
        name: "Q3_K_S",
        bits_per_weight: 3.50,
        quality: 0.900,
    },
];

/// The most faithful quantisation worth planning for.
///
/// Above this the curve is nearly flat while the cost is not: `Q8_0` retains
/// 1.4% more than `Q4_K_M` for three-quarters again the memory, and reads that
/// much more per token, so it is slower as well as larger. Memory spent there
/// buys almost nothing, where the same memory spent on the next size class up
/// buys a great deal, and the planner is already comparing those side by side.
///
/// So the search starts here and only ever goes down. A repository that
/// publishes nothing this faithful still resolves, through the fallback in the
/// resolver, which is what keeps a publisher shipping only `Q5_K_M` usable.
pub const PLANNING_CEILING: &str = "Q4_K_M";

/// The offered quantisations from [`PLANNING_CEILING`] downwards.
///
/// What the planner searches: most faithful first, so the first that fits the
/// machine is the one to plan.
pub fn planning_candidates() -> impl Iterator<Item = &'static Quant> {
    let start = OFFERED
        .iter()
        .position(|q| q.name == PLANNING_CEILING)
        .unwrap_or(0);
    OFFERED[start..].iter()
}

/// How close one quantisation is to another, for picking a substitute.
///
/// Used when a repository does not publish the planned format. Distance rather
/// than "the best available", because the best available is often `Q8_0`, and
/// taking it would undo the trade the plan just made.
#[must_use]
pub fn distance(a: &Quant, b: &Quant) -> f64 {
    (a.quality - b.quality).abs()
}

/// The quantisation a publisher's file name declares.
///
/// Matched on a delimited substring so `Q4_K_M` in `Qwen3-4B-Q4_K_M.gguf` is
/// found while `Q4_K` does not also match inside `Q4_K_M`. The longest name
/// wins for the same reason.
#[must_use]
pub fn from_filename(filename: &str) -> Option<&'static Quant> {
    let upper = filename.to_ascii_uppercase();
    OFFERED
        .iter()
        .filter(|q| {
            upper.contains(&format!("-{}.", q.name)) || upper.contains(&format!("-{}-", q.name))
        })
        .max_by_key(|q| q.name.len())
}

/// Look one up by name.
#[must_use]
pub fn by_name(name: &str) -> Option<&'static Quant> {
    OFFERED.iter().find(|q| q.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_per_weight_predicts_real_file_sizes() {
        // GIVEN the published Q4_K_M sizes of four Qwen3 models
        let published: [(f64, f64); 4] = [(4.0, 2.5), (8.0, 4.7), (14.0, 8.4), (30.0, 18.6)];
        let q4 = by_name("Q4_K_M").expect("Q4_K_M is offered");

        // WHEN each is predicted from its parameter count
        // THEN every prediction lands within a fifth of the real file, which is
        // close enough to plan with before any file has been looked at
        for (params_b, real_gb) in published {
            let predicted_gb = q4.estimated_bytes(params_b) as f64 / 1024.0 / 1024.0 / 1024.0;
            let error = (predicted_gb - real_gb).abs() / real_gb;
            assert!(
                error < 0.2,
                "{params_b}B predicted {predicted_gb:.1} GB against {real_gb} GB"
            );
        }
    }

    #[test]
    fn a_bigger_model_at_lower_precision_outscores_a_smaller_one() {
        // GIVEN a 14B and an 8B of the same family, the larger quantised harder
        let bigger_raw = 73.0;
        let smaller_raw = 68.0;
        let q3 = by_name("Q3_K_M").expect("Q3_K_M");
        let q4 = by_name("Q4_K_M").expect("Q4_K_M");

        // WHEN both are scored through their quantisation
        let bigger = bigger_raw * q3.quality;
        let smaller = smaller_raw * q4.quality;

        // THEN the larger model still wins, which is the comparison a planner
        // assuming one quantisation for everything could not make
        assert!(
            bigger > smaller,
            "14B at Q3_K_M scored {bigger}, 8B at Q4_K_M scored {smaller}"
        );
    }

    #[test]
    fn the_k_quant_ladder_never_scores_a_narrower_format_higher() {
        // GIVEN the K-quant ladder, one family at ascending widths
        let ladder = [
            "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M", "Q6_K", "Q8_0",
        ];

        // WHEN each step is compared with the one below it
        // THEN both the width and the retained quality ascend, so within one
        // family the trade only ever runs in one direction
        for pair in ladder.windows(2) {
            let lower = by_name(pair[0]).expect("in the table");
            let upper = by_name(pair[1]).expect("in the table");
            assert!(
                upper.bits_per_weight > lower.bits_per_weight,
                "{} is not wider than {}",
                upper.name,
                lower.name
            );
            assert!(
                upper.quality >= lower.quality,
                "{} is wider than {} but retains less",
                upper.name,
                lower.name
            );
        }
    }

    #[test]
    fn an_i_quant_retains_more_than_a_legacy_quant_of_the_same_width() {
        // GIVEN IQ4_NL and Q4_0, which store about the same number of bits
        let iq = by_name("IQ4_NL").expect("IQ4_NL");
        let legacy = by_name("Q4_0").expect("Q4_0");

        // WHEN their widths and qualities are compared
        // THEN the newer format retains more at slightly fewer bits, which is
        // why quality is its own field rather than a function of the width
        assert!(iq.bits_per_weight < legacy.bits_per_weight);
        assert!(iq.quality > legacy.quality);
    }

    #[test]
    fn a_publisher_file_name_resolves_to_its_quantisation() {
        // GIVEN file names as the Hub publishes them
        // WHEN each is parsed
        // THEN the longest matching name wins and an unlabelled file is refused
        assert_eq!(
            from_filename("Qwen3-4B-Q4_K_M.gguf").map(|q| q.name),
            Some("Q4_K_M")
        );
        assert_eq!(
            from_filename("gemma-3-4b-it-IQ4_XS.gguf").map(|q| q.name),
            Some("IQ4_XS")
        );
        assert_eq!(
            from_filename("bartowski_Model-Q3_K_L-00001-of-00002.gguf").map(|q| q.name),
            Some("Q3_K_L")
        );
        assert_eq!(from_filename("Qwen3-4B-f16.gguf"), None);
        assert_eq!(from_filename("model.gguf"), None);
    }

    #[test]
    fn nothing_below_three_bits_is_offered() {
        // GIVEN the offered table
        // WHEN the smallest entry is measured
        // THEN it is still above three bits, because below that the loss stops
        // being a trade and the next size class down is the better answer
        let smallest = OFFERED
            .iter()
            .map(|q| q.bits_per_weight)
            .fold(f64::INFINITY, f64::min);
        assert!(smallest >= 3.0, "smallest offered was {smallest} bits");
    }

    #[test]
    fn every_quality_factor_is_a_retention_share() {
        // GIVEN the offered table
        // WHEN each quality factor is checked
        // THEN all sit between a half and one, so none can invert a score or
        // make a quantised model look better than its own weights
        for quant in OFFERED {
            assert!(
                (0.5..=1.0).contains(&quant.quality),
                "{} has an implausible quality factor {}",
                quant.name,
                quant.quality
            );
        }
    }

    #[test]
    fn the_planning_search_starts_at_the_sweet_spot_and_goes_down() {
        // GIVEN the planner's candidate list
        let candidates: Vec<&str> = planning_candidates().map(|q| q.name).collect();

        // WHEN it is compared with the whole table
        // THEN it opens at the ceiling and never offers anything wider, so the
        // planner cannot spend three-quarters again the memory for a percent
        assert_eq!(candidates.first().copied(), Some(PLANNING_CEILING));
        assert!(!candidates.contains(&"Q8_0"));
        assert!(!candidates.contains(&"Q6_K"));
        assert!(candidates.contains(&"Q3_K_M"));
    }

    #[test]
    fn a_substitute_is_chosen_by_closeness_rather_than_by_faithfulness() {
        // GIVEN the planned format and two substitutes, one just above it and
        // one far above
        let planned = by_name("Q4_K_M").expect("Q4_K_M");
        let near = by_name("Q5_K_S").expect("Q5_K_S");
        let far = by_name("Q8_0").expect("Q8_0");

        // WHEN their distances from the plan are compared
        // THEN the near one wins, so a repository without the planned format
        // does not silently upgrade the operator into a much larger download
        assert!(distance(near, planned) < distance(far, planned));
    }
}
