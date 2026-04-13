//! AgentChatExecutor — executes Python agents in Chat Agent mode.
//!
//! Orchestrates the flow: validate agent → convert session to [`AIPTask`] →
//! run agent via [`ChatAgentRunner`] → process [`AIPResult`] → emit events.
//!
//! The [`ChatAgentRunner`] trait follows the ADR-019 pattern: it decouples
//! `apollia-runtime` from PyO3 so that the concrete implementation using
//! `AIPBridge` + `RuntimeContext` lives in `apollia-cli`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info};

use apollia_core::result::{AIPResult, InputResponseData, TaskStatus};
use apollia_core::task::{AIPInput, AIPMessage, AIPPart, AIPTask, TextPart};
use apollia_core::RuntimeEvent;

use super::builtin_agent::ChatAgentResponse;
use super::types::{
    ChatError, ChatMode, ChatRole, ChatSession, PendingChatApprovals, ToolDecision,
};
use crate::eventbus::EventBusSender;

/// Default timeout for chat tool approval requests (5 minutes).
const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

// ─────────────────────────────────────────────
// ChatAgentRunner — trait for Python agent execution
// ─────────────────────────────────────────────

/// Trait abstracting Python agent execution for chat sessions.
///
/// Follows the ADR-019 pattern: decouples `apollia-runtime` from PyO3
/// by defining the execution contract as a trait. The concrete implementation
/// using `AIPBridge` + `RuntimeContext` lives in `apollia-cli`.
#[async_trait::async_trait]
pub trait ChatAgentRunner: Send + Sync {
    /// Execute an agent's `run()` with the given task and return the result.
    ///
    /// The implementation is responsible for:
    /// - Loading the agent module via PyO3 (if not cached)
    /// - Creating the `RuntimeContext` (tools, memory, LLM, budget)
    /// - Calling `AIPBridge.call_run(task, ctx)`
    /// - Returning the `AIPResult`
    async fn run_agent(&self, agent_name: &str, task: AIPTask) -> Result<AIPResult, String>;
}

// ─────────────────────────────────────────────
// AgentChatExecutor — orchestrates chat agent exchanges
// ─────────────────────────────────────────────

/// Executor for Chat Agent mode sessions.
///
/// Orchestrates the flow: convert session to [`AIPTask`] → run agent
/// via [`ChatAgentRunner`] → process result → emit events.
/// Called from [`ChatSessionManager`] when a message is sent in Agent mode.
pub struct AgentChatExecutor {
    /// Runner that delegates to the concrete Python agent execution.
    agent_runner: Arc<dyn ChatAgentRunner>,
    /// Event bus for emitting chat lifecycle events.
    event_bus: EventBusSender,
}

impl AgentChatExecutor {
    /// Create a new executor with the given runner and event bus.
    pub fn new(agent_runner: Arc<dyn ChatAgentRunner>, event_bus: EventBusSender) -> Self {
        Self {
            agent_runner,
            event_bus,
        }
    }

    /// Execute a chat exchange with a Python agent.
    ///
    /// Converts the session history to an [`AIPTask`], calls the agent's `run()`,
    /// and processes the result. Handles `Completed`, `InputRequired` (HITL),
    /// and `Failed` statuses.
    ///
    /// # Errors
    ///
    /// Returns [`ChatError::AgentNotFound`] if the session has no `agent_name`.
    /// Returns [`ChatError::AgentLoadFailed`] if the agent runner returns an error.
    /// Returns [`ChatError::InternalError`] if the agent returns `Failed` status.
    pub async fn execute(
        &self,
        session: &ChatSession,
        user_message: &str,
        message_id: &str,
        authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
    ) -> Result<ChatAgentResponse, ChatError> {
        let agent_name = session
            .agent_name
            .as_deref()
            .ok_or_else(|| ChatError::AgentNotFound("no agent_name in session".into()))?;

        if session.mode != ChatMode::Agent {
            return Err(ChatError::InternalError(
                "AgentChatExecutor called on non-Agent session".into(),
            ));
        }

        let task = session_to_task(session, user_message);

        let _ = self.event_bus.send(RuntimeEvent::ChatResponseStarted {
            session_id: session.id.clone(),
            message_id: message_id.to_string(),
        });

        info!(
            session_id = %session.id,
            agent = %agent_name,
            "Chat Agent: executing run()"
        );

        let result = self
            .agent_runner
            .run_agent(agent_name, task)
            .await
            .map_err(ChatError::AgentLoadFailed)?;

        self.process_result(
            &result,
            session,
            user_message,
            message_id,
            agent_name,
            authorized_tools,
            pending_approvals,
        )
        .await
    }

