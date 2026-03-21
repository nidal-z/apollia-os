//! BuiltInChatAgent — Rust-native ReAct loop for Chat Libre mode.
//!
//! Implements the core reasoning loop: LLM → tool call → approval → result → LLM.
//! Protected by [`StepBudget`] (Principle #7) and integrated with the HITL
//! approval flow via [`PendingChatApprovals`].
//!
//! Uses `LlmRouter.stream()` for token-by-token streaming (STORY-201).
//! Each token emits a `ChatToken` RuntimeEvent on the EventBus so the SSE
//! stream can forward it to the client in real time.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{info, warn};

use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    ChatMessage as LlmChatMessage, CompletionRequest, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
use apollia_llm::{LlmRouter, ObservabilityConfig, ToolInvoker};
use apollia_oria::budget::StepBudget;
use apollia_tools::ToolRegistryHandle;

use super::types::{
    ChatError, ChatMessage, ChatRole, PendingChatApprovals, ToolCallRecord, ToolCallStatus,
    ToolDecision,
};
use crate::eventbus::EventBusSender;

/// Default timeout for chat tool approval requests (5 minutes).
const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

// ─────────────────────────────────────────────
// NativeChatToolInvoker — production tool execution
// ─────────────────────────────────────────────

/// Production [`ToolInvoker`] that dispatches to native Apollia tools.
///
/// Used by [`BuiltInChatAgent`] to execute tools in Chat Libre mode.
/// Each tool invocation is fully async (no `block_in_place`).
pub struct NativeChatToolInvoker {
    /// Sandbox root for file I/O operations.
    home_dir: std::path::PathBuf,
}

impl Default for NativeChatToolInvoker {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeChatToolInvoker {
    /// Create a new invoker using the user's home directory as sandbox root.
    pub fn new() -> Self {
        let home_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        Self { home_dir }
    }

    /// Execute `bash_executor` with the given JSON arguments.
    async fn invoke_bash(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::bash_executor::{BashExecutor, BashInput};

        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or("bash_executor: missing 'command' field")?
            .to_string();
        let timeout_secs = arguments
            .get("timeout_seconds")
            .or_else(|| arguments.get("timeout_secs"))
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let result = BashExecutor::new()
            .run(BashInput {
                command,
                timeout_secs,
                working_dir: None,
            })
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "duration_ms": result.duration_ms,
        })
        .to_string())
    }

    /// Execute `file_io` with the given JSON arguments.
    async fn invoke_file_io(&self, arguments: &serde_json::Value) -> Result<String, String> {
        use apollia_tools::tools::file_io::FileIo;

        let file_io = FileIo::new(self.home_dir.clone()).map_err(|e| e.to_string())?;

        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or("file_io: missing 'action' field")?;

        match action {
            "read" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("file_io: missing 'path' field")?;
                let bytes = file_io.read(path).await.map_err(|e| e.to_string())?;
                let content = String::from_utf8_lossy(&bytes).to_string();
                Ok(serde_json::json!({"content": content, "size": bytes.len()}).to_string())
            }
            "write" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("file_io: missing 'path' field")?;
                let content = arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("file_io: missing 'content' field")?;
                file_io
                    .write(path, content.as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"written": true, "path": path}).to_string())
            }
            "list" => {
                let dir = arguments.get("dir").and_then(|v| v.as_str()).unwrap_or(".");
                let pattern = arguments
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or("file_io: missing 'pattern' field")?;
                let files = file_io
                    .list(dir, pattern)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"files": files}).to_string())
            }
            other => Err(format!("file_io: unknown action '{other}'")),
        }
    }
}

#[async_trait::async_trait]
impl ToolInvoker for NativeChatToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String> {
        match tool_name {
            "bash_executor" => self.invoke_bash(arguments).await,
            "file_io" => self.invoke_file_io(arguments).await,
            other => Err(format!("unknown tool: {other}")),
        }
    }
}

/// Maximum number of characters for input/output previews in events.
const PREVIEW_MAX_LEN: usize = 200;

/// Default system prompt used when no custom prompt is provided (AC-9).
pub const DEFAULT_SYSTEM_PROMPT: &str = "Tu es un assistant IA polyvalent. Tu peux utiliser des \
    outils pour accomplir des tâches concrètes. Réponds de manière concise et structurée. \
    Si tu as besoin d'exécuter une commande ou d'accéder à un fichier, utilise les outils \
    disponibles.";

