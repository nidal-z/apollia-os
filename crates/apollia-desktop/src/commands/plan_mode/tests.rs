use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";

/// The seven commands of the plan mode.
const PLAN_COMMANDS: [&str; 7] = [
    "approve_plan",
    "get_chat_plan",
    "get_plan_mode_default",
    "list_plan_mutations",
    "reject_plan",
    "set_plan_mode",
    "submit_plan_decision",
];

/// A daemon answering `verb path` once, with the token, and `body` when given.
async fn daemon(
    verb: &str,
    route: &str,
    body: Option<serde_json::Value>,
    answer: ResponseTemplate,
) -> (MockServer, AttachedClient) {
    let server = MockServer::start().await;
    let mut mock = Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()));
    if let Some(body) = body {
        mock = mock.and(body_json(body));
    }
    mock.respond_with(answer).expect(1).mount(&server).await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    (server, client)
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

#[test]
fn test_the_plan_commands_pass_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN each plan command is gated
    // THEN none is refused
    for command in PLAN_COMMANDS {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}

#[tokio::test]
async fn test_the_plan_mode_default_is_read_from_the_daemon() {
    // GIVEN a daemon whose sessions start in plan mode
    let (_server, client) = daemon(
        "GET",
        "/api/v1/sessions/plan-mode-default",
        None,
        ok(json!({ "enabled": true })),
    )
    .await;

    // WHEN the attached arm reads the default
    let enabled = plan_mode_default(&client).await.unwrap();

    // THEN it is the daemon's
    assert!(enabled);
}

#[tokio::test]
async fn test_plan_mode_is_put_on_the_session_with_its_id_encoded() {
    // GIVEN a daemon accepting plan mode on a session whose id holds a slash
    let (_server, client) = daemon(
        "PUT",
        "/api/v1/sessions/s%2F1/plan-mode",
        Some(json!({ "enabled": true })),
        ok(json!({ "session_id": "s/1", "status": "plan_mode_on" })),
    )
    .await;

    // WHEN the attached arm turns plan mode on
    let outcome = put_plan_mode(&client, "s/1", true).await;

    // THEN the daemon received it, the id kept in one segment
    assert!(outcome.is_ok(), "{outcome:?}");
}

#[tokio::test]
async fn test_the_plan_and_its_history_are_read_from_the_daemon() {
    // GIVEN a daemon holding a session in discovery with no plan and no
    // mutation
    let server = MockServer::start().await;
    for (route, answer) in [
        (
            "/api/v1/sessions/s-1/plan",
            json!({ "plan": null, "phase": "discovery" }),
        ),
        ("/api/v1/sessions/s-1/plan/mutations", json!([])),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ok(answer))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads the plan and its mutations
    let snapshot: ChatPlanSnapshot = client.get(&session_path("s-1", "plan")).await.unwrap();
    let mutations: Vec<PlanMutation> = client
        .get(&session_path("s-1", "plan/mutations"))
        .await
        .unwrap();

    // THEN they read as the embedded command returns them
    assert!(snapshot.plan.is_none());
    assert_eq!(snapshot.phase, "discovery");
    assert!(mutations.is_empty());
}

