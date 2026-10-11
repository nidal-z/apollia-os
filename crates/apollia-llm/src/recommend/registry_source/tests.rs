//! Tests of the registry-sourced candidates: which cards are suggested, and
//! the order the rule gives them.

use super::*;
use crate::model_cards::shipped_index;

fn shipped_cards() -> Vec<ModelCard> {
    shipped_index().expect("the shipped index loads").cards
}

fn card(id: &str) -> ModelCard {
    shipped_cards()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("{id} is in the shipped index"))
}

#[test]
fn test_every_language_card_carries_what_the_rule_reads() {
    // GIVEN the shipped index
    let cards = shipped_cards();

    // WHEN its language cards are read
    let language: Vec<&ModelCard> = cards
        .iter()
        .filter(|c| c.kind == CardKind::Language)
        .collect();

    // THEN each states a release date, a parameter count and a usage
    assert!(!language.is_empty());
    for card in language {
        assert!(
            card.released.as_deref().and_then(months_of).is_some(),
            "{} has no release date",
            card.id
        );
        assert!(
            card.params_b.is_some_and(|p| p > 0.0),
            "{} has no size",
            card.id
        );
        assert!(card.usage.is_some(), "{} has no usage", card.id);
    }
}

#[test]
fn test_a_card_is_suggested_only_when_it_is_open_general_and_calls_tools() {
    // GIVEN a card that meets every condition, and four that each miss one
    let suggested = card("qwen3.8/Qwen3.8-27B");
    let closed_licence = card("gemma3/gemma-3-12b-it");
    let for_code = card("qwen3-coder/Qwen3-Coder-30B-A3B-Instruct");
    let no_tools = card("olmo3/Olmo-3-7B-Think");
    let regional = card("aya/tiny-aya-water");
    let embeddings = card("bge-m3");

    // WHEN each is judged
    // THEN only the first is suggested unasked
    assert!(is_suggested(&suggested));
    assert!(!is_suggested(&closed_licence));
    assert!(!is_suggested(&for_code));
    assert!(!is_suggested(&no_tools));
    assert!(!is_suggested(&regional));
    assert!(!is_suggested(&embeddings));
}

#[test]
fn test_the_rule_trades_size_against_age() {
    // GIVEN a model, one twice its size, and one eight months older
    let reference = rule_score(8.0, 0.0);

    // WHEN the three are scored
    let doubled = rule_score(16.0, 0.0);
    let older = rule_score(8.0, 8.0);
    let doubled_and_older = rule_score(16.0, 8.0);

    // THEN a doubling is worth what eight months cost, larger beats smaller
    // and newer beats older
    assert!(doubled > reference);
    assert!(older < reference);
    assert!((doubled_and_older - reference).abs() < 1e-9);
}

#[test]
fn test_the_rule_score_stays_within_its_bounds() {
    // GIVEN a tiny model, a huge one, and a very old one
    // WHEN each is scored
    let tiny = rule_score(0.05, 0.0);
    let huge = rule_score(500.0, 0.0);
    let ancient = rule_score(8.0, 600.0);

    // THEN the size term is held between its floor and ceiling, and the age
    // term takes away no more than its ceiling
    assert!((tiny - SIZE_FLOOR).abs() < 1e-9);
    assert!((huge - SIZE_CEILING).abs() < 1e-9);
    assert!((ancient - (rule_score(8.0, 0.0) - AGE_CEILING)).abs() < 1e-9);
}

#[test]
fn test_the_manifest_groups_the_suggested_cards_and_names_their_repositories() {
    // GIVEN the shipped cards
    let cards = shipped_cards();
    let expected = cards.iter().filter(|c| is_suggested(c)).count();

    // WHEN the manifest is built
    let manifest = manifest_from_cards(&cards).expect("the cards group without a clash");

    // THEN every suggested card is one variant, scored by the rule and
    // carrying a GGUF repository to ask the Hub about
    let variants: Vec<&Variant> = manifest
        .families()
        .iter()
        .flat_map(|f| f.variants.iter())
        .collect();
    assert_eq!(variants.len(), expected);
    assert!(variants.len() > 50);
    for variant in &variants {
        assert!(variant.rule_score.is_some(), "{}", variant.hf_name);
        assert!(!variant.gguf_repos.is_empty(), "{}", variant.hf_name);
    }
    let qwen = manifest.family("qwen3.5").expect("Qwen3.5 is suggested");
    assert_eq!(qwen.label, "Qwen3.5");
    assert_eq!(qwen.license, "apache-2.0");
    assert!(qwen.variants.len() > 1);
    assert!(manifest.family("qwen3-coder").is_none());
    assert!(manifest.family("gemma3").is_none());
}

#[test]
fn test_a_newer_generation_of_the_same_size_is_ordered_first() {
    // GIVEN the manifest of the shipped cards
    let manifest = manifest_from_cards(&shipped_cards()).expect("the cards group");
    let score_of = |family: &str, name: &str| {
        let family = manifest.family(family).expect("the family is suggested");
        let variant = family
            .variants
            .iter()
            .find(|v| v.hf_name == name)
            .expect("the size is suggested");
        manifest
            .base_score(family, variant)
            .expect("a registry entry is scored")
    };

    // WHEN two generations of one publisher at the same size are compared
    let newer = score_of("qwen3.8", "Qwen3.8-27B");
    let older = score_of("qwen3.5", "Qwen3.5-27B");

    // THEN the newer one is ahead
    assert!(newer > older);
}

#[test]
fn test_a_date_is_read_as_months() {
    // GIVEN two dates one month apart, and text that is not a date
    // WHEN each is read
    let march = months_of("2026-03-01").expect("a date");
    let april = months_of("2026-04-01").expect("a date");

    // THEN the dates are a month apart and the rest is refused
    assert!((april - march - 1.0).abs() < 1e-9);
    assert_eq!(months_of("2026-13-01"), None);
    assert_eq!(months_of("soon"), None);
    assert_eq!(months_of(""), None);
}

#[test]
fn test_the_selection_holds_every_language_model_and_the_suggestion_only_some() {
    // GIVEN the shipped cards
    let cards = shipped_cards();
    let language = cards
        .iter()
        .filter(|c| c.kind == CardKind::Language && c.draft.is_none())
        .count();

    // WHEN the selection and the suggestion are built
    let selection = selection_from_cards(&cards).expect("the cards group");
    let suggestion = manifest_from_cards(&cards).expect("the cards group");
    let sizes = |m: &FamilyManifest| m.families().iter().map(|f| f.variants.len()).sum::<usize>();

    // THEN the selection has one size for each language card, among them the
    // code models and the models under a licence the suggestion leaves out
    assert_eq!(sizes(&selection), language);
    assert!(sizes(&suggestion) < sizes(&selection));
    assert!(selection.family("qwen3-coder").is_some());
    assert!(selection.family("gemma3").is_some());
    let olmo = selection.family("olmo3").expect("Olmo 3 is selected");
    assert_eq!(olmo.tool_calling, ToolCalling::Native);
    let smolvlm = selection.family("smolvlm").expect("SmolVLM is selected");
    assert_eq!(smolvlm.tool_calling, ToolCalling::None);
}
