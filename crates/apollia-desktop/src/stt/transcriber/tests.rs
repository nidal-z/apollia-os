use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

#[test]
fn test_a_dictation_is_encoded_as_the_wav_the_daemon_decodes() {
    // GIVEN half a second of prepared audio
    let audio: Vec<f32> = (0..8_000).map(|i| (i as f32 / 8_000.0) - 0.5).collect();

    // WHEN it is encoded for the daemon
    let wav = encode_wav(&audio).unwrap();

    // THEN it reads back as the same mono 16 kHz samples
    let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().sample_rate, 16_000);
    let back: Vec<f32> = reader.into_samples::<f32>().map(Result::unwrap).collect();
    assert_eq!(back, audio);
}

#[tokio::test]
async fn test_a_dictation_goes_to_the_daemon_as_a_dictation() {
    // GIVEN a daemon that transcribes what it is sent, answering only the token
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stt/transcribe"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "t-1",
            "full_text": "bonjour",
            "source": "hotkey",
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN a dictation captured by this window is transcribed
    let outcome = Transcriber::Daemon(client)
        .transcribe(vec![0.1; 1_600])
        .await;

    // THEN the daemon's text comes back, the audio kept as a dictation
    assert_eq!(outcome, Transcribed::Text("bonjour".to_owned()));
    let sent = &server.received_requests().await.unwrap()[0].body;
    let marker = b"name=\"source\"\r\n\r\nhotkey\r\n";
    assert!(sent.windows(marker.len()).any(|w| w == marker));
}

#[tokio::test]
async fn test_a_daemon_without_an_engine_reads_as_a_missing_model() {
    // GIVEN a daemon whose engine is not running, and one that fails
    let idle = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({ "error": "off" })))
        .mount(&idle)
        .await;
    let broken = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "error": "boom" })))
        .mount(&broken)
        .await;

    // WHEN a dictation is sent to each
    let idle_outcome = Transcriber::Daemon(AttachedClient::new(idle.uri(), TOKEN).unwrap())
        .transcribe(vec![0.1; 1_600])
        .await;
    let broken_outcome = Transcriber::Daemon(AttachedClient::new(broken.uri(), TOKEN).unwrap())
        .transcribe(vec![0.1; 1_600])
        .await;

    // THEN the window tells a missing model from a failure, as it does embedded
    assert_eq!(idle_outcome, Transcribed::NoModel);
    assert_eq!(broken_outcome, Transcribed::Failed);
}

#[tokio::test]
async fn test_a_runtime_in_this_process_without_an_engine_reads_as_a_missing_model() {
    // GIVEN the engine cell of a runtime in this process, empty
    let cell = apollia_runtime::api::server::shared_stt_engine_from(None);

    // WHEN a dictation is transcribed
    let outcome = Transcriber::Local(cell).transcribe(vec![0.1; 1_600]).await;

    // THEN it is the missing model the flow notifies
    assert_eq!(outcome, Transcribed::NoModel);
}