#[tokio::test]
async fn test_a_plan_is_approved_and_rejected_with_its_reason_on_the_daemon() {
    // GIVEN a daemon accepting an approval and a rejection on one session
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/plan/approve"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ok(json!({ "session_id": "s-1", "status": "approved" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/plan/reject"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({ "reason": "too broad" })))
        .respond_with(ok(json!({ "session_id": "s-1", "status": "rejected" })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm rejects the plan, then approves it
    let rejected = reject(&client, "s-1", Some("too broad".to_owned())).await;
    let approved = approve(&client, "s-1").await;

    // THEN both reached the daemon
    assert!(rejected.is_ok(), "{rejected:?}");
    assert!(approved.is_ok(), "{approved:?}");
}

#[tokio::test]
async fn test_each_gate_decision_is_posted_as_the_route_reads_it() {
    // GIVEN a daemon accepting one decision of each kind on its run
    let step = json!({ "step_id": "s1", "description": "Read" });
    for (dto, body) in [
        (
            json!({ "decision": "approve" }),
            json!({ "decision": "approved", "feedback": null }),
        ),
        (
            json!({ "decision": "reject", "reason": "no" }),
            json!({ "decision": "rejected", "feedback": "no" }),
        ),
        (
            json!({ "decision": "edit", "revised_steps": [step.clone()] }),
            json!({
                "decision": "edited",
                "feedback": null,
                "revised_steps": [serde_json::to_value(
                    serde_json::from_value::<apollia_core::plan::PlanStep>(step.clone()).unwrap()
                ).unwrap()]
            }),
        ),
    ] {
        let (_server, client) = daemon(
            "POST",
            "/api/v1/tasks/run-1/plan-decision",
            Some(body),
            ok(json!({ "run_id": "run-1", "decision": "x" })),
        )
        .await;
        let decision: PlanDecisionDto = serde_json::from_value(dto.clone()).unwrap();

        // WHEN the attached arm submits it
        let outcome = decide(&client, "run-1", decision).await;

        // THEN the daemon received the body its route reads
        assert!(outcome.is_ok(), "{dto}: {outcome:?}");
    }
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_is() {
    // GIVEN a daemon refusing each of the seven plan commands with its own
    // message, as its routes answer them
    let refusals = [
        (
            "approve_plan",
            "POST",
            "/api/v1/sessions/s-1/plan/approve",
            409,
            "session s-1 is not awaiting approval (current phase: discovery)",
        ),
        (
            "get_chat_plan",
            "GET",
            "/api/v1/sessions/ghost/plan",
            404,
            "session not found: ghost",
        ),
        (
            "get_plan_mode_default",
            "GET",
            "/api/v1/sessions/plan-mode-default",
            503,
            "chat subsystem not available",
        ),
        (
            "list_plan_mutations",
            "GET",
            "/api/v1/sessions/ghost/plan/mutations",
            404,
            "session not found: ghost",
        ),
        (
            "reject_plan",
            "POST",
            "/api/v1/sessions/s-1/plan/reject",
            409,
            "session s-1 is not awaiting approval (current phase: discovery)",
        ),
        (
            "set_plan_mode",
            "PUT",
            "/api/v1/sessions/ghost/plan-mode",
            404,
            "session not found: ghost",
        ),
        (
            "submit_plan_decision",
            "POST",
            "/api/v1/tasks/run-9/plan-decision",
            404,
            "no pending plan gate for run run-9",
        ),
    ];
    let refused: Vec<&str> = refusals.iter().map(|refusal| refusal.0).collect();
    assert_eq!(refused, PLAN_COMMANDS);
    for (command, verb, route, status, message) in refusals {
        let (_server, client) = daemon(
            verb,
            route,
            None,
            ResponseTemplate::new(status).set_body_json(json!({ "error": message })),
        )
        .await;

        // WHEN the attached arm of the command calls it
        let error = match command {
            "approve_plan" => approve(&client, "s-1").await.unwrap_err(),
            "get_chat_plan" => client
                .get::<ChatPlanSnapshot>(&session_path("ghost", "plan"))
                .await
                .unwrap_err(),
            "get_plan_mode_default" => plan_mode_default(&client).await.unwrap_err(),
            "list_plan_mutations" => client
                .get::<Vec<PlanMutation>>(&session_path("ghost", "plan/mutations"))
                .await
                .unwrap_err(),
            "reject_plan" => reject(&client, "s-1", Some("too broad".to_owned()))
                .await
                .unwrap_err(),
            "set_plan_mode" => put_plan_mode(&client, "ghost", true).await.unwrap_err(),
            _ => decide(&client, "run-9", PlanDecisionDto::Approve)
                .await
                .unwrap_err(),
        };

        // THEN the interface shows the daemon's message, word for word
        assert!(
            matches!(&error, AttachedError::Refused { status: s, .. } if *s == status),
            "{command}: {error:?}"
        );
        assert_eq!(String::from(error), message, "{command}");
    }
}
