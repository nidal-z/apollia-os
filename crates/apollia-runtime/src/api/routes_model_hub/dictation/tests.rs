//! `GET /api/v1/llm/dictation`, through the whole router.

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::AcceleratorProfile;

use super::dictation_body;
use crate::api::routes_model_hub::embeddings::CountedLanguageModel;
use crate::api::test_support::{app_state, call};

#[tokio::test]
async fn the_route_lists_the_category_with_its_verdicts_and_one_recommendation() {
    // GIVEN a runtime whose data directory holds no model
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = app_state(tmp.path().to_path_buf());

    // WHEN the dictation route is asked with the size of a language model
    let (status, body) = call(
        state,
        "GET",
        "/api/v1/llm/dictation?language_model_bytes=4920000000",
        None,
    )
    .await;

    // THEN it lists every entry with its file, digest, card, footprint and
    // verdict, recommends one of them with its reason, and says which
    // language model it counted
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let models = body["models"].as_array().expect("models is a list");
    let ids: Vec<&str> = models.iter().filter_map(|m| m["id"].as_str()).collect();
    assert!(ids.contains(&"whisper-large-v3-french-q5_0"), "{ids:?}");
    assert!(ids.contains(&"whisper-base"), "{ids:?}");
    for model in models {
        assert_eq!(model["sha256"].as_str().map(str::len), Some(64), "{model}");
        assert!(model["card"].is_string(), "{model}");
        assert!(model["fits"].is_boolean(), "{model}");
        assert!(model["footprint_gb"].is_number(), "{model}");
    }
    let recommended = body["recommended"].as_str().expect("one is recommended");
    assert!(ids.contains(&recommended));
    let reason = body["reason"]["kind"].as_str().expect("a reason kind");
    assert!(["fits_beside", "no_room_beside"].contains(&reason));
    assert_eq!(body["language_model"]["source"], "query");
    assert!(body["hardware"]["total_capacity_gb"].is_number());
}

#[test]
fn the_body_says_why_a_small_machine_gets_the_smallest_file() {
    // GIVEN an 8 GB machine with no accelerator running an 8B language model
    let profile = HardwareProfile {
        total_ram_gb: 8.0,
        available_ram_gb: 4.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 4,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: 4.8,
        devices: Vec::new(),
    };
    let language = CountedLanguageModel {
        source: "query".to_owned(),
        id: None,
        size_bytes: 4_920_000_000,
    };

    // WHEN the response is built
    let body = dictation_body(profile, Some(language)).expect("the table loads");

    // THEN no file fits beside the language model, the smallest is offered
    // and the reason carries the footprints compared
    assert_eq!(body.recommended.as_deref(), Some("whisper-tiny-q5_1"));
    let reason = serde_json::to_value(&body.reason).expect("serialise");
    assert_eq!(reason["kind"], "no_room_beside");
    assert!(reason["language_gb"].as_f64().expect("number") > 4.8);
}
