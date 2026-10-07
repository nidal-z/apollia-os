use apollia_core::model_preset::{ModelSource, PresetDraft, RequestSettings};
use axum::http::StatusCode;
use tempfile::TempDir;

use super::engine_template;
use crate::api::routes_chat::session_reads::tests::{
    new_free_chat, state_with_chat, with_default_qwen3,
};
use crate::api::server::AppState;
use crate::api::test_support::{call, MockBackend};
use crate::llama_server::LlamaServerSupervisor;

/// A loopback listener answering `GET /props` with `props`, as the engine
/// does, and the port it listens on.
async fn engine_answering(props: serde_json::Value) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    // Any other path is unknown to this stand-in, as it is to the engine.
    let app = axum::Router::new().fallback(move |uri: axum::http::Uri| {
        let props = props.clone();
        async move {
            if uri.path() == "/props" {
                Ok(axum::Json(props))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
    });
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

/// The path of the model the default row of [`with_default_qwen3`] serves.
fn qwen3_path(dir: &TempDir) -> String {
    crate::installed_models::models_dir(dir.path())
        .join("qwen3.gguf")
        .display()
        .to_string()
}

/// A preset of the default model whose load settings are `load`.
fn preset_with(state: &AppState<MockBackend>, load: serde_json::Value) -> i64 {
    let repo = state.llm_backend_repo.as_ref().expect("repository");
    let repo = repo.lock().expect("lock");
    repo.create_preset(
        ModelSource::Local,
        "local-qwen",
        &PresetDraft {
            name: "Tuned".into(),
            load: load.as_object().expect("object").clone(),
            request: RequestSettings::default(),
            is_model_default: false,
        },
    )
    .expect("preset")
    .id
}

async fn read(state: AppState<MockBackend>, id: &str) -> (StatusCode, serde_json::Value) {
    call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/prompt-template"),
        None,
    )
    .await
}

#[tokio::test]
async fn test_the_template_of_the_chosen_preset_is_shown_before_the_engine_is_asked() {
    // GIVEN a conversation whose preset carries a chat template, on a model
    // loaded with another one
    let dir = TempDir::new().unwrap();
    let mut state = with_default_qwen3(state_with_chat(&dir), &dir);
    let port = engine_answering(serde_json::json!({ "chat_template": "{{ from_model }}" })).await;
    state.llama_server_supervisor =
        Some(LlamaServerSupervisor::with_stand_in_on_port(&qwen3_path(&dir), port).await);
    let id = new_free_chat(&state).await;
    let preset = preset_with(
        &state,
        serde_json::json!({ "chat_template": "{{ from_preset }}" }),
    );
    let (status, stored) = call(
        state.clone(),
        "PUT",
        &format!("/api/v1/sessions/{id}/settings"),
        Some(serde_json::json!({ "preset": preset })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stored}");

    // WHEN the prompt template of the conversation is read
    let (status, body) = read(state, &id).await;

    // THEN it is the text of the preset, named as such
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "preset");
    assert_eq!(body["template"], "{{ from_preset }}");
}

#[tokio::test]
async fn test_without_a_preset_template_the_loaded_engine_answers_its_own() {
    // GIVEN a conversation on the default model, loaded, whose preset fixes
    // a context window and no template
    let dir = TempDir::new().unwrap();
    let mut state = with_default_qwen3(state_with_chat(&dir), &dir);
    let port = engine_answering(serde_json::json!({
        "chat_template": "{%- if tools %}<|im_start|>system{%- endif %}",
        "total_slots": 1
    }))
    .await;
    state.llama_server_supervisor =
        Some(LlamaServerSupervisor::with_stand_in_on_port(&qwen3_path(&dir), port).await);
    let id = new_free_chat(&state).await;
    let preset = preset_with(&state, serde_json::json!({ "n_ctx": 8192 }));
    let (status, stored) = call(
        state.clone(),
        "PUT",
        &format!("/api/v1/sessions/{id}/settings"),
        Some(serde_json::json!({ "preset": preset })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stored}");

    // WHEN the prompt template of the conversation is read
    let (status, body) = read(state, &id).await;

    // THEN it is the text the engine reports for the process serving the model
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "engine");
    assert_eq!(
        body["template"],
        "{%- if tools %}<|im_start|>system{%- endif %}"
    );
}

#[tokio::test]
async fn test_a_model_that_is_not_loaded_has_no_template_to_show() {
    // GIVEN a conversation on the default model, with an engine holding
    // another model only
    let dir = TempDir::new().unwrap();
    let mut state = with_default_qwen3(state_with_chat(&dir), &dir);
    let port = engine_answering(serde_json::json!({ "chat_template": "{{ other }}" })).await;
    state.llama_server_supervisor =
        Some(LlamaServerSupervisor::with_stand_in_on_port("/models/other.gguf", port).await);
    let id = new_free_chat(&state).await;

    // WHEN the prompt template of the conversation is read
    let (status, body) = read(state.clone(), &id).await;

    // THEN nothing is shown, and the model was not loaded to answer
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "unavailable");
    assert!(body["template"].is_null(), "{body}");
    let resident = state
        .llama_server_supervisor
        .as_ref()
        .unwrap()
        .loaded_models()
        .await;
    assert_eq!(resident.len(), 1);
    assert_eq!(resident[0].model_path, "/models/other.gguf");
}

#[tokio::test]
async fn test_an_engine_that_leaves_the_field_out_has_no_template_to_show() {
    // GIVEN a conversation on a loaded model whose engine reports its
    // properties without a chat template
    let dir = TempDir::new().unwrap();
    let mut state = with_default_qwen3(state_with_chat(&dir), &dir);
    let port = engine_answering(serde_json::json!({ "total_slots": 1 })).await;
    state.llama_server_supervisor =
        Some(LlamaServerSupervisor::with_stand_in_on_port(&qwen3_path(&dir), port).await);
    let id = new_free_chat(&state).await;

    // WHEN the prompt template of the conversation is read
    let (status, body) = read(state, &id).await;

    // THEN nothing is shown
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "unavailable");
    assert!(body["template"].is_null(), "{body}");
}

#[tokio::test]
async fn test_a_host_without_a_local_engine_has_no_template_to_show() {
    // GIVEN a conversation on a host where no llama-server supervisor runs,
    // as when the model is served by another engine
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);
    let id = new_free_chat(&state).await;

    // WHEN the prompt template of the conversation is read
    let (status, body) = read(state, &id).await;

    // THEN nothing is shown
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "unavailable");
}

#[tokio::test]
async fn test_the_template_of_an_unknown_session_is_not_found() {
    // GIVEN a daemon with a chat subsystem and no such session
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);

    // WHEN the prompt template of an unknown session is read
    let (status, body) = read(state, "no-such-session").await;

    // THEN the session is reported as not found
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn test_a_blank_template_and_a_silent_port_read_as_none() {
    // GIVEN an engine reporting a blank template, and a port nothing listens on
    let blank = engine_answering(serde_json::json!({ "chat_template": "  \n" })).await;
    let silent = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    // WHEN each is asked for its template
    let from_blank = engine_template(blank).await;
    let from_silent = engine_template(silent).await;

    // THEN neither has one to show
    assert_eq!(from_blank, None);
    assert_eq!(from_silent, None);
}
