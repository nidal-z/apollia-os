use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode, ROUTED_COMMANDS};

const TOKEN: &str = "token-of-this-installation";

const COMMANDS: [&str; 7] = [
    "get_onboarding_state",
    "advance_onboarding_phase",
    "trigger_onboarding",
    "resume_onboarding",
    "dismiss_onboarding",
    "finalize_onboarding_chat",
    "check_onboarding_finalized",
];

fn state_json(phase: &str) -> serde_json::Value {
    serde_json::to_value(OnboardingState {
        phase: apollia_runtime::onboarding::state::OnboardingPhase::parse(phase)
            .expect("known phase"),
        ..OnboardingState::default()
    })
    .expect("state serializes")
}

fn trigger_json(mode: &str) -> serde_json::Value {
    serde_json::json!({ "session_id": format!("s-{mode}"), "mode": mode })
}

#[tokio::test]
async fn test_each_onboarding_command_reaches_its_daemon_route_with_the_token() {
    // GIVEN a daemon answering each onboarding route once, only when the
    // token of this installation comes with the request and the body is the
    // one the command must send
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    let routes = [
        ("GET", "/api/v1/onboarding", None, state_json("welcome")),
        (
            "POST",
            "/api/v1/onboarding/phase",
            Some(serde_json::json!({ "phase": "ai_setup" })),
            state_json("ai_setup"),
        ),
        (
            "POST",
            "/api/v1/onboarding/sessions",
            Some(serde_json::json!({ "topic": null })),
            trigger_json("full"),
        ),
        (
            "POST",
            "/api/v1/onboarding/resume",
            None,
            trigger_json("resume"),
        ),
        (
            "POST",
            "/api/v1/onboarding/dismiss",
            None,
            serde_json::json!({ "dismissed": true }),
        ),
        (
            "POST",
            "/api/v1/onboarding/finalize",
            None,
            serde_json::json!({ "finalized": true }),
        ),
        (
            "GET",
            "/api/v1/onboarding/finalized",
            None,
            serde_json::json!({ "finalized": true }),
        ),
    ];
    for (verb, route, body, answer) in routes {
        let mut mock = Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", auth.as_str()));
        if let Some(body) = body {
            mock = mock.and(body_json(body));
        }
        mock.respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm of each command runs
    let read = state(&client).await.unwrap();
    let advanced = advance(&client, "ai_setup").await.unwrap();
    let started = trigger(&client, None).await.unwrap();
    let resumed = resume(&client).await.unwrap();
    dismiss(&client).await.unwrap();
    finalize(&client).await.unwrap();
    let finalized = is_finalized(&client).await.unwrap();

    // THEN each reached its own route once (the mocks verify it on drop), and
    // the answers read as the onboarding screens draw them
    assert_eq!(read.phase.as_str(), "welcome");
    assert_eq!(advanced.phase.as_str(), "ai_setup");
    assert_eq!(started.mode, "full");
    assert_eq!(started.session_id, "s-full");
    assert_eq!(resumed.mode, "resume");
    assert!(finalized);
}

#[tokio::test]
async fn test_an_onboarding_refused_by_the_daemon_shows_its_own_message() {
    // GIVEN a daemon that refuses a phase out of sequence, a missing agent,
    // a user memory that did not open, and a completion it cannot write
    let server = MockServer::start().await;
    let refusals = [
        (
            "POST",
            "/api/v1/onboarding/phase",
            409,
            "cannot advance from Welcome to Done",
        ),
        (
            "POST",
            "/api/v1/onboarding/sessions",
            404,
            "onboarding-agent is not installed.",
        ),
        (
            "POST",
            "/api/v1/onboarding/resume",
            404,
            "onboarding-agent is not installed.",
        ),
        (
            "GET",
            "/api/v1/onboarding",
            503,
            "UserMemory database not initialized",
        ),
        (
            "POST",
            "/api/v1/onboarding/dismiss",
            503,
            "UserMemory database not initialized",
        ),
        (
            "POST",
            "/api/v1/onboarding/finalize",
            500,
            "cannot create memory dir: denied",
        ),
        (
            "GET",
            "/api/v1/onboarding/finalized",
            500,
            "persistence error: spawn_blocking failed",
        ),
    ];
    for (verb, route, status, message) in refusals {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({ "error": message })),
            )
            .mount(&server)
            .await;
    }
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm of each command runs against those refusals
    let advanced = advance(&client, "done").await;
    let started = trigger(&client, None).await;
    let resumed = resume(&client).await;
    let read = state(&client).await;
    let dismissed = dismiss(&client).await;
    let finalized = finalize(&client).await;
    let checked = is_finalized(&client).await;

    // THEN the interface reads the daemon's own words, not an unavailability
    assert_eq!(advanced.unwrap_err(), "cannot advance from Welcome to Done");
    assert_eq!(started.unwrap_err(), "onboarding-agent is not installed.");
    assert_eq!(resumed.unwrap_err(), "onboarding-agent is not installed.");
    assert_eq!(read.unwrap_err(), "UserMemory database not initialized");
    assert_eq!(
        dismissed.unwrap_err(),
        "UserMemory database not initialized"
    );
    assert_eq!(finalized.unwrap_err(), "cannot create memory dir: denied");
    assert_eq!(
        checked.unwrap_err(),
        "persistence error: spawn_blocking failed"
    );
}

#[test]
fn test_the_onboarding_commands_are_routed_and_pass_the_gate_attached() {
    // GIVEN the seven commands of the onboarding
    // WHEN each is gated while attached
    // THEN each is routed, none is refused
    for command in COMMANDS {
        assert!(
            ROUTED_COMMANDS.contains(&command),
            "{command} is not routed"
        );
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}