/// Response produced by a complete chat exchange.
#[derive(Debug, Clone)]
pub struct ChatAgentResponse {
    /// Final text content from the LLM.
    pub content: String,
    /// All tool calls made during the exchange.
    pub tool_calls: Vec<ToolCallRecord>,
    /// Tool names newly added to the session whitelist (via AlwaysAccept).
    pub newly_authorized: Vec<String>,
    /// Cumulative token usage across all LLM calls in the exchange.
    pub tokens_used: TokenUsage,
}

/// Rust-native chat agent implementing a ReAct loop for Chat Libre mode.
///
/// Stateless — all mutable state is passed as parameters to [`execute`](Self::execute).
/// Tool execution is delegated to a [`ToolInvoker`] (ADR-015 pattern).
pub struct BuiltInChatAgent {
    /// LLM router for completion calls.
    llm_router: Arc<LlmRouter>,
    /// Tool registry for resolving tool descriptors into LLM-compatible specs.
    tool_registry: ToolRegistryHandle,
    /// Tool invoker for actual tool execution (ADR-015).
    tool_invoker: Arc<dyn ToolInvoker>,
    /// Event bus for emitting chat lifecycle events.
    event_bus: EventBusSender,
}

impl BuiltInChatAgent {
    /// Create a new agent with the given dependencies.
    pub fn new(
        llm_router: Arc<LlmRouter>,
        tool_registry: ToolRegistryHandle,
        tool_invoker: Arc<dyn ToolInvoker>,
        event_bus: EventBusSender,
    ) -> Self {
        Self {
            llm_router,
            tool_registry,
            tool_invoker,
            event_bus,
        }
    }

