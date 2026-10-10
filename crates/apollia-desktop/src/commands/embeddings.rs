//! Tauri IPC commands for the embeddings model, the one search by meaning
//! uses.
//!
//! Each command delegates to the runtime's HTTP API, embedded or attached:
//! `GET /api/v1/llm/embeddings` lists the embeddings models of the registry
//! and names the one for this machine, `PUT /api/v1/llm/embeddings/default`
//! makes one of them the model search uses. The file itself arrives through
//! the verified download or the import, like any other model.

use crate::attached::RuntimeLink;
use tauri::State;

const EMBEDDINGS_PATH: &str = "/api/v1/llm/embeddings";
const DEFAULT_PATH: &str = "/api/v1/llm/embeddings/default";
const TEST_PATH: &str = "/api/v1/llm/embeddings/test";

/// The route that recommends beside a language model of `language_model_bytes`
/// bytes, or beside the largest installed one when the size is absent.
fn recommendation_path(language_model_bytes: Option<u64>) -> String {
    match language_model_bytes {
        Some(bytes) => format!("{EMBEDDINGS_PATH}?language_model_bytes={bytes}"),
        None => EMBEDDINGS_PATH.to_owned(),
    }
}

fn default_body(model: &str) -> serde_json::Value {
    serde_json::json!({ "model": model })
}

/// The embeddings models of the registry and the one recommended for this
/// machine, with the reason. Answered without the network.
///
/// `language_model_bytes` is the size of the language model the recommended
/// entry has to fit beside; absent, the largest installed one is counted.
#[tauri::command]
pub async fn get_embeddings_recommendation(
    link: State<'_, RuntimeLink>,
    language_model_bytes: Option<u64>,
) -> Result<serde_json::Value, String> {
    link.get_json(&recommendation_path(language_model_bytes))
        .await
}

/// Makes `model`, an embeddings entry of the registry, the model search by
/// meaning uses. The answer says whether its file is installed. Rejected with
/// the runtime's message when the identifier names no embeddings entry.
#[tauri::command]
pub async fn set_default_embeddings_model(
    link: State<'_, RuntimeLink>,
    model: String,
) -> Result<serde_json::Value, String> {
    link.put_json(DEFAULT_PATH, &default_body(&model)).await
}

/// Tries the embeddings model search uses on one sentence: the model is
/// loaded and answers a vector. The answer names the model, the size of the
/// vector and the time taken; a machine that is not ready rejects with the
/// runtime's reason.
#[tauri::command]
pub async fn test_embeddings_model(
    link: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    link.post_json(TEST_PATH, &serde_json::json!({})).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_recommendation_route_carries_the_language_model_size_when_given() {
        // GIVEN the size of the language model the onboarding just wired
        let bytes = Some(4_920_000_000_u64);

        // WHEN the route is built with it, and without it
        let with_size = recommendation_path(bytes);
        let without = recommendation_path(None);

        // THEN the size travels as the query the route reads, and its absence
        // leaves the route to count the largest installed language model
        assert_eq!(
            with_size,
            "/api/v1/llm/embeddings?language_model_bytes=4920000000"
        );
        assert_eq!(without, "/api/v1/llm/embeddings");
    }

    #[test]
    fn test_the_default_body_names_the_model_under_the_key_the_route_reads() {
        // GIVEN the identifier of a registry entry
        let model = "multilingual-e5-small";

        // WHEN the body of the default route is built
        let body = default_body(model);

        // THEN it carries the identifier under `model`, and nothing else
        assert_eq!(
            body,
            serde_json::json!({ "model": "multilingual-e5-small" })
        );
    }
}
