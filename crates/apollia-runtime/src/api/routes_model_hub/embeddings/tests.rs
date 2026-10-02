//! `GET /api/v1/llm/embeddings`, through the whole router and on a model index
//! built in a temporary directory.

use std::path::Path;
use std::sync::Arc;

use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::ModelIndex;

use super::installed_language_model;
use crate::api::test_support::{app_state, call, StemLoader};

/// A GGUF header (version 3, no tensor) naming `arch`, with an optional
/// `{arch}.pooling_type`, followed by `padding` bytes standing for weights.
fn gguf(arch: &str, pooling: Option<u32>, padding: usize) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&(1 + u64::from(pooling.is_some())).to_le_bytes());
    let key = "general.architecture";
    out.extend_from_slice(&(key.len() as u64).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(arch.len() as u64).to_le_bytes());
    out.extend_from_slice(arch.as_bytes());
    if let Some(value) = pooling {
        let key = format!("{arch}.pooling_type");
        out.extend_from_slice(&(key.len() as u64).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend(std::iter::repeat_n(0u8, padding));
    out
}

fn indexed(models_dir: &Path) {
    let mut index = ModelIndex::default();
    index.rebuild(models_dir).expect("rebuild");
    index
        .save(&models_dir.join(INDEX_FILE_NAME))
        .expect("save the index");
}

#[tokio::test]
async fn the_route_lists_both_entries_and_recommends_one_beside_the_given_model() {
    // GIVEN a runtime whose data directory holds no model
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );

    // WHEN the embeddings route is asked with the size of a language model
    let (status, body) = call(
        state,
        "GET",
        "/api/v1/llm/embeddings?language_model_bytes=3221225472",
        None,
    )
    .await;

    // THEN it lists the two entries with their technical facts, recommends one
    // of them with its reason, and says which language model it counted
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let ids: Vec<&str> = body["models"]
        .as_array()
        .expect("models is a list")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["multilingual-e5-small", "qwen3-embedding-0.6b"]);
    assert_eq!(body["models"][0]["dimensions"], 384);
    assert_eq!(body["models"][0]["pooling"], "mean");
    assert_eq!(body["models"][0]["query_prefix"], "query: ");
    let recommended = body["recommended"].as_str().expect("one is recommended");
    assert!(ids.contains(&recommended));
    let reason = body["reason"]["kind"].as_str().expect("a reason kind");
    assert!(["fits_beside", "no_room_beside"].contains(&reason));
    assert_eq!(body["language_model"]["source"], "query");
    assert_eq!(body["language_model"]["size_bytes"], 3_221_225_472u64);
    assert!(body["hardware"]["total_capacity_gb"].is_number());
}

#[tokio::test]
async fn without_a_size_the_route_counts_the_largest_installed_language_model() {
    // GIVEN a data directory whose models are two language models and one
    // embeddings model, the embeddings one the largest
    let tmp = tempfile::tempdir().expect("tempdir");
    let models = tmp.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");
    std::fs::write(models.join("small.gguf"), gguf("llama", None, 100)).expect("write");
    std::fs::write(models.join("large.gguf"), gguf("qwen3", None, 4000)).expect("write");
    std::fs::write(models.join("e5.gguf"), gguf("bert", Some(1), 9000)).expect("write");
    indexed(&models);
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        tmp.path().to_path_buf(),
    );

    // WHEN the embeddings route is asked with no size
    let (status, body) = call(state, "GET", "/api/v1/llm/embeddings", None).await;

    // THEN the language model counted is the largest language model, never the
    // embeddings one
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["language_model"]["source"], "installed");
    assert_eq!(body["language_model"]["id"], "large");
}

#[test]
fn a_machine_with_no_index_counts_no_language_model() {
    // GIVEN a models directory with no index
    let tmp = tempfile::tempdir().expect("tempdir");

    // WHEN the installed language model is looked up
    let counted = installed_language_model(tmp.path());

    // THEN none is counted
    assert_eq!(counted, None);
}
