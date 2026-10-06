//! The settings of a conversation reach the calls of its turns, and reach
//! nothing else: neither its agent nor governance.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::descriptor::{ToolDescriptor, ToolKind};
use apollia_tools::{ActionFamily, ToolRegistryHandle};

use crate::chat::manager::{ChatSessionManagerHandle, ChatToolsConfig, CreateSessionParams};
use crate::chat::types::{ChatError, ChatMode, SessionConfigUpdate};
use crate::chat::{ContextOverflowSetting, SessionSettings};
use crate::eventbus::EventBusSender;
use crate::test_support::collect_events_until;

/// Records every request streamed through it and answers its script, then
/// plain text.
struct Recording {
    requests: Arc<Mutex<Vec<CompletionRequest>>>,
    script: Mutex<VecDeque<StreamChunk>>,
}

#[async_trait::async_trait]
impl CompletionModel for Recording {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "summary".into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        let chunk = self
            .script
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| StreamChunk::Text("done".into()));
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "default"
    }
    fn model_id(&self) -> &str {
        "default"
    }
}

fn descriptor(name: &str, family: ActionFamily) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Test tool {name}"),
        kind: ToolKind::Native,
        input_schema: serde_json::json!({ "type": "object" }),
        output_schema: None,
        sandbox_profile: apollia_core::SandboxProfile::NetworkRestricted,
        tags: Vec::new(),
        family,
        family_source: apollia_tools::FamilySource::Unsourced,
        target_arg: None,
        is_read_only: family == ActionFamily::Read,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

/// A chat over `dir` whose model answers `script`, with two reading tools
/// (`notes_read`, `web_lookup`) and a writing one (`notes_write`).
struct Chat {
    handle: ChatSessionManagerHandle,
    requests: Arc<Mutex<Vec<CompletionRequest>>>,
    bus: EventBusSender,
}

impl Chat {
    /// `with_data_dir` gives the chat its data directory, hence the store of
    /// governance and the default access of the Apollia agent; without it a
    /// session runs on the tools it was created with.
    async fn over(dir: &tempfile::TempDir, script: Vec<StreamChunk>, with_data_dir: bool) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
            std::collections::HashMap::new();
        backends.insert(
            "default".into(),
            Arc::new(Recording {
                requests: Arc::clone(&requests),
                script: Mutex::new(script.into()),
            }),
        );
        let registry = ToolRegistryHandle::start();
        for (name, family) in [
            ("notes_read", ActionFamily::Read),
            ("web_lookup", ActionFamily::Read),
            ("notes_write", ActionFamily::WriteOrDelete),
        ] {
            registry
                .register(descriptor(name, family))
                .await
                .expect("register tool");
        }
        let (bus, _) = tokio::sync::broadcast::channel(256);
        let config = with_data_dir.then(|| {
            Arc::new(ChatToolsConfig {
                data_dir: dir.path().to_path_buf(),
                brave_api_key: None,
                tools_config: apollia_core::ToolsConfig::default(),
                tool_turn_temperature: None,
                source_registry: None,
                governance: None,
            })
        });
        let handle = ChatSessionManagerHandle::spawn(
            &dir.path().join("chat.db"),
            Some(Arc::new(LlmRouter::with_backends(backends, "default"))),
            registry,
            None,
            bus.clone(),
            StepBudgetConfig::default(),
            None,
            crate::registry::AgentRegistry::spawn(bus.clone()),
            None,
            None,
            config,
            LoadingMode::Eager,
            20,
            None,
            false,
        )
        .expect("spawn manager");
        Self {
            handle,
            requests,
            bus,
        }
    }

    async fn session(&self, prompt: Option<&str>, tools: &[&str]) -> String {
        self.handle
            .create_session(CreateSessionParams {
                mode: ChatMode::Libre,
                agent_name: None,
                system_prompt: prompt.map(str::to_owned),
                tools: tools.iter().map(|t| (*t).to_owned()).collect(),
                model: None,
            })
            .await
            .expect("create session")
            .id
    }

    /// Send `message` and wait for the turn to end, answered or failed.
    /// Returns the events of the turn.
    async fn turn(&self, session: &str, message: &str) -> Vec<RuntimeEvent> {
        let mut events = self.bus.subscribe();
        self.handle
            .send_message(session.to_owned(), message.to_owned())
            .await
            .expect("send message");
        let (seen, ended) = collect_events_until(Duration::from_secs(10), &mut events, |event| {
            matches!(
                event,
                RuntimeEvent::ChatResponseCompleted { .. } | RuntimeEvent::ChatError { .. }
            )
        })
        .await;
        assert!(ended, "the turn never ended");
        seen
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn text_of(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s,
        MessageContent::TextWithImages { text, .. } => text,
        MessageContent::ToolResult { content, .. } => content,
        MessageContent::WithToolCalls { text, .. } => text,
    }
}

