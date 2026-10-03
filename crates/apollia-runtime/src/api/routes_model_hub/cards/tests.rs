//! The card routes, on a temporary data directory, with no request leaving
//! this machine: the fetch itself is exercised against a fake server by the
//! tests of `apollia_llm::model_cards`.

use std::sync::Arc;

use apollia_llm::model_cards::fetch::RegistrySource;
use apollia_llm::model_cards::{default_cache_dir, store_index};

use super::{card_body, cards_body, configured_source};
use crate::api::test_support::{app_state, call, StemLoader};

const INDEX: &str = include_str!("../../../../../apollia-llm/src/model_cards/fixtures/index.json");
const SIGNATURE: &str =
    include_str!("../../../../../apollia-llm/src/model_cards/fixtures/index.json.minisig");
const TEST_KEY: &str =
    include_str!("../../../../../apollia-llm/src/model_cards/fixtures/test-dist.key.pub");

/// An address on this machine where nothing can listen: port 0 is never bound.
fn closed_address() -> String {
    "http://127.0.0.1:0/models/stable/index.json".to_owned()
}

#[tokio::test]
async fn a_registry_cut_by_configuration_gives_an_empty_catalogue_that_says_so() {
    // GIVEN a runtime whose apollia.toml turns the registry off
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("apollia.toml"),
        "[models]\nregistry_enabled = false\n",
    )
    .expect("write config");
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );

    // WHEN the catalogue opens
    let (status, body) = call(state, "GET", "/api/v1/llm/registry/cards?lang=fr", None).await;

    // THEN the catalogue is empty, with the reason and a sentence for the user
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["status"], "disabled");
    assert_eq!(body["cards"], serde_json::json!([]));
    assert!(body["message"]
        .as_str()
        .is_some_and(|m| m.contains("imported from a disk")));
}

#[test]
fn the_models_section_replaces_the_address_and_its_mirror() {
    // GIVEN an apollia.toml pointing the registry at an internal mirror
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("apollia.toml"),
        "[models]\nregistry_url = \"https://models.intranet.example/index.json\"\nregistry_mirror_url = \"\"\n",
    )
    .expect("write config");

    // WHEN the source is read, and read again with no file at all
    let none = apollia_core::config::PolicySettings::none();
    let configured = configured_source(None, tmp.path(), &none);
    let defaults = configured_source(None, &tmp.path().join("absent"), &none);

    // THEN the mirror replaces the address and the fallback is off; without
    //      a file the project registry applies
    assert_eq!(configured.url, "https://models.intranet.example/index.json");
    assert_eq!(configured.mirror_url, None);
    assert!(configured.enabled);
    assert_eq!(
        defaults.url,
        apollia_core::config::DEFAULT_MODEL_REGISTRY_URL
    );
}

#[tokio::test]
async fn offline_the_kept_copy_serves_the_catalogue_in_the_asked_language() {
    // GIVEN a kept copy of the signed index, and no address answering
    let tmp = tempfile::tempdir().expect("tempdir");
    let cache = default_cache_dir(tmp.path());
    store_index(&cache, INDEX.as_bytes(), SIGNATURE).expect("seed the copy");
    let source = RegistrySource {
        url: closed_address(),
        mirror_url: Some(closed_address()),
        enabled: true,
        disabled_by_policy: false,
    };

    // WHEN the catalogue opens in French
    let body = cards_body(&source, &cache, Some(TEST_KEY), "fr")
        .await
        .expect("served");

    // THEN the copy answers, French where written, English otherwise, each
    //      card with the page of its source
    assert_eq!(body.status, "ok");
    assert_eq!(body.origin.as_deref(), Some("cache"));
    assert_eq!(body.cards.len(), 2);
    assert_eq!(body.cards[0].language, "fr");
    assert_eq!(body.cards[1].language, "en");
    assert_eq!(
        body.cards[0].source_url,
        "https://huggingface.co/Qwen/Qwen3-8B"
    );
}

#[tokio::test]
async fn offline_without_a_copy_the_catalogue_is_empty_with_a_clear_message() {
    // GIVEN no copy and no address answering
    let tmp = tempfile::tempdir().expect("tempdir");
    let source = RegistrySource {
        url: closed_address(),
        mirror_url: None,
        enabled: true,
        disabled_by_policy: false,
    };

    // WHEN the catalogue opens
    let body = cards_body(
        &source,
        &default_cache_dir(tmp.path()),
        Some(TEST_KEY),
        "en",
    )
    .await
    .expect("served");

    // THEN it is empty, says why, and recommends nothing
    assert_eq!(body.status, "offline");
    assert!(body.cards.is_empty());
    assert!(body
        .message
        .is_some_and(|m| m.contains("could not be reached")));
}