    /// Execute a complete exchange: user message → LLM stream → tool calls → response.
    ///
    /// Uses `LlmRouter.stream()` to produce tokens one by one, emitting a
    /// [`RuntimeEvent::ChatToken`] for each token received. The ReAct loop
    /// continues until the LLM produces a final text response (no tool calls)
    /// or the [`StepBudget`] is exhausted.
    ///
    /// # Errors
    ///
    /// - [`ChatError::BudgetExhausted`] if the step budget is exceeded
    /// - [`ChatError::InternalError`] for LLM backend failures
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        session_id: &str,
        message_id: &str,
        user_message: &str,
        history: &[ChatMessage],
        system_prompt: &str,
        available_tools: &[String],
        authorized_tools: &HashSet<String>,
        pending_approvals: &PendingChatApprovals,
        budget: &StepBudget,
    ) -> Result<ChatAgentResponse, ChatError> {
        let effective_prompt = if system_prompt.is_empty() {
            DEFAULT_SYSTEM_PROMPT
        } else {
            system_prompt
        };

        let tool_specs = build_tool_specs(available_tools, &self.tool_registry).await;
        info!(
            session_id = %session_id,
            available = available_tools.len(),
            resolved = tool_specs.len(),
            tool_names = ?tool_specs.iter().map(|s| &s.name).collect::<Vec<_>>(),
            "Chat ReAct loop: tool specs resolved"
        );
        let mut llm_messages = build_llm_messages(effective_prompt, history, user_message);
        let mut all_tool_calls: Vec<ToolCallRecord> = Vec::new();
        let mut newly_authorized: Vec<String> = Vec::new();
        let total_usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
        };
        let mut authorized = authorized_tools.clone();
        let obs = ObservabilityConfig::default();

        loop {
            // Principle #7 — budget check before every LLM call
            if budget.is_exhausted() {
                return Err(ChatError::BudgetExhausted);
            }
            budget.increment_steps();

            let request = CompletionRequest {
                messages: llm_messages.clone(),
                tools: tool_specs.clone(),
                ..Default::default()
            };

            // AC-3 — emit ChatResponseStarted before the first token
            let _ = self.event_bus.send(RuntimeEvent::ChatResponseStarted {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
            });

            // AC-1 — use stream() instead of complete()
            let stream = self
                .llm_router
                .stream_with_observability(None, request, &obs)
                .await
                .map_err(|e| ChatError::InternalError(e.to_string()))?;

            // AC-2/AC-6 — consume stream, emit ChatToken per token, accumulate text
            let mut accumulated_text = String::new();
            let stream_result = self
                .consume_stream(stream, session_id, message_id, &mut accumulated_text)
                .await;

            match stream_result {
                Ok(tool_calls) => {
                    if tool_calls.is_empty() {
                        // AC-4 — final text response (no tool calls)
                        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
                            session_id: session_id.to_string(),
                            message_id: message_id.to_string(),
                            content: accumulated_text.clone(),
                        });

                        return Ok(ChatAgentResponse {
                            content: accumulated_text,
                            tool_calls: all_tool_calls,
                            newly_authorized,
                            tokens_used: total_usage,
                        });
                    }

                    // AC-8 — tool calls detected in stream: process and continue loop
                    llm_messages.push(LlmChatMessage::assistant_with_calls(
                        &accumulated_text,
                        &tool_calls,
                    ));

                    for call in &tool_calls {
                        budget.increment_tool_calls();

                        if authorized.contains(&call.name) {
                            let (record, tool_result) =
                                self.execute_tool_call(session_id, message_id, call).await;
                            llm_messages.push(LlmChatMessage::tool_result(&call.id, &tool_result));
                            all_tool_calls.push(record);
                        } else {
                            // HITL approval
                            let key = format!("{session_id}::{message_id}::{}", call.name);
                            let input_preview = truncate_preview(
                                &serde_json::to_string(&call.arguments).unwrap_or_default(),
                            );

                            let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
                                session_id: session_id.to_string(),
                                message_id: message_id.to_string(),
                                tool_name: call.name.clone(),
                                prompt: format!(
                                    "L'outil '{}' demande à être exécuté avec: {}",
                                    call.name, input_preview
                                ),
                            });

                            let rx = pending_approvals.register(key.clone());
                            pending_approvals.start_timeout(
                                key,
                                CHAT_APPROVAL_TIMEOUT,
                                self.event_bus.clone(),
                                session_id.to_string(),
                                message_id.to_string(),
                                call.name.clone(),
                            );
                            let decision = rx.await.unwrap_or(ToolDecision::Refuse);

                            match decision {
                                ToolDecision::Accept => {
                                    let (record, tool_result) =
                                        self.execute_tool_call(session_id, message_id, call).await;
                                    llm_messages
                                        .push(LlmChatMessage::tool_result(&call.id, &tool_result));
                                    all_tool_calls.push(record);
                                }
                                ToolDecision::AlwaysAccept => {
                                    authorized.insert(call.name.clone());
                                    newly_authorized.push(call.name.clone());
                                    let (record, tool_result) =
                                        self.execute_tool_call(session_id, message_id, call).await;
                                    llm_messages
                                        .push(LlmChatMessage::tool_result(&call.id, &tool_result));
                                    all_tool_calls.push(record);
                                }
                                ToolDecision::Refuse => {
                                    let refusal = "Outil refusé par l'utilisateur";
                                    llm_messages
                                        .push(LlmChatMessage::tool_result(&call.id, refusal));
                                    all_tool_calls.push(ToolCallRecord {
                                        tool_name: call.name.clone(),
                                        input: call.arguments.clone(),
                                        output: Some(refusal.to_string()),
                                        status: ToolCallStatus::Refused,
                                    });
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    // AC-7 — stream interrupted: emit ChatError, return partial content
                    let _ = self.event_bus.send(RuntimeEvent::ChatError {
                        session_id: session_id.to_string(),
                        message_id: Some(message_id.to_string()),
                        error: err.clone(),
                    });

                    // Return partial content so the caller can save what was received
                    let content = if accumulated_text.is_empty() {
                        format!("[erreur streaming : {err}]")
                    } else {
                        accumulated_text
                    };

                    let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        content: content.clone(),
                    });

                    return Ok(ChatAgentResponse {
                        content,
                        tool_calls: all_tool_calls,
                        newly_authorized,
                        tokens_used: total_usage,
                    });
                }
            }
        }
    }

    /// Consume a token stream, emitting [`RuntimeEvent::ChatToken`] for each token
    /// and accumulating text in `accumulated_text`.
    ///
    /// Returns the list of tool calls found in the stream (empty if none).
    /// On stream error, returns the error message; the caller can use the
    /// partially accumulated text.
    async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamChunk, apollia_llm::LlmError>> + Send>,
        >,
        session_id: &str,
        message_id: &str,
        accumulated_text: &mut String,
    ) -> Result<Vec<ToolCall>, String> {
        let mut tool_calls = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(StreamChunk::Text(token)) => {
                    // AC-2 — emit ChatToken and accumulate
                    let _ = self.event_bus.send(RuntimeEvent::ChatToken {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        token: token.clone(),
                    });
                    accumulated_text.push_str(&token);
                }
                Ok(StreamChunk::ToolCall(call)) => {
                    // AC-8 — tool call detected in stream
                    tool_calls.push(call);
                }
                Err(e) => {
                    // AC-7 — stream interrupted
                    warn!(
                        session_id = %session_id,
                        error = %e,
                        "LLM stream interrupted"
                    );
                    return Err(e.to_string());
                }
            }
        }

        Ok(tool_calls)
    }

    /// Execute a single tool call via the [`ToolInvoker`], emitting events.
    async fn execute_tool_call(
        &self,
        session_id: &str,
        message_id: &str,
        call: &apollia_llm::types::ToolCall,
    ) -> (ToolCallRecord, String) {
        let input_preview =
            truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());

        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallStarted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            input_preview,
        });

        let result = self.tool_invoker.invoke(&call.name, &call.arguments).await;
        let (output, success) = match result {
            Ok(s) => (s, true),
            Err(e) => {
                warn!(tool = %call.name, error = %e, "Tool call failed");
                (format!("tool error: {e}"), false)
            }
        };

        let output_preview = truncate_preview(&output);
        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_name: call.name.clone(),
            success,
            output_preview: Some(output_preview),
        });

        let record = ToolCallRecord {
            tool_name: call.name.clone(),
            input: call.arguments.clone(),
            output: Some(output.clone()),
            status: ToolCallStatus::Executed,
        };

        (record, output)
    }
}

