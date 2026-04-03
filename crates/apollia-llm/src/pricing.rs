//! Table de pricing LLM centralisée pour le calcul des coûts d'inférence.
//!
//! Fournit une table statique des modèles connus et un algorithme de lookup
//! par correspondance exacte puis par préfixe (le préfixe le plus long gagne).
//! La correspondance par préfixe gère les suffixes de date des modèles Anthropic
//! (ex : `claude-sonnet-4-5-20261015` correspond à l'entrée `claude-sonnet-4-5`).
//!
//! Les surcharges opérateur sont supportées via `[llm.pricing_overrides]` dans
//! `apollia.toml` — elles ont priorité sur la table par défaut.

use std::collections::HashMap;

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Pricing d'un modèle LLM en dollars par million de tokens.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PricingTier {
    /// Coût par million de tokens en entrée (prompt).
    pub input_per_mtok: f64,
    /// Coût par million de tokens en sortie (completion).
    pub output_per_mtok: f64,
}

// ─────────────────────────────────────────────
// Table par défaut
// ─────────────────────────────────────────────

/// Retourne la table de pricing par défaut pour les modèles connus.
///
/// Les clés sont des préfixes de `model_id` : un modèle dont l'identifiant
/// commence par cette clé correspond à ce tier de pricing. Cette convention
/// permet de gérer les suffixes de date (`claude-sonnet-4-5-20261015`
/// correspond à `claude-sonnet-4-5`).
///
/// Les prix reflètent les tarifs publics au moment de l'implémentation
/// (avril 2026) et peuvent évoluer.
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

// ─────────────────────────────────────────────
// Lookup
// ─────────────────────────────────────────────

/// Cherche le pricing d'un modèle par correspondance exacte ou par préfixe.
///
/// Ordre de priorité :
/// 1. `overrides` — correspondance exacte
/// 2. `overrides` — correspondance par préfixe (le plus long gagne)
/// 3. `table` — correspondance exacte
/// 4. `table` — correspondance par préfixe (le plus long gagne)
///
/// Retourne `None` si aucune correspondance n'est trouvée. L'appelant est
/// responsable d'émettre un avertissement en cas de `None`.
pub fn lookup_pricing<'a>(
    model_id: &str,
    table: &'a HashMap<&str, PricingTier>,
    overrides: &'a HashMap<String, PricingTier>,
) -> Option<&'a PricingTier> {
    // 1. Overrides — correspondance exacte
    if let Some(tier) = overrides.get(model_id) {
        return Some(tier);
    }

    // 2. Overrides — correspondance par préfixe (plus long préfixe en premier)
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

    // 3. Table par défaut — correspondance exacte
    if let Some(tier) = table.get(model_id) {
        return Some(tier);
    }

    // 4. Table par défaut — correspondance par préfixe (plus long préfixe en premier)
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

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_overrides() -> HashMap<String, PricingTier> {
        HashMap::new()
    }

    // GIVEN la table de pricing par défaut
    // WHEN on lookup le modèle "claude-sonnet-4-5"
    // THEN le pricing retourné est input_per_mtok=3.0, output_per_mtok=15.0
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

    // GIVEN la table de pricing par défaut
    // WHEN on lookup le modèle "unknown-model-xyz"
    // THEN le résultat est None
    #[test]
    fn test_unknown_model_returns_none() {
        let table = default_pricing();
        let overrides = empty_overrides();

        let result = lookup_pricing("unknown-model-xyz", &table, &overrides);

        assert!(result.is_none(), "unknown-model-xyz must return None");
    }

    // GIVEN la table de pricing avec l'entrée "claude-sonnet-4-5"
    // WHEN on lookup le modèle "claude-sonnet-4-5-20261015"
    // THEN le pricing retourné est celui de "claude-sonnet-4-5"
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

    // GIVEN des overrides avec "custom-model" = { input_per_mtok=1.0, output_per_mtok=5.0 }
    // WHEN on lookup le modèle "custom-model"
    // THEN le pricing retourné est input_per_mtok=1.0, output_per_mtok=5.0
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

    // GIVEN le codebase après implémentation
    // WHEN on inspecte anthropic.rs
    // THEN il n'y a plus aucun appel à estimate_cost_usd
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

    // GIVEN la table avec "claude-sonnet-4-5" et un hypothétique "claude-sonnet"
    // WHEN on lookup "claude-sonnet-4-5-20261015"
    // THEN "claude-sonnet-4-5" matche avant "claude-sonnet" (plus long préfixe)
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

    // GIVEN la table avec une entrée "claude-sonnet-4-5" et un override "claude-sonnet-4-5"
    // WHEN on lookup l'identifiant exact "claude-sonnet-4-5"
    // THEN le match exact dans les overrides est prioritaire
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

    // GIVEN la table de pricing par défaut
    // WHEN on compte les entrées
    // THEN il y en a exactement 10
    #[test]
    fn test_all_default_models_present() {
        let table = default_pricing();

        assert_eq!(
            table.len(),
            10,
            "default pricing table must contain 10 models"
        );

        // Vérifie la présence des 5 modèles Anthropic
        assert!(table.contains_key("claude-haiku-4-5"));
        assert!(table.contains_key("claude-sonnet-4-5"));
        assert!(table.contains_key("claude-sonnet-4-6"));
        assert!(table.contains_key("claude-opus-4-5"));
        assert!(table.contains_key("claude-opus-4-6"));
        // Vérifie la présence des 5 modèles OpenAI
        assert!(table.contains_key("gpt-4o-mini"));
        assert!(table.contains_key("gpt-4o"));
        assert!(table.contains_key("gpt-4.1-mini"));
        assert!(table.contains_key("gpt-4.1-nano"));
        assert!(table.contains_key("gpt-4.1"));
    }

    // GIVEN des overrides vides
    // WHEN on lookup un modèle connu
    // THEN la table par défaut est utilisée sans erreur
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

    // GIVEN un modèle local Ollama non présent dans la table
    // WHEN on lookup ce modèle
    // THEN None est retourné sans panique
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