fn system_prompt_of(request: &CompletionRequest) -> &str {
    let system = request
        .messages
        .iter()
        .find(|m| m.role == Role::System)
        .expect("a system message");
    text_of(&system.content)
}

fn offered(request: &CompletionRequest) -> Vec<&str> {
    request.tools.iter().map(|t| t.name.as_str()).collect()
}

fn call_of(tool: &str) -> StreamChunk {
    StreamChunk::ToolCall(ToolCall {
        id: "c1".into(),
        name: tool.into(),
        arguments: serde_json::json!({}),
    })
}

/// Every row of every table of the governance store of `dir`, as text: its
/// rules, its consents, its policy, the settings it keeps. The stamps the
/// store writes about its own upgrades when it is first opened are left out:
/// they record no decision.
fn governance_rows(dir: &tempfile::TempDir) -> Vec<String> {
    let db = apollia_tools::governance_db::GovernanceDb::open(dir.path()).expect("governance.db");
    let conn = db.connection();
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name <> 'governance_upgrades' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut rows = Vec::new();
    for table in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = stmt.column_count();
        let mut found = stmt.query([]).unwrap();
        while let Some(row) = found.next().unwrap() {
            let cells: Vec<String> = (0..columns)
                .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                .collect();
            rows.push(format!("{table}: {}", cells.join(" | ")));
        }
    }
    rows
}

