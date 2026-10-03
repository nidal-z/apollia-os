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
    let command = "get_delegation_tree";

    // WHEN the gate is asked before the body runs
    let refusal = gate(LinkMode::Attached, command);

    // THEN it is refused with the typed error, which names the command and
    // the reason in both languages, and nothing else is attempted
    let error = refusal.expect_err("a command not routed must be refused");
    assert!(matches!(&error, AttachedError::NotRouted { command: c } if c == command));
    let fr = error.for_display(UiLocale::Fr);
    let en = error.for_display(UiLocale::En);
    assert!(
        fr.contains("get_delegation_tree") && fr.contains("en service"),
        "{fr}"
    );
    assert!(
        en.contains("get_delegation_tree") && en.contains("as a service"),
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

    // THEN none is refused, and the chat, agents, tasks, approvals and
    // questions to the user are among them
    assert!(refused.is_empty(), "refused: {refused:?}");
    for command in [
        "send_chat_message",
        "pause_chat_session",
        "list_chat_sessions",
        "list_agents",
        "install_agent",
        "list_tasks",
        "get_task_timeline",
        "cancel_task",
        "list_pending_approvals",
        "resume_task",
        "list_pending_user_inputs",
        "respond_user_input",
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
    let verdict = gate(LinkMode::Embedded, "get_delegation_tree");

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

// ── Agents, tasks, approvals and questions to the user ──────────────────

#[tokio::test]
async fn test_the_daemon_agent_view_reads_as_the_list_the_window_draws() {
    // GIVEN a daemon answering one installed agent, not started
    let server = MockServer::start().await;
    let row = serde_json::json!([{
        "id": null, "name": "veille", "version": "1.0.0", "enabled": true,
        "runtime_status": null, "installed_at": "2026-10-02T00:00:00Z",
        "description": null, "tags": [], "tools_required": [], "tools_optional": [],
        "execution_mode": "auto", "install_path": "/a/agent.py", "supports_a2a": false,
        "skills": [], "agent_type": null, "examples": [], "limitations": [],
        "setup_notes": null, "agent_class": null, "memory_namespace": null,
        "shared_memory_namespaces": []
    }]);
    Mock::given(method("GET"))
        .and(path("/api/v1/agents/installed"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(row))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads it as the command's own type
    let agents: Vec<crate::commands::agents::AgentListItem> =
        client.get("/api/v1/agents/installed").await.unwrap();

    // THEN the agent comes back installed and stopped
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].name, "veille");
    assert!(agents[0].runtime_status.is_none());
}

#[tokio::test]
async fn test_a_task_already_gone_reads_as_a_refusal_with_its_status() {
    // GIVEN a daemon that no longer knows the task
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/tasks/t-1"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({ "error": "not found" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm cancels it
    let result = client
        .delete::<serde_json::Value>("/api/v1/tasks/t-1")
        .await;

    // THEN the status survives, which is what turns it into `false`
    assert!(matches!(
        result,
        Err(AttachedError::Refused { status: 404, .. })
    ));
}

#[tokio::test]
async fn test_a_daemon_pending_approval_reads_as_the_inbox_entry() {
    // GIVEN a daemon answering one pending approval with the fields only it
    // carries, the skill and the typed payload
    let server = MockServer::start().await;
    let pending = serde_json::json!([{
        "task_id": "t-7", "agent_name": "veille", "skill_id": "digest",
        "prompt": "Envoyer ?", "context": {}, "payload": null,
        "suspended_at": "2026-10-02T00:00:00Z"
    }]);
    Mock::given(method("GET"))
        .and(path("/api/v1/approvals/pending"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pending))
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads it as the command's own type
    let approvals: Vec<crate::commands::hitl::PendingApproval> =
        client.get("/api/v1/approvals/pending").await.unwrap();

    // THEN the entry the inbox draws comes back
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].task_id, "t-7");
    assert_eq!(approvals[0].prompt, "Envoyer ?");
}