    /// Process the AIPResult from the agent and produce a ChatAgentResponse.
    #[allow(clippy::too_many_arguments)]
    async fn process_result(
        &self,
        result: &AIPResult,
        session: &ChatSession,
        user_message: &str,
        message_id: &str,
        agent_name: &str,
        _authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
    ) -> Result<ChatAgentResponse, ChatError> {
        match result.status {
            TaskStatus::Completed => {
                let content = extract_text_output(result);
                self.emit_completed(&session.id, message_id, &content);
                Ok(build_response(content, vec![]))
            }

            TaskStatus::InputRequired => {
                self.handle_input_required(
                    result,
                    session,
                    user_message,
                    message_id,
                    agent_name,
                    pending_approvals,
                )
                .await
            }

            TaskStatus::Failed => {
                let error_msg = result
                    .error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "agent execution failed".into());
                error!(
                    session_id = %session.id,
                    agent = %agent_name,
                    error = %error_msg,
                    "Chat Agent: run() failed"
                );
                Err(ChatError::InternalError(error_msg))
            }

            ref other => Err(ChatError::InternalError(format!(
                "unexpected task status from agent: {other:?}"
            ))),
        }
    }

    /// Handle the HITL flow when the agent returns InputRequired.
    async fn handle_input_required(
        &self,
        result: &AIPResult,
        session: &ChatSession,
        user_message: &str,
        message_id: &str,
        agent_name: &str,
        pending_approvals: &PendingChatApprovals,
    ) -> Result<ChatAgentResponse, ChatError> {
        let prompt = result
            .input_required_data
            .as_ref()
            .map(|d| d.prompt.clone())
            .unwrap_or_else(|| "Agent requires approval".into());

        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
            session_id: session.id.clone(),
            message_id: message_id.to_string(),
            tool_name: "agent_action".into(),
            prompt: prompt.clone(),
        });

        let key = format!("{}::{}::agent_action", session.id, message_id);
        let rx = pending_approvals.register(key.clone());
        pending_approvals.start_timeout(
            key,
            CHAT_APPROVAL_TIMEOUT,
            self.event_bus.clone(),
            session.id.clone(),
            message_id.to_string(),
            "agent_action".to_string(),
        );

        let decision = rx.await.unwrap_or(ToolDecision::Refuse);

        match decision {
            ToolDecision::Refuse => {
                let content = "Opération refusée par l'utilisateur".to_string();
                self.emit_completed(&session.id, message_id, &content);
                Ok(build_response(content, vec![]))
            }

            ToolDecision::Accept | ToolDecision::AlwaysAccept => {
                let newly_authorized = if decision == ToolDecision::AlwaysAccept {
                    vec!["agent_action".to_string()]
                } else {
                    vec![]
                };

                let resume_result = self
                    .resume_agent(result, session, user_message, agent_name)
                    .await?;

                let content = extract_text_output(&resume_result);
                self.emit_completed(&session.id, message_id, &content);
                Ok(build_response(content, newly_authorized))
            }
        }
    }

    /// Resume an agent after HITL approval (is_resumed=true + input_response).
    async fn resume_agent(
        &self,
        original_result: &AIPResult,
        session: &ChatSession,
        user_message: &str,
        agent_name: &str,
    ) -> Result<AIPResult, ChatError> {
        let context = original_result
            .input_required_data
            .as_ref()
            .map(|d| d.context.clone())
            .unwrap_or(serde_json::Value::Null);

        let mut resume_task = session_to_task(session, user_message);
        resume_task.is_resumed = true;
        resume_task.input_response = Some(InputResponseData {
            approved: true,
            reason: None,
            context,
            responded_at: now_rfc3339(),
        });

        self.agent_runner
            .run_agent(agent_name, resume_task)
            .await
            .map_err(ChatError::AgentLoadFailed)
    }

    /// Emit a ChatResponseCompleted event.
    fn emit_completed(&self, session_id: &str, message_id: &str, content: &str) {
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: content.to_string(),
        });
    }
}

