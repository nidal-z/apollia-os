use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

fn row(id: &str, source: &str) -> serde_json::Value {
    json!({
        "id": id,
        "full_text": "bonjour",
        "language": "fr",
        "source": source,
        "audio_duration_ms": 1000,
        "processing_time_ms": 12,
        "model_name": "whisper",
        "created_at": "2026-10-03T00:00:00Z",
    })
}

#[tokio::test]
async fn test_the_history_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon keeping one transcription, answering only the token
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/transcriptions"))
        .and(query_param("limit", "50"))
        .and(header("authorization", BEARER))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "transcriptions": [row("t-1", "hotkey")] })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm lists the history
    let rows = list_on(&client, 50).await.unwrap();

    // THEN the daemon's rows come back as the embedded command returns them
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "t-1");
    assert_eq!(rows[0].source, "hotkey");
}

#[tokio::test]
async fn test_a_transcription_is_deleted_on_the_daemon_by_its_encoded_identifier() {
    // GIVEN a daemon answering the deletion with no body
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/stt/transcriptions/t%201"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm deletes an identifier that needs encoding
    let deleted = delete_on(&client, "t 1").await;

    // THEN the empty answer is a success
    assert!(deleted.is_ok());
}

#[tokio::test]
async fn test_a_file_of_this_machine_is_uploaded_and_kept_as_a_file() {
    // GIVEN a WAV on this machine, and a daemon that transcribes what it is sent
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("memo.wav");
    std::fs::write(&file, b"RIFF-content-of-the-file").unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/transcribe"))
        .and(header("authorization", BEARER))
        .and(body_string_contains("filename=\"memo.wav\""))
        .and(body_string_contains("RIFF-content-of-the-file"))
        .and(body_string_contains("name=\"source\"\r\n\r\nfile\r\n"))
        .respond_with(ResponseTemplate::new(200).set_body_json(row("t-2", "file")))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm transcribes the file
    let row = transcribe_on(&client, file.to_str().unwrap())
        .await
        .unwrap();

    // THEN the daemon's row comes back, under the file source
    assert_eq!(row.id, "t-2");
    assert_eq!(row.source, "file");
}

#[tokio::test]
async fn test_the_daemon_refusal_is_shown_as_it_words_it() {
    // GIVEN a daemon whose engine is not running
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stt/transcriptions"))
        .respond_with(ResponseTemplate::new(503).set_body_json(
            json!({ "error": "STT engine not available - stt.enabled is false or model failed to load" }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/transcribe"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({ "error": "WAV decode error: invalid WAV" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-audio.wav");
    std::fs::write(&file, b"text").unwrap();

    // WHEN the history is listed and a file is transcribed
    let listed = String::from(list_on(&client, 10).await.unwrap_err());
    let transcribed = transcribe_on(&client, file.to_str().unwrap())
        .await
        .unwrap_err();

    // THEN each refusal is the daemon's own message
    assert_eq!(
        listed,
        "STT engine not available - stt.enabled is false or model failed to load"
    );
    assert_eq!(transcribed, "WAV decode error: invalid WAV");
}

#[tokio::test]
async fn test_a_missing_file_is_refused_before_the_daemon_is_asked() {
    // GIVEN a daemon that must not be reached
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN a path that does not exist on this machine is transcribed
    let error = transcribe_on(&client, "/nowhere/memo.wav")
        .await
        .unwrap_err();

    // THEN the read failure is named, as in the embedded mode
    assert!(error.starts_with("failed to read file '/nowhere/memo.wav'"));
}

#[test]
fn test_the_family_passes_the_gate_of_the_attached_mode() {
    // GIVEN the five commands of the dictation and its history
    let family = [
        "list_transcriptions",
        "delete_transcription",
        "transcribe_file",
        "start_tour_recording",
        "stop_tour_recording",
    ];

    // WHEN the attached gate is asked about each
    // THEN none is refused as unavailable
    for command in family {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}