#[tokio::test]
async fn test_an_answer_to_a_question_is_posted_to_the_daemon() {
    // GIVEN a daemon waiting for the answer to one question
    let server = MockServer::start().await;
    let body = serde_json::json!({ "answers": [{ "id": "q1", "value": "oui", "skipped": false }] });
    Mock::given(method("POST"))
        .and(path("/api/v1/user-inputs/r%201/answer"))
        .and(body_json(body.clone()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "answered" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm answers it, its id encoded as a path segment
    let path = format!("/api/v1/user-inputs/{}/answer", segment("r 1"));
    let answered: serde_json::Value = client.post(&path, &body).await.unwrap();

    // THEN the daemon received the answers
    assert_eq!(answered["status"], "answered");
}

// ── Commands served by one runtime route ─────────────────────────────────

#[tokio::test]
async fn test_a_route_command_reaches_the_daemon_with_each_verb_and_the_token() {
    // GIVEN a daemon that answers each verb of a route-backed command, only
    // when the token of this installation comes with it
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    for (verb, route) in [
        ("GET", "/api/v1/mcp/servers"),
        ("POST", "/api/v1/triggers"),
        ("PUT", "/api/v1/notifications/channels/c-1"),
        ("PATCH", "/api/v1/mcp/servers/fs/approval"),
        ("DELETE", "/api/v1/llm/backends/local"),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", auth.as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "verb": verb })),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());
    let body = serde_json::json!({ "name": "x" });

    // WHEN the attached arm sends each of them
    let answers = [
        link.get_json("/api/v1/mcp/servers").await.unwrap(),
        link.post_json("/api/v1/triggers", &body).await.unwrap(),
        link.put_json("/api/v1/notifications/channels/c-1", &body)
            .await
            .unwrap(),
        link.patch_json("/api/v1/mcp/servers/fs/approval", &body)
            .await
            .unwrap(),
        link.delete_json("/api/v1/llm/backends/local")
            .await
            .unwrap(),
    ];

    // THEN each reached the daemon with its own verb and came back as JSON
    let verbs: Vec<&str> = answers.iter().filter_map(|a| a["verb"].as_str()).collect();
    assert_eq!(verbs, ["GET", "POST", "PUT", "PATCH", "DELETE"]);
}

#[tokio::test]
async fn test_a_route_command_refused_by_the_daemon_shows_its_message() {
    // GIVEN a daemon that refuses to remove an MCP server it does not know
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/mcp/servers/ghost"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({ "error": "unknown server ghost" })),
        )
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the attached arm asks for the removal
    let error = link
        .delete_json("/api/v1/mcp/servers/ghost")
        .await
        .unwrap_err();

    // THEN the interface reads the daemon's own words, not an unavailability
    assert_eq!(error, "unknown server ghost");
}

#[test]
fn test_models_mcp_connections_and_notifications_pass_the_gate_attached() {
    // GIVEN the screens of models, MCP servers, connections, notifications
    // and triggers, through the command each one opens with
    let screens = [
        "update_llm_backend",
        "list_loaded_models",
        "list_mcp_servers",
        "add_mcp_server",
        "list_connections",
        "list_notification_channels",
        "list_triggers",
        "list_tools",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = screens
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[test]
fn test_a_window_command_runs_attached_and_carries_its_reason() {
    // GIVEN the commands of the window, which read no state of the runtime
    // WHEN each is gated while attached
    let refused: Vec<&str> = WINDOW_COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| gate(LinkMode::Attached, name).is_err())
        .collect();

    // THEN none is refused, each says why it needs no route, and none is
    // also on the routed list
    assert!(refused.is_empty(), "refused: {refused:?}");
    for (name, reason) in WINDOW_COMMANDS {
        assert!(!reason.trim().is_empty(), "{name} gives no reason");
        assert!(!ROUTED_COMMANDS.contains(name), "{name} is listed twice");
    }
    assert!(gate(LinkMode::Attached, "quit_app").is_ok());
}

// ── Permissions ──────────────────────────────────────────────────────────

#[test]
fn test_the_proposed_rules_pass_the_gate_attached() {
    // GIVEN the commands of the proposed rules
    let commands = [
        "list_proposed_permission_rules",
        "apply_proposed_permission_rule",
        "dismiss_proposed_permission_rule",
        "add_permission_prefix_rule",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused, and none is a window exception
    assert!(refused.is_empty(), "refused: {refused:?}");
    assert!(WINDOW_COMMANDS
        .iter()
        .all(|(name, _)| !commands.contains(name)));
}

// ── The profile ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_the_daemon_profile_reads_as_the_page_draws_it_and_an_entry_is_put_by_key() {
    // GIVEN a daemon holding one profile entry, and accepting a write to a
    // dotted key
    let server = MockServer::start().await;
    let entry = serde_json::json!({
        "key": "tools.daily", "value": "tableur", "written_by": "user",
        "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
        "in_schema": true,
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "schema_entries": [entry.clone()], "extras": [], "entries": [entry.clone()],
            "last_updated_at": "2026-10-02T00:00:00Z",
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/profile/entries/tools.daily"))
        .and(body_json(serde_json::json!({ "value": "tableur" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(entry))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads the profile and writes the entry
    let profile: apollia_runtime::profile::UserProfileView =
        client.get("/api/v1/profile").await.unwrap();
    let body = apollia_runtime::api::routes_profile::SetProfileEntryBody {
        value: "tableur".to_owned(),
    };
    let stored: apollia_runtime::profile::ProfileEntryView = client
        .put(
            &format!("/api/v1/profile/entries/{}", segment("tools.daily")),
            &body,
        )
        .await
        .unwrap();

    // THEN both read as the view the profile page draws
    assert_eq!(profile.schema_entries.len(), 1);
    assert_eq!(profile.entries[0].key, "tools.daily");
    assert_eq!(stored.value, "tableur");
    assert!(gate(LinkMode::Attached, "set_profile_entry").is_ok());
}

// ── The conversations ────────────────────────────────────────────────────

#[tokio::test]
async fn test_each_conversation_command_reaches_its_daemon_route_with_the_token() {
    use crate::commands::chat::conversations;

    // GIVEN a daemon answering each route of the conversations, only when the
    // token of this installation comes with the request and the body is the
    // one the command must send
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    let ok = || ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "ok" }));
    let routes = [
        (
            "PATCH",
            "/api/v1/sessions/s%201",
            serde_json::json!({ "system_prompt": "Brief.", "tools": null, "llm_backend": null }),
        ),
        (
            "POST",
            "/api/v1/sessions/s%201/erase",
            serde_json::json!({}),
        ),
        (
            "PUT",
            "/api/v1/sessions/s%201/title",
            serde_json::json!({ "title": "Plan" }),
        ),
        (
            "PUT",
            "/api/v1/sessions/s%201/project",
            serde_json::json!({ "project_id": "p-1" }),
        ),
    ];
    for (verb, route, body) in routes {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", auth.as_str()))
            .and(body_json(body))
            .respond_with(ok())
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s%201/title/generate"))
        .and(header("authorization", auth.as_str()))
        .and(body_json(serde_json::json!({ "first_message": "Help me" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "title": "CV help" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p%2F1/sessions"))
        .and(header("authorization", auth.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![session("s-2")]))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let update = crate::commands::chat::UpdateSessionRequest {
        system_prompt: Some("Brief.".to_owned()),
        tools: None,
        llm_backend: None,
    };

    // WHEN the attached arm of each command runs, with identifiers that need
    // encoding in a path
    conversations::update(&client, "s 1", &update)
        .await
        .unwrap();
    conversations::erase(&client, "s 1").await.unwrap();
    conversations::rename(&client, "s 1", "Plan").await.unwrap();
    conversations::link_project(&client, "s 1", Some("p-1"))
        .await
        .unwrap();
    let title = conversations::name(&client, "s 1", "Help me")
        .await
        .unwrap();
    let sessions = conversations::project_sessions(&client, "p/1")
        .await
        .unwrap();

    // THEN each reached its own route once (the mocks verify it on drop), the
    // generated title and the project's sessions come back as the daemon said
    assert_eq!(title, "CV help");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "s-2");
}

#[tokio::test]
async fn test_a_conversation_refused_by_the_daemon_shows_its_message() {
    use crate::commands::chat::conversations;

    // GIVEN a daemon that refuses a blank title and an unknown session
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/sessions/s-1/title"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(serde_json::json!({ "error": "title must not be empty" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/ghost/erase"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({ "error": "session not found: ghost" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm renames with a blank title and erases the unknown
    let renamed = conversations::rename(&client, "s-1", "  ").await;
    let erased = conversations::erase(&client, "ghost").await;

    // THEN the interface reads the daemon's own words, not an unavailability
    assert_eq!(renamed.unwrap_err(), "title must not be empty");
    assert_eq!(erased.unwrap_err(), "session not found: ghost");
}

#[test]
fn test_the_conversation_commands_pass_the_gate_attached() {
    // GIVEN the commands that manage the conversations of the chat
    let commands = [
        "delete_chat_session",
        "generate_chat_session_name",
        "link_chat_to_project",
        "list_chats_by_project",
        "rename_chat_session",
        "update_chat_session",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused
    assert!(refused.is_empty(), "refused: {refused:?}");
}

// ── The conversation: metrics, export, reveal ────────────────────────────

#[tokio::test]
async fn test_the_metrics_of_a_conversation_are_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon answering the metrics of one session, only to the token
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s%201/metrics"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "session_id": "s 1", "prompt_tokens": 120, "completion_tokens": 30,
            "cache_read_input_tokens": 0, "cache_write_input_tokens": 0,
            "cost_usd": null, "budget_max_steps": 20, "steps_used": 2,
            "context_window_tokens": 8192, "context_tokens_used": 150,
            "tool_stats": [], "exchanges_count": 2, "started_at": null,
            "updated_at": null, "active_duration_ms": 900, "llm_backend": null,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm of the command reads them, the id encoded
    let metrics: apollia_runtime::chat::SessionMetrics = client
        .get(&crate::commands::chat::metrics_path("s 1"))
        .await
        .unwrap();

    // THEN the record is the one the metrics panel draws, and the command
    // passes the gate
    assert_eq!(metrics.session_id, "s 1");
    assert_eq!(metrics.prompt_tokens, 120);
    assert_eq!(metrics.exchanges_count, 2);
    assert!(gate(LinkMode::Attached, "chat_session_metrics").is_ok());
}

#[tokio::test]
async fn test_a_refusal_of_the_metrics_by_the_daemon_shows_its_own_message() {
    // GIVEN a daemon whose chat subsystem did not start
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s-1/metrics"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(serde_json::json!({ "error": "chat subsystem not available" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the metrics are read
    let error = client
        .get::<apollia_runtime::chat::SessionMetrics>(&crate::commands::chat::metrics_path("s-1"))
        .await
        .unwrap_err();

    // THEN the error carries the daemon's words, as the embedded mode says them
    assert!(matches!(error, AttachedError::Refused { status: 503, .. }));
    assert_eq!(
        error.for_display(UiLocale::En),
        "chat subsystem not available"
    );
}

#[tokio::test]
async fn test_the_folder_a_relative_path_is_revealed_against_comes_from_the_daemon() {
    // GIVEN a daemon resolving the working directory of one session, and not
    // of another
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s-1/workspace"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "workspace": "/srv/projet" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s-2/workspace"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "workspace": null })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the reveal command asks the daemon for each folder
    let known = crate::commands::chat::export::daemon_workspace(&client, "s-1")
        .await
        .unwrap();
    let unknown = crate::commands::chat::export::daemon_workspace(&client, "s-2")
        .await
        .unwrap();

    // THEN the first is the daemon's folder, the second leaves the raw path
    assert_eq!(known, Some(std::path::PathBuf::from("/srv/projet")));
    assert_eq!(unknown, None);
}

#[test]
fn test_the_export_and_the_reveal_run_attached_as_window_commands() {
    // GIVEN the two commands acting on files of the machine carrying the window
    let commands = ["export_conversation", "reveal_session_path"];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|name| gate(LinkMode::Attached, name).is_err())
        .collect();

    // THEN none is refused, and both are window commands, not routed ones
    assert!(refused.is_empty(), "refused: {refused:?}");
    for name in commands {
        assert!(WINDOW_COMMANDS.iter().any(|(window, _)| *window == name));
        assert!(!ROUTED_COMMANDS.contains(&name));
    }
}

// ── MCP: secrets, registry, sign-in ──────────────────────────────────────

#[tokio::test]
async fn test_an_mcp_secret_is_put_and_removed_on_the_daemon_with_the_token() {
    // GIVEN a daemon that stores and removes the secret of a server, only
    // with the token of this installation, and answers both with no content
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    Mock::given(method("PUT"))
        .and(path("/api/v1/mcp/servers/notion/secrets/NOTION_API_KEY"))
        .and(header("authorization", auth.as_str()))
        .and(body_json(serde_json::json!({ "value": "secret-1" })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/mcp/servers/notion/secrets/NOTION_API_KEY"))
        .and(header("authorization", auth.as_str()))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the bodies of store_mcp_secret and delete_mcp_secret run attached
    let stored =
        crate::commands::mcp::put_mcp_secret(&link, "notion", "NOTION_API_KEY", "secret-1").await;
    let removed = crate::commands::mcp::remove_mcp_secret(&link, "notion", "NOTION_API_KEY").await;

    // THEN both reached the daemon and the empty answers read as success
    assert_eq!(stored, Ok(()));
    assert_eq!(removed, Ok(()));
    assert!(gate(LinkMode::Attached, "store_mcp_secret").is_ok());
    assert!(gate(LinkMode::Attached, "delete_mcp_secret").is_ok());
}

#[tokio::test]
async fn test_an_mcp_secret_refused_by_the_daemon_shows_its_message() {
    // GIVEN a daemon that refuses a server name it cannot store a secret for
    let server = MockServer::start().await;
    let refusal = "server name must be lowercase letters, digits, '_' or '-'";
    Mock::given(method("PUT"))
        .and(path("/api/v1/mcp/servers/Bad%20Name/secrets/KEY"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(serde_json::json!({ "error": refusal })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the body of store_mcp_secret runs attached with that name
    let error = crate::commands::mcp::put_mcp_secret(&link, "Bad Name", "KEY", "v")
        .await
        .unwrap_err();

    // THEN the name was encoded into one segment, and the interface reads
    // the daemon's own words
    assert_eq!(error, refusal);
}

#[tokio::test]
async fn test_the_mcp_registry_is_read_from_the_daemon_with_the_search_and_the_token() {
    // GIVEN a daemon whose registry route lists one server for a search
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/mcp/registry"))
        .and(query_param("search", "base de notes"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "server": {
                    "name": "io.github.owner/notes", "title": "Notes",
                    "description": "Read notes", "version": "1.0.0",
                    "repository": null, "websiteUrl": null, "packages": null, "icons": null
                },
                "_meta": null
            }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the body of fetch_mcp_registry reads the registry attached
    let servers =
        crate::commands::mcp::discovery::fetch_registry_servers(&link, Some(" base de notes "))
            .await
            .unwrap();

    // THEN the daemon's entry comes back in the registry's shape
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].server.name, "io.github.owner/notes");
    assert!(gate(LinkMode::Attached, "fetch_mcp_registry").is_ok());
}

#[tokio::test]
async fn test_an_unreachable_registry_on_the_daemon_shows_its_message() {
    // GIVEN a daemon whose registry is offline with no cache
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/mcp/registry"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(serde_json::json!({ "error": "registry offline" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the registry is read attached, without a search
    let error = crate::commands::mcp::discovery::fetch_registry_servers(&link, None)
        .await
        .unwrap_err();

    // THEN the daemon's own words come back
    assert_eq!(error, "registry offline");
}

#[tokio::test]
async fn test_an_empty_answer_of_the_embedded_runtime_reads_as_null() {
    // GIVEN a runtime listener answering a removal with no content
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/mcp/servers/notion/secrets/KEY"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let port = server.address().port();

    // WHEN the embedded arm's helper sends the removal
    let answer =
        crate::commands::http_delete_json(port, "/api/v1/mcp/servers/notion/secrets/KEY").await;

    // THEN the empty body is a success, not an unreadable answer
    assert_eq!(answer, Ok(serde_json::Value::Null));
}

#[test]
fn test_the_mcp_sign_in_commands_run_attached_as_window_commands() {
    // GIVEN the commands the MCP wizard calls around the sign-in, which read
    // no state of the runtime
    let sign_in = [
        "list_mcp_enrichments",
        "mcp_oauth_discover",
        "mcp_oauth_resolve_client_id",
        "mcp_oauth_login",
    ];

    // WHEN each is gated while attached
    // THEN none is refused, and each is a window command with its reason
    for command in sign_in {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        assert!(
            WINDOW_COMMANDS
                .iter()
                .any(|(name, reason)| *name == command && !reason.is_empty()),
            "{command} is not a window command"
        );
    }
}

// ── The model hub: downloads, import, removal ───────────────────────────

#[tokio::test]
async fn test_an_installed_model_is_removed_on_the_daemon_with_the_token() {
    // GIVEN a daemon that removes an installed model, answering only the
    // token, and a path holding a space
    let server = MockServer::start().await;
    let file = "/Users/nadia/.apollia/models/Qwen 3 8B.gguf";
    Mock::given(method("DELETE"))
        .and(path("/api/v1/llm/installed-models"))
        .and(query_param("path", file))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "deleted": file })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the command removes it through the seam
    let outcome =
        crate::commands::model_hub::delete_installed_through(&link, file.to_owned()).await;

    // THEN the daemon received the removal of that exact path, and the
    // command succeeds
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn test_a_removal_the_daemon_refuses_shows_its_own_words() {
    // GIVEN a daemon refusing the removal of a model a backend uses
    let server = MockServer::start().await;
    let words = "model is in use by LLM backend 'local'; change or remove that configuration first";
    Mock::given(method("DELETE"))
        .and(path("/api/v1/llm/installed-models"))
        .respond_with(
            ResponseTemplate::new(409).set_body_json(serde_json::json!({ "error": words })),
        )
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the command asks for the removal
    let outcome = crate::commands::model_hub::delete_installed_through(
        &link,
        "/Users/nadia/.apollia/models/qwen3-8b.gguf".to_owned(),
    )
    .await;

    // THEN the interface reads the daemon's words, not an unavailability
    assert_eq!(outcome, Err(words.to_owned()));
}

#[test]
fn test_the_downloads_the_import_and_the_removal_of_a_model_pass_the_gate_attached() {
    // GIVEN the commands the model hub opens a download, a cancellation, an
    // import and a removal with
    let commands = [
        "start_model_download",
        "cancel_model_download",
        "import_model_file",
        "delete_installed_model",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused: the removal reaches the daemon, the other three
    // belong to this window and say why
    assert!(refused.is_empty(), "refused: {refused:?}");
    assert!(ROUTED_COMMANDS.contains(&"delete_installed_model"));
    for name in [
        "start_model_download",
        "cancel_model_download",
        "import_model_file",
    ] {
        let reason = WINDOW_COMMANDS
            .iter()
            .find(|(window, _)| *window == name)
            .map(|(_, reason)| *reason);
        assert!(
            reason.is_some_and(|r| r.contains("this")),
            "{name}: {reason:?}"
        );
    }
}

// ── The LLM engines ──────────────────────────────────────────────────────

fn attached_link(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

fn bearer() -> String {
    format!("Bearer {TOKEN}")
}

#[tokio::test]
async fn test_the_daemon_engines_are_listed_with_the_last_ping_of_this_window() {
    // GIVEN a daemon holding one engine, answering only the token, and a ping
    // of that engine this window recorded earlier
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/llm/backends"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "backends": [{
                "name": "local",
                "provider": "llama-cpp",
                "model": "qwen3.gguf",
                "config_json": {},
                "enabled": true,
                "is_default": true,
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let cache = crate::commands::llm::LlmPingCache::default();
    cache.write().unwrap().insert(
        "local".to_owned(),
        crate::commands::llm::PingState {
            last_error: Some("connection refused".to_owned()),
            last_latency_ms: None,
            last_ping_at: Some("2026-10-03T00:00:00Z".to_owned()),
            last_available: false,
        },
    );

    // WHEN the attached arm lists the engines
    let views = crate::commands::llm::backends_with_pings(&attached_link(&server), &cache)
        .await
        .unwrap();

    // THEN the daemon's engine comes back with the last ping projected on it
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "local");
    assert_eq!(
        views[0].last_ping_error.as_deref(),
        Some("connection refused")
    );
    assert!(gate(LinkMode::Attached, "list_llm_backends").is_ok());
}

#[tokio::test]
async fn test_an_engine_is_pinged_on_the_daemon_and_the_outcome_recorded() {
    // GIVEN a daemon that reaches the engine it is asked about
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/llm/ping"))
        .and(header("authorization", bearer().as_str()))
        .and(body_json(serde_json::json!({ "backend": "local" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "backend": "local",
            "available": true,
            "latency_ms": 42,
            "error": null,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let cache = crate::commands::llm::LlmPingCache::default();

    // WHEN the attached arm tests the engine
    let result =
        crate::commands::llm::ping_and_record(&attached_link(&server), &cache, "local".to_owned())
            .await;

    // THEN the engine answered, and this window remembers it for the list
    assert!(result.available);
    assert_eq!(result.latency_ms, Some(42));
    let recorded = cache.read().unwrap().get("local").cloned().unwrap();
    assert!(recorded.last_available);
    assert!(recorded.last_error.is_none());
    assert!(gate(LinkMode::Attached, "ping_llm_backend").is_ok());
}

#[tokio::test]
async fn test_a_ping_refused_by_the_daemon_is_a_failed_test_with_its_message() {
    // GIVEN a daemon serving no router
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/llm/ping"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(serde_json::json!({ "error": "no LLM router configured" })),
        )
        .mount(&server)
        .await;
    let cache = crate::commands::llm::LlmPingCache::default();

    // WHEN the attached arm tests an engine
    let result =
        crate::commands::llm::ping_and_record(&attached_link(&server), &cache, "local".to_owned())
            .await;

    // THEN the test failed with the daemon's own words, recorded as such
    assert!(!result.available);
    assert_eq!(result.error.as_deref(), Some("no LLM router configured"));
    let recorded = cache.read().unwrap().get("local").cloned().unwrap();
    assert_eq!(
        recorded.last_error.as_deref(),
        Some("no LLM router configured")
    );
}

#[tokio::test]
async fn test_the_cost_alert_threshold_is_read_from_the_daemon() {
    // GIVEN a daemon whose router carries a threshold, and one that has none
    let with = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/llm/cost-alert-threshold"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "threshold_usd": 0.5 })),
        )
        .expect(1)
        .mount(&with)
        .await;
    let without = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/llm/cost-alert-threshold"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "threshold_usd": null })),
        )
        .mount(&without)
        .await;

    // WHEN the attached arm reads the threshold of each
    let set = crate::commands::llm::cost_alert_threshold(&attached_link(&with)).await;
    let unset = crate::commands::llm::cost_alert_threshold(&attached_link(&without)).await;

    // THEN the first reads as its value and the second as none
    assert_eq!(set, Ok(Some(0.5)));
    assert_eq!(unset, Ok(None));
    assert!(gate(LinkMode::Attached, "get_cost_alert_threshold").is_ok());
}

#[tokio::test]
async fn test_the_engines_are_reloaded_by_the_daemon_with_the_token() {
    // GIVEN a daemon that rebuilds its router on request
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/llm/reload"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "backends": [],
            "default": "local",
            "reaches_running_agents": true,
        })))
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the attached arm asks for the reload
    let outcome = crate::commands::llm::reload_runtime_router(&attached_link(&server)).await;

    // THEN the daemon did it, and both reload commands pass the gate
    assert_eq!(outcome, Ok(()));
    assert!(gate(LinkMode::Attached, "reload_llm").is_ok());
    assert!(gate(LinkMode::Attached, "reload_llm_from_db").is_ok());
}

#[tokio::test]
async fn test_a_list_threshold_or_reload_refused_by_the_daemon_shows_its_message() {
    // GIVEN a daemon refusing each of the three routes with its own words
    let server = MockServer::start().await;
    for (verb, route, words) in [
        (
            "GET",
            "/api/v1/llm/backends",
            "backend repository unavailable",
        ),
        (
            "GET",
            "/api/v1/llm/cost-alert-threshold",
            "threshold unreadable",
        ),
        (
            "POST",
            "/api/v1/llm/reload",
            "rebuild failed: model file missing",
        ),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(503).set_body_json(serde_json::json!({ "error": words })),
            )
            .mount(&server)
            .await;
    }
    let link = attached_link(&server);
    let cache = crate::commands::llm::LlmPingCache::default();

    // WHEN the attached arm lists, reads the threshold and reloads
    let listed = crate::commands::llm::backends_with_pings(&link, &cache).await;
    let threshold = crate::commands::llm::cost_alert_threshold(&link).await;
    let reloaded = crate::commands::llm::reload_runtime_router(&link).await;

    // THEN the interface reads the daemon's own words, not an unavailability
    assert_eq!(listed.unwrap_err(), "backend repository unavailable");
    assert_eq!(threshold.unwrap_err(), "threshold unreadable");
    assert_eq!(reloaded.unwrap_err(), "rebuild failed: model file missing");
}
