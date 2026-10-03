use std::sync::Arc;

use apollia_llm::meta::next_steps::NextStepsRequest;
use apollia_llm::meta::rewrite_input::RewriteInputRequest;
use apollia_llm::LlmRouter;

use super::*;

fn examples_for(server: &str) -> Vec<CoachingExample> {
    coaching(CoachingRequest {
        server_name: server.to_string(),
        server_title: None,
    })
}

#[test]
fn test_coaching_cards_are_written_in_one_language() {
    // GIVEN every branch of the connector heuristic, including the seeded
    // `filesystem` server and the generic fallback
    let servers = [
        "notion",
        "github",
        "slack",
        "filesystem",
        "brave-search",
        "totally-unknown",
    ];
    // These go straight into the wizard's final step with no translation
    // layer, so a French card lands untouched in an English window. A
    // non-ASCII scan alone would not do: "Liste les fichiers" is pure ASCII,
    // hence the check on the leading verb of each text.
    let english_openers = [
        "Summarise",
        "List",
        "Explore",
        "Run",
        "Open",
        "Get",
        "Ask",
        "Show",
        "Look",
        "Search",
        "Which",
    ];

    for server in servers {
        // WHEN their cards are generated
        let cards = examples_for(server);

        // THEN every visible string reads in the codebase language
        assert!(!cards.is_empty(), "{server} produced no card");
        for card in cards {
            for text in [&card.title, &card.description, &card.prompt] {
                let first = text.split_whitespace().next().unwrap_or("");
                assert!(
                    english_openers.contains(&first),
                    "{server}: '{text}' does not open in English"
                );
            }
        }
    }

    // The negative case, so this test is known to be able to fail.
    assert!(!english_openers.contains(&"Liste")); // French, on purpose
}

#[test]
fn test_unknown_server_still_gets_a_card_naming_it() {
    // GIVEN a connector the heuristic knows nothing about
    // WHEN coaching is generated
    let cards = examples_for("acme-widgets");

    // THEN one generic card comes back and it names the server, so the
    // wizard's last step is never blank
    assert_eq!(cards.len(), 1);
    assert!(cards[0].title.contains("acme-widgets"));
}

#[tokio::test]
async fn test_a_rewrite_without_router_returns_the_text_and_says_why() {
    // GIVEN no router mounted
    let request = RewriteInputRequest {
        original_text: "fix it".into(),
        work_context: None,
    };

    // WHEN the prompt is rewritten
    let reply = rewrite(None, request).await;

    // THEN the original text comes back, flagged as having no router
    assert_eq!(reply.rewritten_text, "fix it");
    assert_eq!(reply.fallback, Some(RewriteFallbackReason::NoRouter));
}

#[tokio::test]
async fn test_a_rewrite_with_an_empty_router_says_no_backend() {
    // GIVEN a router that holds no backend
    let router = Arc::new(LlmRouter::empty());
    let request = RewriteInputRequest {
        original_text: "fix it".into(),
        work_context: None,
    };

    // WHEN the prompt is rewritten
    let reply = rewrite(Some(&router), request).await;

    // THEN the cause is the missing backend, not the missing router
    assert_eq!(reply.rewritten_text, "fix it");
    assert_eq!(reply.fallback, Some(RewriteFallbackReason::NoBackend));
}

#[tokio::test]
async fn test_next_steps_without_router_come_from_the_heuristic() {
    // GIVEN no router mounted and an empty set of facts
    let request = NextStepsRequest::default();

    // WHEN the next steps are generated
    let reply = next_steps(None, request).await;

    // THEN the heuristic writes them and says so
    assert!(!reply.from_llm);
    assert!(!reply.steps.is_empty());
}

#[test]
fn test_an_empty_description_parses_with_low_confidence() {
    // GIVEN an empty description
    let request = ParseAutomationRequest {
        input: String::new(),
        known_agents: Vec::new(),
    };

    // WHEN it is parsed
    let parsed = parse(&request, chrono::Utc::now());

    // THEN the parse asks the operator to resolve an ambiguity
    assert!(!parsed.ambiguities.is_empty());
}
