use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};
use crate::commands::config::system::setup_local_llm_on;

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

const FAMILY: [&str; 5] = [
    "get_ai_setup_info",
    "scan_for_gguf_models",
    "scan_for_whisper_models",
    "setup_local_llm",
    "setup_whisper_model",
];

#[test]
fn test_the_local_ai_installation_passes_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN each command of the local AI installation is gated
    let refused: Vec<&str> = FAMILY
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused with "unavailable while Apollia runs as a service"
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[tokio::test]
async fn test_the_daemon_machine_and_its_models_are_read_with_the_token() {
    // GIVEN a daemon describing its machine and the models on its disk, and
    // answering only the token
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/local-ai/system"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_ram_gb": 32.0, "available_ram_gb": 20.0, "os": "macos",
            "arch": "aarch64", "gpu_available": true, "gpu_vram_gb": 32.0
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/local-ai/models/gguf"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "path": "/d/models/a.gguf", "filename": "a.gguf", "size_bytes": 4096,
            "size_human": "4.0 KB", "recommended": true
        }])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/local-ai/models/whisper"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "path": "/d/models/ggml-base.bin", "filename": "ggml-base.bin",
            "size_bytes": 1024, "model_size": "base", "recommended": true
        }])))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads the machine, then scans for both kinds
    let info = system_info_on(&client).await.unwrap();
    let gguf = gguf_models_on(&client).await.unwrap();
    let whisper = whisper_models_on(&client).await.unwrap();

    // THEN the daemon's answers come back as the onboarding draws them
    assert_eq!(info.total_ram_gb, 32.0);
    assert_eq!(info.gpu_vram_gb, Some(32.0));
    assert_eq!(gguf.len(), 1);
    assert_eq!(gguf[0].path, "/d/models/a.gguf");
    assert!(gguf[0].recommended);
    assert_eq!(whisper.len(), 1);
    assert_eq!(whisper[0].model_size, "base");
}

#[tokio::test]
async fn test_a_gguf_file_is_set_up_on_the_daemon_with_the_token() {
    // GIVEN a daemon that copies and registers a model
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/local-ai/llm"))
        .and(header("authorization", BEARER))
        .and(body_json(
            json!({ "ggufPath": "/Users/u/Downloads/a-Q8_0.gguf", "contextWindow": 16384 }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model_path": "/Users/u/.apollia/models/a-Q8_0.gguf", "quantization": "q8_0"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm sets the model up with a window
    let done = setup_local_llm_on(
        &client,
        "/Users/u/Downloads/a-Q8_0.gguf".to_owned(),
        Some(16_384),
    )
    .await
    .unwrap();

    // THEN the daemon's models folder is where the model now lives
    assert_eq!(done.model_path, "/Users/u/.apollia/models/a-Q8_0.gguf");
    assert_eq!(done.quantization, "q8_0");
}

#[tokio::test]
async fn test_a_dictation_model_is_stored_then_loaded_on_the_daemon() {
    // GIVEN a daemon that stores a dictation model and reloads its engine
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/local-ai/dictation"))
        .and(header("authorization", BEARER))
        .and(body_json(
            json!({ "modelPath": "/d/models/ggml-base.bin", "language": "fr" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model_path": "/d/models/ggml-base.bin", "language": "fr"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/reload"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "loaded": true })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm sets the model up in French
    let done = setup_dictation_on(
        &client,
        "/d/models/ggml-base.bin".to_owned(),
        Some("fr".to_owned()),
    )
    .await
    .unwrap();

    // THEN the stored model comes back, and the engine was asked to load it
    assert_eq!(done.model_path, "/d/models/ggml-base.bin");
    assert_eq!(done.language.as_deref(), Some("fr"));
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_words_it() {
    // GIVEN a daemon that refuses a file that is not there, a dictation model
    // while its onboarding store is closed, and a scan
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/local-ai/llm"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "file not found: /x.gguf" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/local-ai/dictation"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(json!({ "error": "UserMemory database not initialized" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/local-ai/models/gguf"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({ "error": "GGUF scan failed: x" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm calls each
    let llm = setup_local_llm_on(&client, "/x.gguf".to_owned(), None)
        .await
        .unwrap_err();
    let dictation = setup_dictation_on(&client, "/m/ggml-base.bin".to_owned(), None)
        .await
        .unwrap_err();
    let scan = gguf_models_on(&client).await.unwrap_err();

    // THEN the interface shows the daemon's own words
    assert_eq!(String::from(llm), "file not found: /x.gguf");
    assert_eq!(
        String::from(dictation),
        "UserMemory database not initialized"
    );
    assert_eq!(String::from(scan), "GGUF scan failed: x");
}