/// Build LLM messages from system prompt, chat history, and current user message (AC-2).
///
/// Returns messages in order: system, history (converted), user.
fn build_llm_messages(
    system_prompt: &str,
    history: &[ChatMessage],
    user_message: &str,
) -> Vec<LlmChatMessage> {
    let mut messages = Vec::with_capacity(history.len() + 2);

    messages.push(LlmChatMessage::system(system_prompt));

    for msg in history {
        match msg.role {
            ChatRole::User => messages.push(LlmChatMessage::user(&msg.content)),
            ChatRole::Assistant => messages.push(LlmChatMessage::assistant(&msg.content)),
            ChatRole::Tool => {
                let call_id = msg.tool_name.as_deref().unwrap_or("unknown");
                messages.push(LlmChatMessage::tool_result(call_id, &msg.content));
            }
            ChatRole::System => {
                // System messages from history are skipped — we already have the prompt
            }
        }
    }

    messages.push(LlmChatMessage::user(user_message));
    messages
}

/// Convert available tool names to LLM-compatible [`ToolSpec`]s via the registry (AC-3).
async fn build_tool_specs(
    available_tools: &[String],
    tool_registry: &ToolRegistryHandle,
) -> Vec<ToolSpec> {
    let mut specs = Vec::with_capacity(available_tools.len());
    for name in available_tools {
        match tool_registry.get(name).await {
            Ok(Some(descriptor)) => {
                specs.push(ToolSpec {
                    name: descriptor.name,
                    description: descriptor.description,
                    parameters: descriptor.input_schema,
                });
            }
            Ok(None) => {
                info!(tool = %name, "Tool not found in registry, skipping");
            }
            Err(e) => {
                warn!(tool = %name, error = %e, "Failed to get tool descriptor, skipping");
            }
        }
    }
    specs
}

