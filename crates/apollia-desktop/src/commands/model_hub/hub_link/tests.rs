//! The Model Hub, reading side, attached to a daemon: each command reaches its
//! route with the token of this installation, and a refusal of the daemon
//! reaches the interface in the daemon's own words.

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

/// A daemon answering `GET route` with `status` and `body`, only with the token.
async fn daemon_answering(route: &str, status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

fn search(query: &str) -> HfSearchParams {
    HfSearchParams {
        query: query.to_owned(),
        format: apollia_llm::SearchFormat::default(),
        sort: Some("downloads".to_owned()),
        limit: Some(20),
        fits_only: true,
        hf_token: None,
    }
}

/// The body a route of the daemon refuses with.
fn refusal(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": message })
}

#[test]
fn test_the_five_commands_of_the_hub_pass_the_gate_in_attached_mode() {
    // GIVEN the application attached to a daemon
    let commands = [
        "get_hardware_profile",
        "search_hf_models",
        "get_hf_model",
        "list_installed_models",
        "recommend_models",
    ];

    // WHEN each command of the family is gated
    let verdicts: Vec<_> = commands
        .iter()
        .map(|c| gate(LinkMode::Attached, c))
        .collect();

    // THEN none is refused
    assert!(verdicts.iter().all(Result::is_ok), "{verdicts:?}");
}

#[tokio::test]
async fn test_the_hardware_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon that measured 32 GB on this machine
    let server = daemon_answering(
        "/api/v1/llm/hardware",
        200,
        serde_json::json!({ "total_ram_gb": 32.0, "total_capacity_gb": 19.2 }),
    )
    .await;

    // WHEN the attached arm reads the hardware
    let hardware = hardware_on(&attached(&server)).await.unwrap();

    // THEN the daemon's profile comes back
    assert_eq!(hardware["total_ram_gb"], 32.0);
}

#[tokio::test]
async fn test_a_refusal_of_the_hardware_route_is_shown_in_its_own_words() {
    // GIVEN a daemon that cannot probe the hardware
    let server = daemon_answering(
        "/api/v1/llm/hardware",
        500,
        refusal("hardware detection failed"),
    )
    .await;

    // WHEN the attached arm reads the hardware
    let error = hardware_on(&attached(&server)).await.unwrap_err();

    // THEN the interface receives the daemon's message
    assert_eq!(error, "hardware detection failed");
}

#[tokio::test]
async fn test_a_search_sends_its_parameters_to_the_daemon() {
    // GIVEN a daemon answering a search for "Qwen3", sorted by downloads,
    //       kept to what fits
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/llm/registry/search"))
        .and(query_param("q", "Qwen3"))
        .and(query_param("format", "gguf"))
        .and(query_param("sort", "downloads"))
        .and(query_param("limit", "20"))
        .and(query_param("fits_only", "true"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "results": [{ "id": "Qwen/Qwen3-8B" }] })),
        )
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the attached arm searches
    let page = search_on(&attached(&server), search("Qwen3"))
        .await
        .unwrap();

    // THEN the daemon's page comes back
    assert_eq!(page["results"][0]["id"], "Qwen/Qwen3-8B");
}

#[tokio::test]
async fn test_an_offline_search_of_the_daemon_is_shown_in_its_own_words() {
    // GIVEN a daemon that reaches no organisation
    let server = daemon_answering(
        "/api/v1/llm/registry/search",
        503,
        refusal("search is unavailable offline"),
    )
    .await;

    // WHEN the attached arm searches
    let error = search_on(&attached(&server), search("Qwen3"))
        .await
        .unwrap_err();

    // THEN the interface receives the daemon's message
    assert_eq!(error, "search is unavailable offline");
}

#[tokio::test]
async fn test_a_model_card_is_read_by_its_encoded_organisation_and_name() {
    // GIVEN a daemon holding the card of a repository whose name has a space
    let server = daemon_answering(
        "/api/v1/llm/registry/model/Qwen/Qwen3%208B",
        200,
        serde_json::json!({ "id": "Qwen/Qwen3 8B", "variants": [] }),
    )
    .await;
    let cache = apollia_llm::HfModelTypeCache::new();

    // WHEN the attached arm reads the card
    let card = model_on(&attached(&server), &cache, "Qwen/Qwen3 8B".to_owned(), None)
        .await
        .unwrap();

    // THEN the daemon's card comes back
    assert_eq!(card["id"], "Qwen/Qwen3 8B");
}