#[test]
fn one_card_is_read_from_the_copy_with_its_source_and_english_as_fallback() {
    // GIVEN a kept copy of the signed index
    let tmp = tempfile::tempdir().expect("tempdir");
    let cache = default_cache_dir(tmp.path());
    store_index(&cache, INDEX.as_bytes(), SIGNATURE).expect("seed the copy");

    // WHEN one card is asked in French, one in German, and an unknown one
    let qwen = card_body(&cache, Some(TEST_KEY), "qwen3/Qwen3-8B", "fr").expect("qwen");
    let e5 = card_body(&cache, Some(TEST_KEY), "multilingual-e5-small", "de").expect("e5");
    let missing = card_body(&cache, Some(TEST_KEY), "nope", "fr");

    // THEN each comes in the asked language or in English, with its source
    //      page, and the unknown one is a 404
    assert_eq!(qwen.language, "fr");
    assert_eq!(qwen.source_url, "https://huggingface.co/Qwen/Qwen3-8B");
    assert_eq!(e5.language, "en");
    assert_eq!(
        missing.err().map(|(status, _)| status),
        Some(axum::http::StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn the_card_route_answers_404_before_the_catalogue_was_ever_opened() {
    // GIVEN a runtime whose data directory holds no copy
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );

    // WHEN a card is asked
    let (status, body) = call(
        state,
        "GET",
        "/api/v1/llm/registry/card?id=qwen3/Qwen3-8B&lang=fr",
        None,
    )
    .await;

    // THEN the route says the catalogue was never opened
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{body}");
    assert!(body["error"]
        .as_str()
        .is_some_and(|e| e.contains("open the catalogue")));
}

#[tokio::test]
async fn the_readme_route_refuses_an_identifier_that_is_not_owner_and_name() {
    // GIVEN a runtime
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );

    // WHEN a README is asked for a path escape
    let (status, _body) = call(
        state,
        "GET",
        "/api/v1/llm/registry/readme?repo=../etc",
        None,
    )
    .await;

    // THEN it is refused before any request, and nothing is written
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read_dir(tmp.path()).expect("read").count(), 0);
}

#[tokio::test]
async fn a_registry_cut_by_the_policy_says_so_and_the_import_from_a_disk_stays() {
    // GIVEN an apollia.toml that leaves the registry on, and a policy whose
    //       [settings] cut it
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("apollia.toml"),
        "[models]\nregistry_enabled = true\n",
    )
    .expect("write config");
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );
    let verdict =
        apollia_permissions::PolicyVerdict::Valid(Box::new(apollia_permissions::ValidPolicy {
            policy: apollia_permissions::parse_policy(
                "schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n\
                 [settings]\n\"models.registry_enabled\" = false\n",
            )
            .expect("policy parses"),
            key_fingerprint: "00".into(),
            publisher: "it".into(),
            signed_at: "2026-10-01T09:00:00Z".parse().expect("datetime"),
        }));
    state.inference_governance = crate::governance::InferenceGovernance::new(Arc::new(verdict));

    // WHEN the catalogue opens
    let (status, body) = call(state, "GET", "/api/v1/llm/registry/cards?lang=fr", None).await;

    // THEN it is empty, turned off, imposed by the policy, and a disk import
    //      is still offered
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["status"], "disabled");
    assert_eq!(body["cards"], serde_json::json!([]));
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("imposed by the organisation policy"),
        "{message}"
    );
    assert!(message.contains("imported from a disk"), "{message}");
}

#[test]
fn a_policy_replaces_the_registry_address_by_an_internal_mirror() {
    // GIVEN an apollia.toml pointing at the project registry, and a policy
    //       fixing an internal address with no fallback
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("apollia.toml"),
        "[models]\nregistry_url = \"https://registry.apollia.fr/models/stable/index.json\"\n",
    )
    .expect("write config");
    let imposed = apollia_core::config::PolicySettings::none()
        .impose(
            "models.registry_url",
            serde_json::json!("https://models.intranet.example/index.json"),
        )
        .impose("models.registry_mirror_url", serde_json::json!(""));

    // WHEN the source is read under the policy
    let source = configured_source(None, tmp.path(), &imposed);

    // THEN the internal address replaces the file's, the fallback is off and
    //      the registry stays on
    assert_eq!(source.url, "https://models.intranet.example/index.json");
    assert_eq!(source.mirror_url, None);
    assert!(source.enabled);
    assert!(!source.disabled_by_policy);
}