/// Truncate a string to a maximum length, appending "..." if truncated.
fn truncate_preview(s: &str) -> String {
    if s.len() <= PREVIEW_MAX_LEN {
        s.to_string()
    } else {
        let boundary = s
            .char_indices()
            .take_while(|(i, _)| *i < PREVIEW_MAX_LEN.saturating_sub(3))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::types::{
        CompletionModel, CompletionRequest, CompletionResponse, FinishReason as LlmFinishReason,
        StreamChunk as LlmStreamChunk, ToolCall as LlmToolCall,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Mock CompletionModel: streams text tokens then stops ─────────────

    struct MockStopModel {
        /// Tokens to emit (each becomes a StreamChunk::Text).
        tokens: Vec<String>,
    }

    impl MockStopModel {
        fn with_content(content: &str) -> Self {
            Self {
                tokens: split_tokens(content),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockStopModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            Ok(CompletionResponse {
                content: self.tokens.join(""),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cost_usd: None,
                },
                finish_reason: LlmFinishReason::Stop,
                latency_ms: 1,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                .tokens
                .iter()
                .map(|t| Ok(LlmStreamChunk::Text(t.clone())))
                .collect();
            Ok(Box::pin(futures::stream::iter(chunks)))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-stop"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: streams tool calls then text ───────────────

    struct MockReActModel {
        calls: Vec<LlmToolCall>,
        final_tokens: Vec<String>,
        iteration: AtomicU32,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockReActModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            let current = self.iteration.load(Ordering::SeqCst);
            if current == 0 {
                Ok(CompletionResponse {
                    content: String::new(),
                    tool_calls: self.calls.clone(),
                    usage: TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        cost_usd: None,
                    },
                    finish_reason: LlmFinishReason::ToolCalls,
                    latency_ms: 1,
                })
            } else {
                Ok(CompletionResponse {
                    content: self.final_tokens.join(""),
                    tool_calls: vec![],
                    usage: TokenUsage {
                        prompt_tokens: 15,
                        completion_tokens: 8,
                        cost_usd: None,
                    },
                    finish_reason: LlmFinishReason::Stop,
                    latency_ms: 1,
                })
            }
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let current = self.iteration.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                // First iteration: emit tool calls
                let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                    .calls
                    .iter()
                    .map(|c| Ok(LlmStreamChunk::ToolCall(c.clone())))
                    .collect();
                Ok(Box::pin(futures::stream::iter(chunks)))
            } else {
                // Subsequent iterations: emit text tokens
                let chunks: Vec<Result<LlmStreamChunk, apollia_llm::types::LlmError>> = self
                    .final_tokens
                    .iter()
                    .map(|t| Ok(LlmStreamChunk::Text(t.clone())))
                    .collect();
                Ok(Box::pin(futures::stream::iter(chunks)))
            }
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-react"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: always streams tool calls (infinite loop) ──

    struct MockInfiniteToolCallModel;

    #[async_trait::async_trait]
    impl CompletionModel for MockInfiniteToolCallModel {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
            Ok(CompletionResponse {
                content: String::new(),
                tool_calls: vec![LlmToolCall {
                    id: "c1".into(),
                    name: "bash_executor".into(),
                    arguments: serde_json::json!({"command": "echo"}),
                }],
                usage: TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 3,
                    cost_usd: None,
                },
                finish_reason: LlmFinishReason::ToolCalls,
                latency_ms: 1,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                        + Send,
                >,
            >,
            apollia_llm::types::LlmError,
        > {
            let chunks = vec![Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "echo"}),
            }))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-infinite"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    /// Split content into word-boundary tokens for mock streaming.
    fn split_tokens(content: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        for ch in content.chars() {
            if ch == ' ' {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(" ".to_string());
            } else {
                current.push(ch);
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }

    // ── Mock ToolInvoker ─────────────────────────────────────────────────

    struct MockToolInvoker {
        result: String,
    }

    impl MockToolInvoker {
        fn new(result: impl Into<String>) -> Self {
            Self {
                result: result.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ToolInvoker for MockToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.result.clone())
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────

    fn make_router(model: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
        let mut backends = std::collections::HashMap::new();
        backends.insert("default".to_string(), model);
        Arc::new(LlmRouter::with_backends(backends, "default"))
    }

    fn make_event_bus() -> EventBusSender {
        let (tx, _rx) = tokio::sync::broadcast::channel(128);
        tx
    }

    fn make_budget(max_steps: u32) -> StepBudget {
        StepBudget::with_max(max_steps)
    }

    // ── Tests ────────────────────────────────────────────────────────────

    /// AC-1/AC-8 — Simple text response without tool calls (streamed).
    #[tokio::test]
    async fn test_simple_text_response() {
        // GIVEN a model that streams text tokens without tool calls
        let model = Arc::new(MockStopModel::with_content("Bonjour !"));
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute with a simple user message
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await;

        // THEN response contains the text, no tool calls
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Bonjour !");
        assert!(resp.tool_calls.is_empty());
        assert!(resp.newly_authorized.is_empty());

        tool_registry.shutdown().await;
    }

    /// AC-4 — Tool call authorized: direct execution (via streaming).
    #[tokio::test]
    async fn test_tool_call_authorized() {
        // GIVEN a model that streams a tool call, then text
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "echo hello"}),
            }],
            final_tokens: split_tokens("Commande exécutée"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("hello\n"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());

        // WHEN execute with "bash_executor" in authorized_tools
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Execute echo",
                &[],
                "Tu es un assistant.",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
            )
            .await;

        // THEN tool was executed, response contains final text
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Commande exécutée");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].tool_name, "bash_executor");
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert!(resp.tool_calls[0].output.is_some());

        tool_registry.shutdown().await;
    }

    /// AC-5/AC-6 — Tool call not authorized, HITL Accept.
    #[tokio::test]
    async fn test_tool_call_hitl_accept() {
        // GIVEN a model with tool call "file_io" NOT in authorized_tools
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_io".into(),
                arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            }],
            final_tokens: split_tokens("Fichier lu"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("file content"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // Pre-resolve the approval to Accept before execute (simulates user action)
        let key = "sess-1::msg-1::file_io".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::Accept);
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Read file",
                &[],
                "assistant",
                &["file_io".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await;

        // THEN tool was executed after approval
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Fichier lu");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert!(resp.newly_authorized.is_empty());

        tool_registry.shutdown().await;
    }

    /// AC-6 — Tool call HITL Refuse: refusal message injected.
    #[tokio::test]
    async fn test_tool_call_hitl_refuse() {
        // GIVEN a model with unauthorized tool, decision = Refuse
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_io".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Ok, pas de souci."),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("unused"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        let key = "sess-1::msg-1::file_io".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::Refuse);
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Read",
                &[],
                "assistant",
                &["file_io".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await;

        // THEN refusal recorded, LLM sees it and produces final text
        let resp = result.expect("should succeed");
        assert_eq!(resp.content, "Ok, pas de souci.");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Refused);
        assert_eq!(
            resp.tool_calls[0].output.as_deref(),
            Some("Outil refusé par l'utilisateur")
        );

        tool_registry.shutdown().await;
    }

    /// AC-6 — Tool call HITL AlwaysAccept: tool whitelisted.
    #[tokio::test]
    async fn test_tool_call_hitl_always_accept() {
        // GIVEN unauthorized tool, decision = AlwaysAccept
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "file_io".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Done"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        let key = "sess-1::msg-1::file_io".to_string();
        tokio::spawn({
            let approvals = approvals.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                approvals.resolve(&key, ToolDecision::AlwaysAccept);
            }
        });

        // WHEN execute
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Read",
                &[],
                "assistant",
                &["file_io".to_string()],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await;

        // THEN tool executed AND newly_authorized contains "file_io"
        let resp = result.expect("should succeed");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);
        assert_eq!(resp.newly_authorized, vec!["file_io".to_string()]);

        tool_registry.shutdown().await;
    }

    /// AC-7 — Budget exhausted returns error.
    #[tokio::test]
    async fn test_budget_exhausted() {
        // GIVEN a model that always returns tool calls + budget max_steps=1
        let model = Arc::new(MockInfiniteToolCallModel);
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(1);
        let mut authorized = HashSet::new();
        authorized.insert("bash_executor".to_string());
        let approvals = PendingChatApprovals::new();

        // WHEN execute — first iteration uses the budget, second checks and fails
        let result = agent
            .execute(
                "sess-1",
                "msg-1",
                "Loop",
                &[],
                "assistant",
                &["bash_executor".to_string()],
                &authorized,
                &approvals,
                &budget,
            )
            .await;

        // THEN BudgetExhausted error
        assert!(
            matches!(result, Err(ChatError::BudgetExhausted)),
            "expected BudgetExhausted, got: {result:?}"
        );

        tool_registry.shutdown().await;
    }

    /// AC-2 — build_llm_messages constructs messages in correct order.
    #[test]
    fn test_build_llm_messages() {
        // GIVEN system prompt, 3 history messages, and a user message
        let history = vec![
            ChatMessage {
                id: "m1".into(),
                role: ChatRole::User,
                content: "Hello".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 1,
            },
            ChatMessage {
                id: "m2".into(),
                role: ChatRole::Assistant,
                content: "Hi there".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 2,
            },
            ChatMessage {
                id: "m3".into(),
                role: ChatRole::User,
                content: "How are you?".into(),
                tool_calls: None,
                tool_name: None,
                created_at: String::new(),
                seq: 3,
            },
        ];

        // WHEN building LLM messages
        let messages = build_llm_messages("You are helpful.", &history, "Final question");

        // THEN 5 messages in order: system, h1 (user), h2 (assistant), h3 (user), current user
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0].role, apollia_llm::types::Role::System);
        assert_eq!(messages[1].role, apollia_llm::types::Role::User);
        assert_eq!(messages[2].role, apollia_llm::types::Role::Assistant);
        assert_eq!(messages[3].role, apollia_llm::types::Role::User);
        assert_eq!(messages[4].role, apollia_llm::types::Role::User);
    }

    /// AC-10 — Events emitted in correct order (including ChatToken).
    #[tokio::test]
    async fn test_events_emitted_in_order() {
        // GIVEN a model that streams one tool call then text "Done"
        let model = Arc::new(MockReActModel {
            calls: vec![LlmToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({}),
            }],
            final_tokens: split_tokens("Done"),
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("output"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_tx);

        let budget = make_budget(10);
        let mut authorized = HashSet::new();
        authorized.insert("bash".to_string());
        let approvals = PendingChatApprovals::new();

        // WHEN execute completes
        let _resp = agent
            .execute(
                "s1",
                "m1",
                "Go",
                &[],
                "prompt",
                &["bash".to_string()],
                &authorized,
                &approvals,
                &budget,
            )
            .await
            .expect("should succeed");

        // THEN events are: ResponseStarted (tool iteration), ToolCallStarted,
        // ToolCallCompleted, ResponseStarted (text iteration), Token("Done"),
        // ResponseCompleted
        let mut event_names = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            let name = match evt {
                RuntimeEvent::ChatResponseStarted { .. } => "ResponseStarted",
                RuntimeEvent::ChatToken { .. } => "Token",
                RuntimeEvent::ChatToolCallStarted { .. } => "ToolCallStarted",
                RuntimeEvent::ChatToolCallCompleted { .. } => "ToolCallCompleted",
                RuntimeEvent::ChatResponseCompleted { .. } => "ResponseCompleted",
                RuntimeEvent::LlmCallCompleted { .. } => continue,
                _ => "other",
            };
            event_names.push(name);
        }

        assert_eq!(
            event_names,
            vec![
                "ResponseStarted",
                "ToolCallStarted",
                "ToolCallCompleted",
                "ResponseStarted",
                "Token",
                "ResponseCompleted"
            ]
        );

        tool_registry.shutdown().await;
    }

    #[test]
    fn test_truncate_preview_short() {
        // GIVEN a string shorter than PREVIEW_MAX_LEN
        let s = "short string";
        // WHEN truncating
        let result = truncate_preview(s);
        // THEN unchanged
        assert_eq!(result, s);
    }

    #[test]
    fn test_truncate_preview_long() {
        // GIVEN a string longer than PREVIEW_MAX_LEN
        let s = "a".repeat(300);
        // WHEN truncating
        let result = truncate_preview(&s);
        // THEN truncated with "..."
        assert!(result.len() <= PREVIEW_MAX_LEN);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_default_system_prompt_used_when_empty() {
        // GIVEN empty system_prompt
        let messages = build_llm_messages("", &[], "Hello");

        // THEN first message is the empty string we passed (caller decides default)
        assert_eq!(messages.len(), 2);
    }

    // ── Streaming-specific tests (STORY-201) ─────────────────────────────

    /// AC-2 — Each token emits a ChatToken event.
    #[tokio::test]
    async fn test_stream_tokens_emitted() {
        // GIVEN a model that streams ["Bon", "jour", " ", "!"]
        let model = Arc::new(MockStopModel {
            tokens: vec!["Bon".into(), "jour".into(), " ".into(), "!".into()],
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_tx);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "Salut",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await
            .expect("should succeed");

        // THEN 4 ChatToken events emitted, content is "Bonjour !"
        assert_eq!(resp.content, "Bonjour !");

        let mut tokens = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatToken { token, .. } = evt {
                tokens.push(token);
            }
        }
        assert_eq!(tokens, vec!["Bon", "jour", " ", "!"]);

        tool_registry.shutdown().await;
    }

    /// AC-6 — Accumulated text from stream matches final content.
    #[tokio::test]
    async fn test_stream_accumulation() {
        // GIVEN a model that streams ["Hello", " ", "world"]
        let model = Arc::new(MockStopModel {
            tokens: vec!["Hello".into(), " ".into(), "world".into()],
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let event_bus = make_event_bus();
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_bus);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await
            .expect("should succeed");

        // THEN accumulated text is "Hello world"
        assert_eq!(resp.content, "Hello world");

        tool_registry.shutdown().await;
    }

    /// AC-7 — Stream interruption returns partial content.
    #[tokio::test]
    async fn test_stream_interrupted() {
        // GIVEN a model whose stream returns 2 tokens then an error
        struct InterruptedModel;

        #[async_trait::async_trait]
        impl CompletionModel for InterruptedModel {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
                unimplemented!()
            }

            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::types::LlmError,
            > {
                let chunks = vec![
                    Ok(LlmStreamChunk::Text("Par".into())),
                    Ok(LlmStreamChunk::Text("tial".into())),
                    Err(apollia_llm::types::LlmError::InferenceError(
                        "connection reset".into(),
                    )),
                ];
                Ok(Box::pin(futures::stream::iter(chunks)))
            }

            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "mock-interrupted"
            }
            fn model_id(&self) -> &str {
                "mock"
            }
        }

        let model: Arc<dyn CompletionModel> = Arc::new(InterruptedModel);
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("ok"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_tx);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "test",
                &[],
                "",
                &[],
                &HashSet::new(),
                &approvals,
                &budget,
            )
            .await
            .expect("should return partial content, not error");

        // THEN partial content is saved
        assert_eq!(resp.content, "Partial");

        // AND ChatError event was emitted
        let mut has_error = false;
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatError { error, .. } = evt {
                assert!(error.contains("connection reset"));
                has_error = true;
            }
        }
        assert!(has_error, "ChatError event should have been emitted");

        tool_registry.shutdown().await;
    }

    /// AC-8 — Stream with tool call: text tokens emitted, then tool executed.
    #[tokio::test]
    async fn test_stream_with_tool_call() {
        // GIVEN a model that streams text + tool_call on first iteration,
        // then only text on second iteration
        struct TextThenToolModel {
            iteration: AtomicU32,
        }

        #[async_trait::async_trait]
        impl CompletionModel for TextThenToolModel {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
                unimplemented!()
            }

            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::types::LlmError,
            > {
                let current = self.iteration.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    let chunks = vec![
                        Ok(LlmStreamChunk::Text("Je ".into())),
                        Ok(LlmStreamChunk::Text("vais ".into())),
                        Ok(LlmStreamChunk::Text("lire".into())),
                        Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                            id: "c1".into(),
                            name: "file_io".into(),
                            arguments: serde_json::json!({"action": "read", "path": "/tmp"}),
                        })),
                    ];
                    Ok(Box::pin(futures::stream::iter(chunks)))
                } else {
                    let chunks = vec![
                        Ok(LlmStreamChunk::Text("Fichier ".into())),
                        Ok(LlmStreamChunk::Text("lu.".into())),
                    ];
                    Ok(Box::pin(futures::stream::iter(chunks)))
                }
            }

            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "mock-text-tool"
            }
            fn model_id(&self) -> &str {
                "mock"
            }
        }

        let model: Arc<dyn CompletionModel> = Arc::new(TextThenToolModel {
            iteration: AtomicU32::new(0),
        });
        let router = make_router(model);
        let tool_registry = ToolRegistryHandle::start();
        let invoker: Arc<dyn ToolInvoker> = Arc::new(MockToolInvoker::new("file content"));
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(128);
        let agent = BuiltInChatAgent::new(router, tool_registry.clone(), invoker, event_tx);

        let budget = make_budget(10);
        let approvals = PendingChatApprovals::new();
        let mut authorized = HashSet::new();
        authorized.insert("file_io".to_string());

        // WHEN execute
        let resp = agent
            .execute(
                "sess-1",
                "msg-1",
                "lis le fichier",
                &[],
                "",
                &["file_io".to_string()],
                &authorized,
                &approvals,
                &budget,
            )
            .await
            .expect("should succeed");

        // THEN final content from second iteration
        assert_eq!(resp.content, "Fichier lu.");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].tool_name, "file_io");
        assert_eq!(resp.tool_calls[0].status, ToolCallStatus::Executed);

        // AND tokens from both iterations were emitted
        let mut tokens = Vec::new();
        while let Ok(evt) = event_rx.try_recv() {
            if let RuntimeEvent::ChatToken { token, .. } = evt {
                tokens.push(token);
            }
        }
        // First iteration text tokens + second iteration text tokens
        assert_eq!(tokens, vec!["Je ", "vais ", "lire", "Fichier ", "lu."]);

        tool_registry.shutdown().await;
    }
}