#[tokio::test]
async fn test_a_session_prompt_is_added_to_the_mission_and_never_replaces_it() {
    // GIVEN a free chat created with a prompt of its own, the way older
    // clients send the instructions of a conversation
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::over(&dir, Vec::new(), false).await;
    let session = chat
        .session(Some("Answer in one sentence."), &["notes_read"])
        .await;

    // WHEN a message is sent
    chat.turn(&session, "Bonjour").await;

    // THEN the call keeps the built-in mission of the assistant, and the
    // prompt of the session comes after it under the heading of the
    // instructions of the conversation
    let requests = chat.requests();
    let prompt = system_prompt_of(&requests[0]);
    let mission =
        crate::chat::builtin_agent::built_in_prompt(crate::agents::global_default_autonomy());
    let mission_at = prompt
        .find(mission.trim())
        .expect("the built-in mission is kept");
    let heading_at = prompt
        .find(apollia_prompts::blocks::SESSION_INSTRUCTIONS_HEADING)
        .expect("the instructions have their heading");
    let text_at = prompt
        .find("Answer in one sentence.")
        .expect("instructions");
    assert!(mission_at < heading_at && heading_at < text_at, "{prompt}");
    let detail = chat.handle.get_session(session).await.expect("detail");
    assert_eq!(
        detail.session.settings.instructions.as_deref(),
        Some("Answer in one sentence.")
    );
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_the_request_settings_of_a_conversation_are_sent_with_its_calls() {
    // GIVEN two free chats, one of them tuned: a temperature, an answer
    // length, reasoning switched off
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::over(&dir, Vec::new(), false).await;
    let tuned = chat.session(None, &["notes_read"]).await;
    let plain = chat.session(None, &["notes_read"]).await;
    chat.handle
        .set_session_settings(
            tuned.clone(),
            SessionSettings {
                temperature: Some(0.9),
                max_tokens: Some(256),
                thinking: Some(false),
                reasoning_effort: Some(apollia_llm::ReasoningEffort::Low),
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");

    // WHEN a message is sent in each
    chat.turn(&tuned, "Bonjour").await;
    chat.turn(&plain, "Bonjour").await;

    // THEN the call of the tuned one carries its settings, the temperature
    // of the conversation winning over the tool-turn one; the other call
    // carries no answer length and keeps the tool-turn temperature
    let requests = chat.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].max_tokens, Some(256));
    assert_eq!(requests[0].temperature, Some(0.9));
    assert_eq!(requests[0].thinking, Some(false));
    assert_eq!(
        requests[0].reasoning_effort,
        Some(apollia_llm::ReasoningEffort::Low)
    );
    assert_eq!(requests[1].max_tokens, None);
    assert_eq!(
        requests[1].temperature,
        Some(crate::chat::builtin_agent::DEFAULT_TOOL_TURN_TEMPERATURE)
    );
    assert_eq!(requests[1].thinking, None);
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_the_preset_of_a_conversation_is_its_third_level_under_its_own_settings() {
    // GIVEN a chat with a data directory, a preset of its model that sets a
    // temperature and an answer length, and a conversation that chooses the
    // preset and sets its own answer length
    let dir = tempfile::tempdir().unwrap();
    let preset = apollia_core::LlmBackendRepository::open(
        &apollia_core::paths::DataFile::System.path(dir.path()),
    )
    .expect("system.db")
    .create_preset(
        apollia_core::model_preset::ModelSource::Local,
        "default",
        &apollia_core::model_preset::PresetDraft {
            name: "creative".into(),
            load: serde_json::Map::new(),
            request: apollia_core::model_preset::RequestSettings {
                temperature: Some(1.2),
                max_tokens: Some(2048),
                top_k: Some(50),
                ..Default::default()
            },
            is_model_default: false,
        },
    )
    .expect("preset")
    .id;
    let chat = Chat::over(&dir, Vec::new(), true).await;
    let session = chat.session(None, &[]).await;
    chat.handle
        .set_session_settings(
            session.clone(),
            SessionSettings {
                preset: Some(preset),
                max_tokens: Some(128),
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");

    // WHEN a message is sent
    chat.turn(&session, "Bonjour").await;

    // THEN the conversation sets the length, its preset the temperature,
    // over the tool-turn one, and the top-k
    let request = &chat.requests()[0];
    assert_eq!(request.max_tokens, Some(128));
    assert_eq!(request.temperature, Some(1.2));
    assert_eq!(request.top_k, Some(50));
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_an_access_switched_off_is_neither_offered_nor_callable_and_governance_is_untouched() {
    // GIVEN a chat whose assistant reads by default (`notes_read`,
    // `web_lookup`), a governance store, and a conversation that switches
    // `web_lookup` off; the model calls it all the same
    let dir = tempfile::tempdir().unwrap();
    let before = governance_rows(&dir);
    let chat = Chat::over(&dir, vec![call_of("web_lookup")], true).await;
    let tuned = chat.session(None, &[]).await;
    let other = chat.session(None, &[]).await;
    chat.handle
        .set_session_settings(
            tuned.clone(),
            SessionSettings {
                disabled_accesses: vec!["web_lookup".into()],
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");

    // WHEN a message is sent in that conversation, then in another one
    chat.turn(&tuned, "look it up").await;
    let turn_requests = chat.requests().len();
    chat.turn(&other, "look it up").await;

    // THEN the model was not offered the access in the first, its call was
    // answered as an unknown tool, the other conversation still offers it,
    // and nothing was written to governance
    let requests = chat.requests();
    assert!(offered(&requests[0]).contains(&"notes_read"));
    assert!(!offered(&requests[0]).contains(&"web_lookup"));
    let refusal = text_of(&requests[1].messages.last().expect("tool result").content);
    assert!(refusal.contains("unknown tool `web_lookup`"), "{refusal}");
    assert!(offered(&requests[turn_requests]).contains(&"web_lookup"));
    assert_eq!(governance_rows(&dir), before);
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_an_access_proposed_for_a_conversation_is_offered_and_asks_before_it_runs() {
    // GIVEN a chat whose assistant reads by default, and a conversation that
    // proposes a reading access it already holds and a writing one it does
    // not; the model calls the reading tool `notes_read`, then `notes_write`
    let dir = tempfile::tempdir().unwrap();
    let before = governance_rows(&dir);
    let chat = Chat::over(
        &dir,
        vec![call_of("notes_read"), call_of("notes_write")],
        true,
    )
    .await;
    let session = chat.session(None, &[]).await;
    chat.handle
        .set_session_settings(
            session.clone(),
            SessionSettings {
                proposed_accesses: vec!["notes_read".into(), "notes_write".into()],
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");

    // WHEN a message is sent, and the question the turn raises is refused
    let mut events = chat.bus.subscribe();
    chat.handle
        .send_message(session.clone(), "write it down".into())
        .await
        .expect("send");
    let (seen, asked) = collect_events_until(Duration::from_secs(10), &mut events, |event| {
        matches!(event, RuntimeEvent::ChatApprovalRequired { .. })
    })
    .await;
    assert!(asked, "the proposed access never asked");
    let Some(RuntimeEvent::ChatApprovalRequired {
        message_id,
        tool_call_id,
        tool_name,
        ..
    }) = seen.last().cloned()
    else {
        unreachable!("the last event is the approval");
    };
    chat.handle
        .resolve_tool(
            session.clone(),
            message_id,
            tool_call_id,
            tool_name.clone(),
            crate::chat::types::ToolDecision::refuse(),
        )
        .await
        .expect("refuse");
    let (_, ended) = collect_events_until(Duration::from_secs(10), &mut events, |event| {
        matches!(event, RuntimeEvent::ChatResponseCompleted { .. })
    })
    .await;

    // THEN the model was offered the proposed access, the access the agent
    // already held ran without a question, the one it did not hold asked,
    // and nothing was written to governance
    assert!(ended, "the turn never ended");
    let requests = chat.requests();
    assert!(offered(&requests[0]).contains(&"notes_write"));
    assert_eq!(tool_name, "notes_write");
    assert_eq!(governance_rows(&dir), before);
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_a_conversation_set_to_stop_at_the_limit_fails_its_turn_with_a_readable_message() {
    // GIVEN a conversation set to stop at the limit, whose first exchange is
    // already longer than the window of its model
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::over(&dir, Vec::new(), false).await;
    let session = chat.session(None, &["notes_read"]).await;
    chat.turn(&session, &"x".repeat(700_000)).await;
    chat.handle
        .set_session_settings(
            session.clone(),
            SessionSettings {
                context_overflow: Some(ContextOverflowSetting::StopAtLimit),
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");
    let calls_before = chat.requests().len();

    // WHEN the next message is sent
    let events = chat.turn(&session, "and now?").await;

    // THEN no call is made, and the turn ends on an error that says what
    // happened and what to do
    assert_eq!(chat.requests().len(), calls_before);
    let error = events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::ChatError { error, .. } => Some(error.clone()),
            _ => None,
        })
        .expect("the turn fails");
    assert!(error.contains("context limit"), "{error}");
    assert!(error.contains("fork it"), "{error}");
    assert!(error.contains("new conversation"), "{error}");
    chat.handle.shutdown().await;
}

#[tokio::test]
async fn test_settings_are_refused_on_a_closed_conversation_and_kept_across_a_restart() {
    // GIVEN a conversation with settings and notes
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::over(&dir, Vec::new(), false).await;
    let session = chat.session(None, &["notes_read"]).await;
    let settings = SessionSettings {
        max_tokens: Some(64),
        instructions: Some("Be brief.".into()),
        ..SessionSettings::default()
    };
    chat.handle
        .set_session_settings(session.clone(), settings.clone())
        .await
        .expect("settings");
    chat.handle
        .set_session_notes(session.clone(), "ask about March".into())
        .await
        .expect("notes");

    // WHEN the legacy prompt field rewrites the instructions, the
    // conversation is closed, written again, and the chat restarts
    chat.handle
        .update_session(
            session.clone(),
            SessionConfigUpdate {
                system_prompt: Some("Be very brief.".into()),
                ..SessionConfigUpdate::default()
            },
        )
        .await
        .expect("legacy update");
    chat.handle
        .close_session(session.clone())
        .await
        .expect("close");
    let refused_settings = chat
        .handle
        .set_session_settings(session.clone(), SessionSettings::default())
        .await;
    let refused_notes = chat
        .handle
        .set_session_notes(session.clone(), String::new())
        .await;
    chat.handle.shutdown().await;
    let reopened = Chat::over(&dir, Vec::new(), false).await;
    let detail = reopened
        .handle
        .get_session(session.clone())
        .await
        .expect("detail");

    // THEN the writes on the closed conversation are refused, and what was
    // stored before is read back: the instructions rewritten, the other
    // settings untouched by the legacy field, the notes
    assert!(matches!(refused_settings, Err(ChatError::SessionClosed(_))));
    assert!(matches!(refused_notes, Err(ChatError::SessionClosed(_))));
    assert_eq!(
        detail.session.settings,
        SessionSettings {
            instructions: Some("Be very brief.".into()),
            ..settings
        }
    );
    assert_eq!(detail.session.notes, "ask about March");
    reopened.handle.shutdown().await;
}

#[tokio::test]
async fn test_the_mission_of_apollia_is_read_at_each_message_and_the_instructions_follow_it() {
    // GIVEN a chat with a data directory, a conversation with instructions
    // of its own, and an Apollia agent whose mission is written after the
    // conversation was created. The governance store exists, as it does
    // beside a runtime that started
    let dir = tempfile::tempdir().unwrap();
    governance_rows(&dir);
    let chat = Chat::over(&dir, Vec::new(), true).await;
    let session = chat.session(None, &[]).await;
    chat.handle
        .set_session_settings(
            session.clone(),
            SessionSettings {
                instructions: Some("Answer in one sentence.".into()),
                ..SessionSettings::default()
            },
        )
        .await
        .expect("settings");
    apollia_tools::AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .expect("agents.db")
        .set_override(
            apollia_tools::APOLLIA_AGENT_ID,
            apollia_tools::OverrideField::Mission,
            &serde_json::json!("You keep the books of the firm."),
        )
        .expect("mission");

    // WHEN a message is sent
    chat.turn(&session, "Bonjour").await;

    // THEN the call carries the mission written since, then the instructions
    // of the conversation, and the conversation stored none of the mission
    let requests = chat.requests();
    let prompt = system_prompt_of(&requests[0]);
    let mission_at = prompt
        .find("You keep the books of the firm.")
        .expect("the mission of the agent");
    let text_at = prompt
        .find("Answer in one sentence.")
        .expect("instructions");
    assert!(mission_at < text_at, "{prompt}");
    let detail = chat.handle.get_session(session).await.expect("detail");
    assert_eq!(
        detail.session.settings.instructions.as_deref(),
        Some("Answer in one sentence.")
    );
    chat.handle.shutdown().await;
}