// ─────────────────────────────────────────────
// Pure helper functions
// ─────────────────────────────────────────────

/// Convert a chat session's history into an [`AIPTask`] for the agent's `run()`.
///
/// The task contains:
/// - A fresh UUID v4 as `task_id`
/// - `session.id` as `context_id`
/// - `user_message` as the sole `TextPart` in `input.parts`
/// - All `User`/`Assistant` messages from history converted to [`AIPMessage`]
/// - `is_resumed = false`, `input_response = None`
fn session_to_task(session: &ChatSession, user_message: &str) -> AIPTask {
    AIPTask {
        task_id: uuid::Uuid::new_v4().to_string(),
        context_id: session.id.clone(),
        input: AIPInput {
            parts: vec![AIPPart::Text(TextPart {
                text: user_message.to_string(),
            })],
        },
        history: session
            .history
            .iter()
            .filter(|m| matches!(m.role, ChatRole::User | ChatRole::Assistant))
            .map(|m| AIPMessage {
                role: match m.role {
                    ChatRole::User => "user".to_string(),
                    ChatRole::Assistant => "agent".to_string(),
                    _ => "user".to_string(),
                },
                parts: vec![AIPPart::Text(TextPart {
                    text: m.content.clone(),
                })],
            })
            .collect(),
        timeout_seconds: None,
        is_resumed: false,
        input_response: None,
    }
}

