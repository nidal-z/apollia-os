use apollia_core::events::RuntimeEvent;
use apollia_runtime::chat::{ChatMode, SessionInfo, SessionStatus};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::relay::{decode, liveness_signal, Frame, Liveness, ReadStop, SseReader};
use super::*;
use crate::i18n::UiLocale;

const TOKEN: &str = "token-of-this-installation";

fn session(id: &str) -> SessionInfo {
    SessionInfo {
        id: id.to_owned(),
        mode: ChatMode::Libre,
        agent_name: None,
        status: SessionStatus::Active,
        created_at: "2026-10-01T00:00:00Z".to_owned(),
        title: Some("Une conversation".to_owned()),
        project_id: None,
    }
}

// ── The gate ─────────────────────────────────────────────────────────────

#[test]
fn test_a_command_not_routed_is_refused_in_attached_mode_with_a_readable_error() {
    // GIVEN the application attached to a daemon, and a command whose body
    // reads state only the embedded runtime builds
    let command = "list_agents";

    // WHEN the gate is asked before the body runs
    let refusal = gate(LinkMode::Attached, command);

    // THEN it is refused with the typed error, which names the command and
    // the reason in both languages, and nothing else is attempted
    let error = refusal.expect_err("a command not routed must be refused");
    assert!(matches!(&error, AttachedError::NotRouted { command: c } if c == command));
    let fr = error.for_display(UiLocale::Fr);
    let en = error.for_display(UiLocale::En);
    assert!(
        fr.contains("list_agents") && fr.contains("en service"),
        "{fr}"
    );
    assert!(
        en.contains("list_agents") && en.contains("as a service"),
        "{en}"
    );
}

#[test]
fn test_every_routed_command_passes_the_gate_in_attached_mode() {
    // GIVEN the routed commands
    // WHEN each is gated while attached
    let refused: Vec<&&str> = ROUTED_COMMANDS
        .iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused, and the chat surface of the story is among them
    assert!(refused.is_empty(), "refused: {refused:?}");
    for command in [
        "send_chat_message",
        "pause_chat_session",
        "list_chat_sessions",
        "start_variant_download",
    ] {
        assert!(
            ROUTED_COMMANDS.contains(&command),
            "{command} is not routed"
        );
    }
}

#[test]
fn test_the_embedded_mode_lets_every_command_through() {
    // GIVEN the runtime running in this process
    // WHEN a command not routed is gated
    let verdict = gate(LinkMode::Embedded, "list_agents");

    // THEN it runs as it always did
    assert!(verdict.is_ok());
}

// ── The HTTP arm ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_sessions_are_listed_over_http_with_the_token() {
    // GIVEN a daemon holding one active session, answering only the token
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions"))
        .and(query_param("status", "active"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![session("s-1")]))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm lists the active sessions
    let sessions = client.list_sessions(Some("active")).await.unwrap();

    // THEN the daemon's session comes back
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "s-1");
}

#[tokio::test]
async fn test_a_message_is_sent_and_a_turn_paused_over_http() {
    // GIVEN a daemon accepting a message and a pause on one session
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/messages"))
        .and(body_json(serde_json::json!({ "content": "Bonjour" })))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(serde_json::json!({ "message_id": "m-9", "status": "processing" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/pause"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "paused" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN a message is sent, then the turn paused
    let message_id = client.send_message("s-1", "Bonjour").await.unwrap();
    client.pause_session("s-1").await.unwrap();

    // THEN the daemon's message id is returned and both calls reached it
    assert_eq!(message_id, "m-9");
}

#[tokio::test]
async fn test_a_session_is_created_over_http_from_the_interface_request() {
    // GIVEN a daemon creating sessions
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(session("s-new")))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let request = crate::commands::chat::CreateSessionRequest {
        mode: "libre".to_owned(),
        agent_name: None,
        system_prompt: None,
        tools: Vec::new(),
        project_id: None,
    };

    // WHEN the attached arm creates a free conversation
    let info = client.create_session(&request).await.unwrap();

    // THEN the daemon's session is returned
    assert_eq!(info.id, "s-new");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_carries_its_own_message() {
    // GIVEN a daemon that does not know the session
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/unknown"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({ "error": "session not found" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the session is read
    let error = client.get_session("unknown").await.unwrap_err();

    // THEN the error is typed and shows the daemon's words, which the
    // interface already classifies as a missing session
    assert!(matches!(error, AttachedError::Refused { status: 404, .. }));
    assert_eq!(error.for_display(UiLocale::En), "session not found");
}

#[tokio::test]
async fn test_a_stopped_daemon_is_an_error_not_a_hang() {
    // GIVEN a port nothing listens on any more
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = AttachedClient::new(format!("http://127.0.0.1:{port}"), TOKEN).unwrap();

    // WHEN a routed command and the liveness probe reach for it
    let error = client.list_sessions(None).await.unwrap_err();
    let alive = client.is_alive().await;

    // THEN both answer: an unreachable runtime, and not alive
    assert!(matches!(error, AttachedError::Unreachable(_)), "{error:?}");
    assert!(!alive);
}

// ── The relay ────────────────────────────────────────────────────────────

#[test]
fn test_frames_split_across_chunks_are_reassembled_and_keep_alives_skipped() {
    // GIVEN the wire form of two runtime events, a keep-alive between them,
    // cut in the middle of the first frame
    let event = serde_json::to_string(&RuntimeEvent::ShutdownRequested).unwrap();
    let wire =
        format!("event: runtime\ndata: {event}\n\n:\n\nevent: runtime\r\ndata: {event}\r\n\r\n");
    let (head, tail) = wire.as_bytes().split_at(10);
    let mut reader = SseReader::default();

    // WHEN the two chunks are fed in order
    let mut frames = reader.push(head).unwrap();
    frames.extend(reader.push(tail).unwrap());

    // THEN two frames come out, each decoding back into the event
    assert_eq!(frames.len(), 2, "{frames:?}");
    for frame in &frames {
        assert!(matches!(
            decode(frame),
            Some(RuntimeEvent::ShutdownRequested)
        ));
    }
}

#[test]
fn test_a_lag_report_is_logged_and_not_relayed() {
    // GIVEN the frame the runtime sends when this subscriber fell behind
    let frame = Frame {
        event: "lagged".to_owned(),
        data: "{\"skipped\":3}".to_owned(),
    };

    // WHEN it is decoded
    // THEN no runtime event is invented from it
    assert!(decode(&frame).is_none());
}

#[test]
fn test_a_frame_that_never_ends_stops_the_stream() {
    // GIVEN a body that keeps sending data without ever closing its frame
    let mut reader = SseReader::default();
    let chunk = vec![b'x'; 1024 * 1024];

    // WHEN more than the bound is fed
    let mut stop = None;
    for _ in 0..9 {
        if let Err(error) = reader.push(&chunk) {
            stop = Some(error);
            break;
        }
    }

    // THEN the reader gives up instead of growing without bound
    assert_eq!(stop, Some(ReadStop::Oversized));
}

#[test]
fn test_liveness_emits_a_heartbeat_on_answer_and_names_the_loss_once() {
    // GIVEN the four transitions of two successive probes
    // WHEN the signal of each is computed
    // THEN an answer is a heartbeat, the first silence is a loss, and the
    // silences after it say nothing more
    assert_eq!(liveness_signal(true, true), Some(Liveness::Heartbeat));
    assert_eq!(liveness_signal(false, true), Some(Liveness::Heartbeat));
    assert_eq!(liveness_signal(true, false), Some(Liveness::Lost));
    assert_eq!(liveness_signal(false, false), None);
}
