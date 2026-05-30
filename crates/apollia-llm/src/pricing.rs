//! Centralized LLM pricing table for inference cost calculation.
//!
//! Provides a static table of known models and a lookup algorithm by exact
//! match then by prefix (longest prefix wins). The prefix match handles the
//! date suffixes of Anthropic models (e.g. `claude-sonnet-4-5-20261015`
//! matches the `claude-sonnet-4-5` entry).
//!
//! Operator overrides are supported via `[llm.pricing_overrides]` in
//! `apollia.toml`; they take priority over the default table.

use std::collections::HashMap;

/// Pricing of an LLM model in dollars per million tokens.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PricingTier {
    /// Cost per million input (prompt) tokens.
    pub input_per_mtok: f64,
    /// Cost per million output (completion) tokens.
    pub output_per_mtok: f64,
}

/// Return the default pricing table for known models.
///
/// Keys are `model_id` prefixes: a model whose identifier starts with a key
/// matches that pricing tier. This convention handles date suffixes
/// (`claude-sonnet-4-5-20261015` matches `claude-sonnet-4-5`).
///
/// Prices reflect the public rates at implementation time (April 2026) and
/// may change.
pub fn default_pricing() -> HashMap<&'static str, PricingTier> {
    let mut m = HashMap::new();
    // Anthropic
    m.insert(
        "claude-haiku-4-5",
        PricingTier {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
        },
    );
    m.insert(
        "claude-sonnet-4-5",
        PricingTier {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        },
    );
    m.insert(
        "claude-sonnet-4-6",
        PricingTier {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        },
    );
    m.insert(
        "claude-opus-4-5",
        PricingTier {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        },
    );
    m.insert(
        "claude-opus-4-6",
        PricingTier {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        },
    );
    // OpenAI
    m.insert(
        "gpt-4o-mini",
        PricingTier {
            input_per_mtok: 0.15,
            output_per_mtok: 0.60,
        },
    );
    m.insert(
        "gpt-4o",
        PricingTier {
            input_per_mtok: 2.50,
            output_per_mtok: 10.0,
        },
    );
    m.insert(
        "gpt-4.1-mini",
        PricingTier {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
        },
    );
    m.insert(
        "gpt-4.1-nano",
        PricingTier {
            input_per_mtok: 0.10,
            output_per_mtok: 0.40,
        },
    );
    m.insert(
        "gpt-4.1",
        PricingTier {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
        },
    );
    m
}