/// Extract text content from an [`AIPResult`]'s output parts.
///
/// Concatenates all `TextPart` values with newlines.
/// Returns an empty string if no text parts are present.
fn extract_text_output(result: &AIPResult) -> String {
    result
        .output
        .iter()
        .filter_map(|part| {
            if let AIPPart::Text(t) = part {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a [`ChatAgentResponse`] with the given content and newly authorized tools.
fn build_response(content: String, newly_authorized: Vec<String>) -> ChatAgentResponse {
    ChatAgentResponse {
        content,
        tool_calls: vec![],
        newly_authorized,
        tokens_used: apollia_llm::types::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            ..Default::default()
        },
        thinking_trace: None,
    }
}

/// Return the current time as an RFC-3339/ISO-8601 string.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::task::AIPPart;
    use std::collections::HashSet;

    use super::super::types::ChatMessage;

    // ── Test ChatAgentRunner ──

    /// Runner that returns Completed with given text.
    struct CompletedRunner {
        response_text: String,
    }

    #[async_trait::async_trait]
    impl ChatAgentRunner for CompletedRunner {
        async fn run_agent(&self, _name: &str, _task: AIPTask) -> Result<AIPResult, String> {
            Ok(AIPResult::completed(&self.response_text))
        }
    }

    /// Runner that returns Failed.
    struct FailedRunner {
        error_message: String,
    }

    #[async_trait::async_trait]
    impl ChatAgentRunner for FailedRunner {
        async fn run_agent(&self, _name: &str, _task: AIPTask) -> Result<AIPResult, String> {
            Ok(AIPResult::failed("AGENT_ERROR", &self.error_message))
        }
    }

    /// Runner that returns Err (agent load failure).
    struct LoadErrorRunner;

    #[async_trait::async_trait]
    impl ChatAgentRunner for LoadErrorRunner {
        async fn run_agent(&self, _name: &str, _task: AIPTask) -> Result<AIPResult, String> {
            Err("agent not found on disk".into())
        }
    }

    /// Runner that returns InputRequired on first call, Completed on resume.
    struct HitlRunner;

    #[async_trait::async_trait]
    impl ChatAgentRunner for HitlRunner {
        async fn run_agent(&self, _name: &str, task: AIPTask) -> Result<AIPResult, String> {
            if task.is_resumed {
                Ok(AIPResult::completed("resumed successfully"))
            } else {
                Ok(AIPResult::input_required(
                    "Approve this action?",
                    serde_json::json!({"tool": "dangerous_op"}),
                ))
            }
        }
    }

    // ── Helpers ──

    fn test_session(agent_name: &str) -> ChatSession {
        ChatSession {
            id: "sess-test".into(),
            mode: ChatMode::Agent,
            agent_name: Some(agent_name.into()),
            system_prompt: String::new(),
            status: super::super::types::SessionStatus::Active,
            history: vec![],
            authorized_tools: HashSet::new(),
            available_tools: vec!["bash_executor".into()],
            created_at: "2026-03-20T10:00:00Z".into(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id: None,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    fn test_session_with_history() -> ChatSession {
        ChatSession {
            id: "sess-history".into(),
            mode: ChatMode::Agent,
            agent_name: Some("test-agent".into()),
            system_prompt: String::new(),
            status: super::super::types::SessionStatus::Active,
            history: vec![
                ChatMessage {
                    id: "m1".into(),
                    role: ChatRole::User,
                    content: "Hello".into(),
                    tool_calls: None,
                    tool_name: None,
                    created_at: "2026-03-20T10:00:00Z".into(),
                    seq: 1,
                    metadata: None,
                },
                ChatMessage {
                    id: "m2".into(),
                    role: ChatRole::Assistant,
                    content: "Hi there!".into(),
                    tool_calls: None,
                    tool_name: None,
                    created_at: "2026-03-20T10:00:01Z".into(),
                    seq: 2,
                    metadata: None,
                },
                ChatMessage {
                    id: "m3".into(),
                    role: ChatRole::System,
                    content: "system message".into(),
                    tool_calls: None,
                    tool_name: None,
                    created_at: "2026-03-20T10:00:02Z".into(),
                    seq: 3,
                    metadata: None,
                },
                ChatMessage {
                    id: "m4".into(),
                    role: ChatRole::Tool,
                    content: "tool output".into(),
                    tool_calls: None,
                    tool_name: Some("bash_executor".into()),
                    created_at: "2026-03-20T10:00:03Z".into(),
                    seq: 4,
                    metadata: None,
                },
                ChatMessage {
                    id: "m5".into(),
                    role: ChatRole::User,
                    content: "Thanks".into(),
                    tool_calls: None,
                    tool_name: None,
                    created_at: "2026-03-20T10:00:04Z".into(),
                    seq: 5,
                    metadata: None,
                },
            ],
            authorized_tools: HashSet::new(),
            available_tools: vec![],
            created_at: "2026-03-20T09:59:00Z".into(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id: None,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    fn make_event_bus() -> EventBusSender {
        let (tx, _) = tokio::sync::broadcast::channel(128);
        tx
    }

    // ── session_to_task tests ──

    #[test]
    fn test_session_to_task_conversion() {
        // GIVEN a session with 4 messages (User, Assistant, System, Tool, User)
        let session = test_session_with_history();

        // WHEN session_to_task is called
        let task = session_to_task(&session, "nouveau message");

        // THEN task_id is a UUID, context_id = session.id
        assert!(!task.task_id.is_empty());
        assert_eq!(task.context_id, "sess-history");

        // AND input contains the user message
        assert_eq!(task.input.parts.len(), 1);
        if let AIPPart::Text(ref t) = task.input.parts[0] {
            assert_eq!(t.text, "nouveau message");
        } else {
            panic!("expected TextPart");
        }

        // AND history contains only User and Assistant messages (3 out of 5)
        assert_eq!(task.history.len(), 3);
        assert_eq!(task.history[0].role, "user");
        assert_eq!(task.history[1].role, "agent");
        assert_eq!(task.history[2].role, "user");

        // AND is_resumed=false, input_response=None
        assert!(!task.is_resumed);
        assert!(task.input_response.is_none());
    }

    #[test]
    fn test_session_to_task_filters_system_tool_roles() {
        // GIVEN a session with System and Tool messages
        let session = test_session_with_history();

        // WHEN session_to_task is called
        let task = session_to_task(&session, "query");

        // THEN only User (2) and Assistant (1) messages appear in history
        assert_eq!(task.history.len(), 3);
        for msg in &task.history {
            assert!(
                msg.role == "user" || msg.role == "agent",
                "unexpected role: {}",
                msg.role
            );
        }
    }

    #[test]
    fn test_session_to_task_empty_history() {
        // GIVEN a session with no history
        let session = test_session("test-agent");

        // WHEN session_to_task is called
        let task = session_to_task(&session, "first message");

        // THEN history is empty
        assert!(task.history.is_empty());
        assert_eq!(task.input.parts.len(), 1);
    }

    // ── extract_text_output tests ──

    #[test]
    fn test_extract_text_output_single() {
        // GIVEN an AIPResult with one text part
        let result = AIPResult::completed("Hello world");

        // WHEN extract_text_output is called
        let text = extract_text_output(&result);

        // THEN the text is extracted
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_output_empty() {
        // GIVEN an AIPResult with no output
        let result = AIPResult {
            task_id: String::new(),
            status: TaskStatus::Completed,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        };

        // WHEN extract_text_output is called
        let text = extract_text_output(&result);

        // THEN empty string
        assert!(text.is_empty());
    }

    // ── AgentChatExecutor tests ──

    #[tokio::test]
    async fn test_agent_completed_response() {
        // GIVEN a runner that returns Completed
        let runner = Arc::new(CompletedRunner {
            response_text: "Here is the result".into(),
        });
        let event_bus = make_event_bus();
        let executor = AgentChatExecutor::new(runner, event_bus);
        let session = test_session("devis-agent");
        let approvals = PendingChatApprovals::new();

        // WHEN execute is called
        let response = executor
            .execute(
                &session,
                "Génère un devis",
                "msg-1",
                &HashSet::new(),
                &approvals,
            )
            .await
            .expect("should succeed");

        // THEN content matches the runner's output
        assert_eq!(response.content, "Here is the result");
        assert!(response.tool_calls.is_empty());
        assert!(response.newly_authorized.is_empty());
    }

    #[tokio::test]
    async fn test_agent_not_found() {
        // GIVEN a session with no agent_name
        let runner = Arc::new(CompletedRunner {
            response_text: "".into(),
        });
        let event_bus = make_event_bus();
        let executor = AgentChatExecutor::new(runner, event_bus);
        let mut session = test_session("test");
        session.agent_name = None;
        let approvals = PendingChatApprovals::new();

        // WHEN execute is called
        let result = executor
            .execute(&session, "hello", "msg-1", &HashSet::new(), &approvals)
            .await;

        // THEN Err(AgentNotFound)
        assert!(matches!(result, Err(ChatError::AgentNotFound(_))));
    }

    #[tokio::test]
    async fn test_agent_load_failed() {
        // GIVEN a runner that returns Err
        let runner = Arc::new(LoadErrorRunner);
        let event_bus = make_event_bus();
        let executor = AgentChatExecutor::new(runner, event_bus);
        let session = test_session("missing-agent");
        let approvals = PendingChatApprovals::new();

        // WHEN execute is called
        let result = executor
            .execute(&session, "hello", "msg-1", &HashSet::new(), &approvals)
            .await;

        // THEN Err(AgentLoadFailed)
        assert!(matches!(result, Err(ChatError::AgentLoadFailed(_))));
    }

    #[tokio::test]
    async fn test_agent_crash_recovery() {
        // GIVEN a runner that returns Failed status
        let runner = Arc::new(FailedRunner {
            error_message: "Python exception in agent".into(),
        });
        let event_bus = make_event_bus();
        let executor = AgentChatExecutor::new(runner, event_bus);
        let session = test_session("buggy-agent");
        let approvals = PendingChatApprovals::new();

        // WHEN execute is called
        let result = executor
            .execute(&session, "hello", "msg-1", &HashSet::new(), &approvals)
            .await;

        // THEN Err(InternalError) — session stays Active (handled by manager)
        assert!(matches!(result, Err(ChatError::InternalError(_))));
        if let Err(ChatError::InternalError(msg)) = result {
            assert!(msg.contains("Python exception"));
        }
    }

    #[tokio::test]
    async fn test_agent_input_required_refuse() {
        // GIVEN a runner that returns InputRequired
        let runner = Arc::new(HitlRunner);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel(128);
        let executor = AgentChatExecutor::new(runner, event_tx);
        let session = test_session("hitl-agent");
        let approvals = PendingChatApprovals::new();

        // Pre-register approval resolution (simulate user refusing)
        let sid = session.id.clone();
        let approvals_clone = approvals.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let key = format!("{sid}::msg-hitl::agent_action");
            approvals_clone.resolve(&key, ToolDecision::Refuse);
        });

        // WHEN execute is called
        let response = executor
            .execute(
                &session,
                "do something dangerous",
                "msg-hitl",
                &HashSet::new(),
                &approvals,
            )
            .await
            .expect("should succeed with refusal message");

        // THEN content is the refusal message
        assert_eq!(response.content, "Opération refusée par l'utilisateur");
    }

    #[tokio::test]
    async fn test_agent_input_required_accept() {
        // GIVEN a runner that returns InputRequired then Completed on resume
        let runner = Arc::new(HitlRunner);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel(128);
        let executor = AgentChatExecutor::new(runner, event_tx);
        let session = test_session("hitl-agent");
        let approvals = PendingChatApprovals::new();

        // Pre-register approval resolution (simulate user accepting)
        let sid = session.id.clone();
        let approvals_clone = approvals.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let key = format!("{sid}::msg-hitl2::agent_action");
            approvals_clone.resolve(&key, ToolDecision::Accept);
        });

        // WHEN execute is called
        let response = executor
            .execute(
                &session,
                "do something",
                "msg-hitl2",
                &HashSet::new(),
                &approvals,
            )
            .await
            .expect("should succeed after approval");

        // THEN content is the resumed agent's output
        assert_eq!(response.content, "resumed successfully");
        assert!(response.newly_authorized.is_empty());
    }

    #[tokio::test]
    async fn test_agent_input_required_always_accept() {
        // GIVEN a runner that returns InputRequired then Completed on resume
        let runner = Arc::new(HitlRunner);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel(128);
        let executor = AgentChatExecutor::new(runner, event_tx);
        let session = test_session("hitl-agent");
        let approvals = PendingChatApprovals::new();

        // Pre-register approval resolution (simulate AlwaysAccept)
        let sid = session.id.clone();
        let approvals_clone = approvals.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let key = format!("{sid}::msg-hitl3::agent_action");
            approvals_clone.resolve(&key, ToolDecision::AlwaysAccept);
        });

        // WHEN execute is called
        let response = executor
            .execute(
                &session,
                "do something",
                "msg-hitl3",
                &HashSet::new(),
                &approvals,
            )
            .await
            .expect("should succeed after always-accept");

        // THEN content is the resumed agent's output AND newly_authorized is populated
        assert_eq!(response.content, "resumed successfully");
        assert_eq!(response.newly_authorized, vec!["agent_action"]);
    }
}