#[tokio::test]
async fn test_a_model_the_daemon_cannot_find_is_shown_in_its_own_words() {
    // GIVEN a daemon for which the repository does not exist
    let server = daemon_answering(
        "/api/v1/llm/registry/model/acme/missing",
        404,
        refusal("failed to fetch model acme/missing: not found"),
    )
    .await;
    let cache = apollia_llm::HfModelTypeCache::new();

    // WHEN the attached arm reads the card
    let error = model_on(&attached(&server), &cache, "acme/missing".to_owned(), None)
        .await
        .unwrap_err();

    // THEN the interface receives the daemon's message
    assert_eq!(error, "failed to fetch model acme/missing: not found");
}

#[tokio::test]
async fn test_a_repository_id_without_an_owner_is_refused_before_the_daemon() {
    // GIVEN a daemon that must not be asked
    let server = MockServer::start().await;
    let cache = apollia_llm::HfModelTypeCache::new();

    // WHEN the attached arm reads a card by a bare name
    let error = model_on(&attached(&server), &cache, "Qwen3".to_owned(), None)
        .await
        .unwrap_err();

    // THEN it is refused with the embedded wording, and nothing was sent
    assert!(error.starts_with("failed to fetch model Qwen3"), "{error}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_the_installed_models_are_read_from_the_daemon() {
    // GIVEN a daemon whose models folder holds one model
    let server = daemon_answering(
        "/api/v1/llm/installed-models",
        200,
        serde_json::json!([{ "id": "deep", "name": "deep.gguf", "kind": "llm" }]),
    )
    .await;

    // WHEN the attached arm lists the installed models
    let models = installed_on(&attached(&server)).await.unwrap();

    // THEN the daemon's list comes back
    assert_eq!(models[0]["name"], "deep.gguf");
}

#[tokio::test]
async fn test_an_unreadable_index_of_the_daemon_is_shown_in_its_own_words() {
    // GIVEN a daemon that cannot read its model index
    let server = daemon_answering(
        "/api/v1/llm/installed-models",
        500,
        refusal("failed to read the model index: permission denied"),
    )
    .await;

    // WHEN the attached arm lists the installed models
    let error = installed_on(&attached(&server)).await.unwrap_err();

    // THEN the interface receives the daemon's message
    assert_eq!(error, "failed to read the model index: permission denied");
}

#[tokio::test]
async fn test_the_recommendation_is_asked_of_the_daemon_with_its_parameters() {
    // GIVEN a daemon answering the recommendation for a 8192-token window
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/llm/recommendations"))
        .and(query_param("n_ctx", "8192"))
        .and(query_param("limit", "3"))
        .and(query_param("fits_only", "true"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "status": "unreachable", "detail": "offline", "selection": [] }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let params = apollia_runtime::recommendation::RecommendParams {
        hf_token: None,
        limit: Some(3),
        n_ctx: Some(8192),
        fits_only: true,
    };

    // WHEN the attached arm asks for the recommendation
    let outcome = recommend_on(&attached(&server), params).await.unwrap();

    // THEN the daemon's outcome comes back with its status
    assert_eq!(outcome["status"], "unreachable");
}

#[tokio::test]
async fn test_a_refused_recommendation_is_shown_in_its_own_words() {
    // GIVEN a daemon whose shipped model table does not load
    let server = daemon_answering(
        "/api/v1/llm/recommendations",
        500,
        refusal("the shipped model table is invalid: bad row"),
    )
    .await;

    // WHEN the attached arm asks for the recommendation
    let error = recommend_on(&attached(&server), Default::default())
        .await
        .unwrap_err();

    // THEN the interface receives the daemon's message
    assert_eq!(error, "the shipped model table is invalid: bad row");
}