/// Look up a model's pricing by exact match or by prefix.
///
/// Priority order:
/// 1. `overrides`, exact match
/// 2. `overrides`, prefix match (longest wins)
/// 3. `table`, exact match
/// 4. `table`, prefix match (longest wins)
///
/// Returns `None` if no match is found. The caller is responsible for emitting
/// a warning when `None` is returned.
pub fn lookup_pricing<'a>(
    model_id: &str,
    table: &'a HashMap<&str, PricingTier>,
    overrides: &'a HashMap<String, PricingTier>,
) -> Option<&'a PricingTier> {
    // 1. Overrides, exact match
    if let Some(tier) = overrides.get(model_id) {
        return Some(tier);
    }

    // 2. Overrides, prefix match (longest prefix first)
    {
        let mut keys: Vec<&str> = overrides.keys().map(String::as_str).collect();
        keys.sort_unstable_by_key(|k| std::cmp::Reverse(k.len()));
        for key in keys {
            if model_id.starts_with(key) {
                if let Some(tier) = overrides.get(key) {
                    return Some(tier);
                }
            }
        }
    }

    // 3. Default table, exact match
    if let Some(tier) = table.get(model_id) {
        return Some(tier);
    }

    // 4. Default table, prefix match (longest prefix first)
    {
        let mut keys: Vec<&str> = table.keys().copied().collect();
        keys.sort_unstable_by_key(|k| std::cmp::Reverse(k.len()));
        for key in keys {
            if model_id.starts_with(key) {
                if let Some(tier) = table.get(key) {
                    return Some(tier);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_overrides() -> HashMap<String, PricingTier> {
        HashMap::new()
    }

    // GIVEN the default pricing table
    // WHEN looking up the model "claude-sonnet-4-5"
    // THEN the returned pricing is input_per_mtok=3.0, output_per_mtok=15.0
    #[test]
    fn test_known_model_returns_correct_pricing() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let tier = lookup_pricing("claude-sonnet-4-5", &table, &overrides);

        assert!(
            tier.is_some(),
            "claude-sonnet-4-5 must be in the default table"
        );
        let tier = tier.unwrap();
        assert_eq!(tier.input_per_mtok, 3.0);
        assert_eq!(tier.output_per_mtok, 15.0);
    }

    // GIVEN the default pricing table
    // WHEN looking up the model "unknown-model-xyz"
    // THEN the result is None
    #[test]
    fn test_unknown_model_returns_none() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let result = lookup_pricing("unknown-model-xyz", &table, &overrides);

        assert!(result.is_none(), "unknown-model-xyz must return None");
    }

    // GIVEN the pricing table with the "claude-sonnet-4-5" entry
    // WHEN looking up the model "claude-sonnet-4-5-20261015"
    // THEN the returned pricing is that of "claude-sonnet-4-5"
    #[test]
    fn test_prefix_match_with_date_suffix() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let tier = lookup_pricing("claude-sonnet-4-5-20261015", &table, &overrides);

        assert!(tier.is_some(), "date suffix must match via prefix lookup");
        let tier = tier.unwrap();
        assert_eq!(tier.input_per_mtok, 3.0);
        assert_eq!(tier.output_per_mtok, 15.0);
    }

    // GIVEN overrides with "custom-model" = { input_per_mtok=1.0, output_per_mtok=5.0 }
    // WHEN looking up the model "custom-model"
    // THEN the returned pricing is input_per_mtok=1.0, output_per_mtok=5.0
    #[test]
    fn test_toml_override_takes_precedence() {
        let table = default_pricing();
        let mut overrides = empty_overrides();
        overrides.insert(
            "custom-model".to_owned(),
            PricingTier {
                input_per_mtok: 1.0,
                output_per_mtok: 5.0,
            },
        );

        let tier = lookup_pricing("custom-model", &table, &overrides).unwrap();

        assert_eq!(tier.input_per_mtok, 1.0);
        assert_eq!(tier.output_per_mtok, 5.0);
    }

    // GIVEN the codebase after implementation
    // WHEN inspecting anthropic.rs
    // THEN there is no longer any call to estimate_cost_usd
    #[test]
    fn test_no_model_contains_in_pricing_logic() {
        let source = include_str!("backends/anthropic.rs");
        assert!(
            !source.contains("estimate_cost_usd"),
            "estimate_cost_usd must no longer exist in anthropic.rs"
        );
        assert!(
            !source.contains(".contains(\"haiku\")"),
            "model.contains() must not be used for pricing in anthropic.rs"
        );
        assert!(
            !source.contains(".contains(\"sonnet\")"),
            "model.contains() must not be used for pricing in anthropic.rs"
        );
        assert!(
            !source.contains(".contains(\"opus\")"),
            "model.contains() must not be used for pricing in anthropic.rs"
        );
    }

    // GIVEN the table with "claude-sonnet-4-5" and a hypothetical "claude-sonnet"
    // WHEN looking up "claude-sonnet-4-5-20261015"
    // THEN "claude-sonnet-4-5" matches before "claude-sonnet" (longer prefix)
    #[test]
    fn test_longest_prefix_wins() {
        let mut table: HashMap<&str, PricingTier> = HashMap::new();
        table.insert(
            "claude-sonnet",
            PricingTier {
                input_per_mtok: 1.0,
                output_per_mtok: 2.0,
            },
        );
        table.insert(
            "claude-sonnet-4-5",
            PricingTier {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
            },
        );
        let overrides = empty_overrides();

        let tier = lookup_pricing("claude-sonnet-4-5-20261015", &table, &overrides).unwrap();

        assert_eq!(
            tier.input_per_mtok, 3.0,
            "longer prefix claude-sonnet-4-5 must win over claude-sonnet"
        );
    }

    // GIVEN the table with a "claude-sonnet-4-5" entry and a "claude-sonnet-4-5" override
    // WHEN looking up the exact identifier "claude-sonnet-4-5"
    // THEN the exact match in the overrides takes priority
    #[test]
    fn test_exact_match_preferred_over_prefix() {
        let table = default_pricing();
        let mut overrides = empty_overrides();
        overrides.insert(
            "claude-sonnet-4-5".to_owned(),
            PricingTier {
                input_per_mtok: 2.5,
                output_per_mtok: 12.0,
            },
        );

        let tier = lookup_pricing("claude-sonnet-4-5", &table, &overrides).unwrap();

        assert_eq!(
            tier.input_per_mtok, 2.5,
            "override exact match must take precedence over table"
        );
    }

    // GIVEN the default pricing table
    // WHEN counting the entries
    // THEN there are exactly 10
    #[test]
    fn test_all_default_models_present() {
        let table = default_pricing();

        assert_eq!(
            table.len(),
            10,
            "default pricing table must contain 10 models"
        );

        // Check the 5 Anthropic models are present
        assert!(table.contains_key("claude-haiku-4-5"));
        assert!(table.contains_key("claude-sonnet-4-5"));
        assert!(table.contains_key("claude-sonnet-4-6"));
        assert!(table.contains_key("claude-opus-4-5"));
        assert!(table.contains_key("claude-opus-4-6"));
        // Check the 5 OpenAI models are present
        assert!(table.contains_key("gpt-4o-mini"));
        assert!(table.contains_key("gpt-4o"));
        assert!(table.contains_key("gpt-4.1-mini"));
        assert!(table.contains_key("gpt-4.1-nano"));
        assert!(table.contains_key("gpt-4.1"));
    }

    // GIVEN empty overrides
    // WHEN looking up a known model
    // THEN the default table is used without error
    #[test]
    fn test_empty_overrides_uses_defaults() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let tier = lookup_pricing("claude-haiku-4-5", &table, &overrides);

        assert!(
            tier.is_some(),
            "empty overrides must fall back to default table"
        );
        assert_eq!(tier.unwrap().input_per_mtok, 0.80);
    }

    // GIVEN a local Ollama model not present in the table
    // WHEN looking up that model
    // THEN None is returned without panicking
    #[test]
    fn test_local_model_not_in_table() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let result = lookup_pricing("llama3.2:8b", &table, &overrides);

        assert!(
            result.is_none(),
            "local Ollama model must return None — cost is zero by definition"
        );
    }
}
