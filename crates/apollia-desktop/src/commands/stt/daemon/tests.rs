use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};
use crate::i18n::UiLocale;

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

fn config_json() -> Value {
    json!({
        "enabled": true,
        "model_path": "/models/whisper-large-v3-fr-q5_0.bin",
        "hotkey": "ctrl+shift+space",
        "clipboard_mode": "paste",
        "clipboard_restore": true,
        "silence_threshold_db": -40.0,
        "max_recording_sec": 60,
        "language": "fr",
        "trigger_mode": "toggle",
        "input_device": null
    })
}

fn refusal(message: &str) -> ResponseTemplate {
    ResponseTemplate::new(503).set_body_json(json!({ "error": message }))
}

#[test]
fn test_the_five_dictation_commands_pass_the_attached_gate() {
    // GIVEN the five dictation commands this module serves attached
    let commands = [
        "get_stt_config",
        "update_stt_config",
        "get_stt_status",
        "list_stt_models",
        "reload_stt",
    ];

    // WHEN the gate of the attached mode reads each of them
    // THEN none is refused
    for command in commands {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}

#[tokio::test]
async fn test_the_configuration_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon holding a configuration, answering only the token
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/config"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(config_json()))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads it
    let config = config_on(&client).await.unwrap();

    // THEN the window gets the daemon's row
    assert!(config.enabled);
    assert_eq!(config.model_path, "/models/whisper-large-v3-fr-q5_0.bin");
    assert_eq!(config.language.as_deref(), Some("fr"));
    assert_eq!(config.input_device, None);
}

#[tokio::test]
async fn test_an_update_is_put_on_the_daemon_then_its_engine_reloaded() {
    // GIVEN a daemon accepting the configuration and the reload
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/stt/config"))
        .and(header("authorization", BEARER))
        .and(body_json(config_json()))
        .respond_with(ResponseTemplate::new(200).set_body_json(config_json()))
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
    let config: SttConfigView = serde_json::from_value(config_json()).unwrap();

    // WHEN the attached arm saves it
    let saved = update_config_on(&client, &config).await;

    // THEN the save succeeds, and the mock counts one PUT and one reload
    assert!(saved.is_ok(), "{saved:?}");
}

#[tokio::test]
async fn test_a_refused_update_is_shown_as_the_daemon_wrote_it_and_nothing_reloads() {
    // GIVEN a daemon refusing the configuration
    let server = MockServer::start().await;
    let message = "unsupported dictation language 'xx': expected one of fr, en";
    Mock::given(method("PUT"))
        .and(path("/api/v1/stt/config"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": message })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/reload"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "loaded": true })))
        .expect(0)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let config: SttConfigView = serde_json::from_value(config_json()).unwrap();

    // WHEN the attached arm saves it
    let error = update_config_on(&client, &config).await.unwrap_err();

    // THEN the daemon's message is the one the window shows
    assert!(matches!(error, AttachedError::Refused { status: 400, .. }));
    assert_eq!(error.for_display(UiLocale::En), message);
}

#[tokio::test]
async fn test_a_failed_reload_after_a_save_is_not_an_error() {
    // GIVEN a daemon saving the configuration but failing the reload
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/stt/config"))
        .respond_with(ResponseTemplate::new(200).set_body_json(config_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/reload"))
        .respond_with(refusal("database error: locked"))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let config: SttConfigView = serde_json::from_value(config_json()).unwrap();

    // WHEN the attached arm saves it
    let saved = update_config_on(&client, &config).await;

    // THEN the save stands, as in the embedded arm
    assert!(saved.is_ok(), "{saved:?}");
}

#[tokio::test]
async fn test_a_reload_posts_to_the_daemon_and_its_refusal_is_shown_as_is() {
    // GIVEN a daemon that reloads once, then refuses
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/reload"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "loaded": false })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/reload"))
        .respond_with(refusal("STT config repository not available"))
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reloads twice
    let first = reload_on(&client).await;
    let second = reload_on(&client).await.unwrap_err();

    // THEN the first succeeds and the second shows the daemon's message
    assert!(first.is_ok(), "{first:?}");
    assert_eq!(
        second.for_display(UiLocale::En),
        "STT config repository not available"
    );
}

#[tokio::test]
async fn test_the_status_is_the_daemons_and_its_refusal_is_shown_as_is() {
    // GIVEN a daemon answering its status once, then refusing
    let server = MockServer::start().await;
    let status = json!({
        "enabled": true,
        "model_loaded": true,
        "model_path": "/models/ggml-base.bin",
        "model_name": "ggml-base",
        "backend_name": "whisper-cpp",
        "metal_enabled": true,
        "cuda_enabled": false
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/status"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(status.clone()))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/status"))
        .respond_with(refusal("STT engine actor has stopped"))
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads it twice
    let first = status_on(&client).await.unwrap();
    let second = status_on(&client).await.unwrap_err();

    // THEN the first is the daemon's status, the second its message
    assert_eq!(first, status);
    assert_eq!(
        second.for_display(UiLocale::En),
        "STT engine actor has stopped"
    );
}

#[tokio::test]
async fn test_the_models_are_the_daemons_described_as_the_embedded_arm_does() {
    // GIVEN a daemon listing a French model of 2 MiB and a generic one
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/models"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [
                { "name": "whisper-large-v3-fr-q5_0.bin", "path": "/m/whisper-large-v3-fr-q5_0.bin", "size_bytes": 2_097_152 },
                { "name": "ggml-small.gguf", "path": "/m/ggml-small.gguf", "size_bytes": 0 }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm lists them
    let models = models_on(&client).await.unwrap();

    // THEN each comes with its size in megabytes and the language its name
    // carries
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].name, "whisper-large-v3-fr-q5_0.bin");
    assert!((models[0].size_mb - 2.0).abs() < f64::EPSILON);
    assert_eq!(models[0].language.as_deref(), Some("fr"));
    assert_eq!(models[1].path, "/m/ggml-small.gguf");
    assert_eq!(models[1].language, None);
}

#[tokio::test]
async fn test_a_refused_model_list_is_shown_as_the_daemon_wrote_it() {
    // GIVEN a daemon that cannot read its models folder
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/models"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(json!({ "error": "failed to read models directory: denied" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm lists them
    let error = models_on(&client).await.unwrap_err();

    // THEN the daemon's message is the one the window shows
    assert_eq!(
        error.for_display(UiLocale::En),
        "failed to read models directory: denied"
    );
}
