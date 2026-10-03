use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;

fn e5() -> EmbeddingModel {
    EmbeddingCatalogue::embedded()
        .unwrap()
        .get("multilingual-e5-small")
        .unwrap()
        .clone()
}

/// A stand-in engine answering `/v1/embeddings` with one vector per input,
/// `[k + 1, 0, ...]` scaled by `scale`, so the order and the normalisation can
/// both be read back.
struct Vectors {
    scale: f32,
}

impl wiremock::Respond for Vectors {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let inputs = body["input"].as_array().unwrap();
        // Answered in reverse, as an engine is free to: the client reorders.
        let data: Vec<serde_json::Value> = (0..inputs.len())
            .rev()
            .map(|k| {
                serde_json::json!({
                    "object": "embedding",
                    "index": k,
                    "embedding": [self.scale * (k as f32 + 1.0), 0.0, self.scale],
                })
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": data,
            "model": body["model"],
        }))
    }
}

async fn engine(scale: f32) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(Vectors { scale })
        .mount(&server)
        .await;
    server
}

fn texts(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("text {i}")).collect()
}

#[tokio::test]
async fn test_a_batch_comes_back_in_order_prefixed_and_normalised() {
    // GIVEN a stand-in engine on the loopback and the e5 entry
    let server = engine(3.0).await;
    let client = EmbeddingsClient::new(&format!("{}/v1", server.uri()), e5()).unwrap();

    // WHEN two passages are embedded
    let vectors = client
        .embed(&texts(2), EmbedPurpose::Passage)
        .await
        .unwrap();

    // THEN there is one unit vector per text, in the order sent, and the
    // engine received the passage prefix of the entry before each text
    assert_eq!(vectors.len(), 2);
    for v in &vectors {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "{v:?}");
    }
    assert!(vectors[0][0] < vectors[1][0], "order kept: {vectors:?}");
    let received = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&received.body).unwrap();
    assert_eq!(body["input"][0], "passage: text 0");
    assert_eq!(body["input"][1], "passage: text 1");
    assert_eq!(body["model"], "multilingual-e5-small");
}

#[tokio::test]
async fn test_a_query_takes_the_query_prefix() {
    // GIVEN a stand-in engine and the e5 entry
    let server = engine(1.0).await;
    let client = EmbeddingsClient::new(&format!("{}/v1", server.uri()), e5()).unwrap();

    // WHEN a query is embedded
    client
        .embed(&["where is the invoice".to_owned()], EmbedPurpose::Query)
        .await
        .unwrap();

    // THEN the engine received the query prefix of the entry
    let received = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&received.body).unwrap();
    assert_eq!(body["input"][0], "query: where is the invoice");
}

#[tokio::test]
async fn test_a_large_set_goes_out_in_several_requests() {
    // GIVEN a stand-in engine
    let server = engine(1.0).await;
    let client = EmbeddingsClient::new(&format!("{}/v1", server.uri()), e5()).unwrap();

    // WHEN more texts than one batch holds are embedded
    let vectors = client
        .embed(&texts(MAX_BATCH + 3), EmbedPurpose::Passage)
        .await
        .unwrap();

    // THEN every text has its vector, sent in two requests
    assert_eq!(vectors.len(), MAX_BATCH + 3);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn test_an_engine_error_is_typed_with_its_status() {
    // GIVEN an engine answering 500
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(500).set_body_string("model not loaded"))
        .mount(&server)
        .await;
    let client = EmbeddingsClient::new(&server.uri(), e5()).unwrap();

    // WHEN a text is embedded
    let err = client
        .embed(&texts(1), EmbedPurpose::Passage)
        .await
        .unwrap_err();

    // THEN the error carries the status and the start of the body
    assert_eq!(
        err,
        EmbeddingError::Status {
            status: 500,
            body: "model not loaded".to_owned()
        }
    );
    assert!(!err.falls_back_to_keywords());
}

#[tokio::test]
async fn test_a_missing_vector_and_a_zero_vector_are_typed() {
    // GIVEN an engine answering one vector for two texts
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"index": 0, "embedding": [1.0, 0.0]}]
        })))
        .mount(&server)
        .await;
    let client = EmbeddingsClient::new(&server.uri(), e5()).unwrap();

    // WHEN two texts are embedded
    let err = client
        .embed(&texts(2), EmbedPurpose::Passage)
        .await
        .unwrap_err();

    // THEN the mismatch is named
    assert_eq!(
        err,
        EmbeddingError::CountMismatch {
            sent: 2,
            received: 1
        }
    );

    // AND an engine answering a zero vector is refused, not normalised to NaN
    let zero = engine(0.0).await;
    let client = EmbeddingsClient::new(&format!("{}/v1", zero.uri()), e5()).unwrap();
    let err = client
        .embed(&texts(1), EmbedPurpose::Passage)
        .await
        .unwrap_err();
    assert_eq!(err, EmbeddingError::ZeroVector { index: 0 });
}

#[test]
fn test_only_the_loopback_is_accepted() {
    // GIVEN addresses on this machine and elsewhere
    let local = [
        "http://127.0.0.1:8081/v1",
        "http://localhost:8081/v1",
        "http://[::1]:8081/v1",
        "http://127.0.0.2:9000",
    ];
    let remote = [
        "http://192.168.1.10:8081/v1",
        "https://api.openai.com/v1",
        "http://example.com/v1",
        "https://127.0.0.1:8081/v1",
        "not a url",
    ];

    // WHEN each is checked, and a client is built on a remote one
    let refused = EmbeddingsClient::new("https://api.openai.com/v1", e5()).unwrap_err();

    // THEN only the loopback over HTTP passes, and the client refuses the rest
    for url in local {
        assert!(is_loopback_url(url), "{url}");
    }
    for url in remote {
        assert!(!is_loopback_url(url), "{url}");
    }
    assert_eq!(
        refused,
        EmbeddingError::NotLoopback {
            url: "https://api.openai.com/v1".to_owned()
        }
    );
}

#[test]
fn test_without_a_default_the_error_is_no_model_and_means_keywords_alone() {
    // GIVEN the registry and no default embeddings model
    let catalogue = EmbeddingCatalogue::embedded().unwrap();

    // WHEN the chosen model is resolved, without a default and with an
    // identifier the registry does not know
    let none = chosen_model(None, &catalogue).unwrap_err();
    let unknown = chosen_model(Some("gone"), &catalogue).unwrap_err();
    let known = chosen_model(Some("qwen3-embedding-0.6b"), &catalogue).unwrap();

    // THEN the first is the typed "no embeddings model", both tell the caller
    // to go on with keywords alone, and a known identifier gives its entry
    assert_eq!(none, EmbeddingError::NoModel);
    assert_eq!(none.to_string(), "no embeddings model is set");
    assert!(none.falls_back_to_keywords());
    assert!(unknown.falls_back_to_keywords());
    assert_eq!(known.id, "qwen3-embedding-0.6b");
}

#[test]
fn test_normalising_keeps_the_direction_and_refuses_a_zero_vector() {
    // GIVEN a vector of norm five and a zero vector
    let mut v = vec![3.0_f32, 4.0];
    let mut zero = vec![0.0_f32, 0.0];

    // WHEN both are normalised
    let ok = normalize(&mut v);
    let refused = normalize(&mut zero);

    // THEN the first is a unit vector in the same direction, the second refused
    assert!(ok);
    assert!(
        (v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6,
        "{v:?}"
    );
    assert!(!refused);
}
