//! `RewriteInput`: meta-LLM routine that rewrites terse user input into
//! clearer, more actionable prompts before sending to an agent.
//!
//! # Principles
//!
//! - **Reuses the user's default LLM backend** via [`LlmRouter::get(None)`]:
//!   never allocates a second model nor performs a cloud call the user
//!   has not already authorised.
//! - **Explicitly triggered**: the user must click the "Improve prompt"
//!   button. This is the third exception to Principle 6 (memory injection):
//!   the rewrite request includes the Work section of the user profile when
//!   filled, but only when the operator explicitly triggers it.
//! - **Graceful fallback, and it says which one**: any LLM error or timeout
//!   returns the original text unchanged, tagged with the [`RewriteFallback`]
//!   that produced it. A reasoning model that spends its whole budget inside a
//!   `<think>` block leaves nothing usable behind, and that is a different
//!   fallback from an engine that was never configured: the caller must be able
//!   to tell the operator which of the two happened.
//! - **No iteration drift**: the frontend always passes the first original
//!   input, never a prior rewrite, so successive rewrites don't compound.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::router::LlmRouter;
use crate::types::{ChatMessage, CompletionRequest, LlmError};

/// Upper bound on the LLM call; beyond that we return the original text.
///
/// Sized on [`MAX_OUTPUT_TOKENS`] rather than on the length of a rewrite: a
/// reasoning model emits its whole `<think>` block before the first word of the
/// answer, so the wall clock is set by the reasoning, not by the two sentences
/// the operator ends up seeing. A cap that fits the answer alone turns every
/// reasoning model into a silent fallback.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Generation budget for one rewrite.
///
/// A rewrite is one or two sentences, but a reasoning model (Qwen3, DeepSeek
/// R1, o1-style, ...) emits its `<think>` block first and only then the answer.
/// The budget covers both: cut too short, the response is nothing but an
/// unterminated reasoning block, which [`strip_reasoning_blocks`] reduces to an
/// empty string and the caller reports as "no rewrite happened".
const MAX_OUTPUT_TOKENS: u32 = 1024;

/// Work context extracted from the user profile's Work section.
/// Fields mirror `profile_schema.rs` keys under `ProfileSection::Work`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkContext {
    /// Industry sector (e.g. fintech, healthcare, e-commerce).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,
    /// Team size (e.g. solo, small team, large organization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_size: Option<String>,
    /// Tech stack (main languages, frameworks, tools).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tech_stack: Option<String>,
    /// Daily tools (e.g. Git, Notion, Slack).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_tools: Option<String>,
    /// Connected integrations (e.g. GitHub, Gmail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrations: Option<String>,
}

/// Request passed to [`rewrite_input`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteInputRequest {
    /// The original user input to rewrite (always the first input, never a
    /// prior rewrite).
    pub original_text: String,
    /// Work context from the user profile, if filled.
    #[serde(default)]
    pub work_context: Option<WorkContext>,
}

/// Why no rewrite happened.
///
/// One variant per place [`rewrite_input`] gives up, so the caller renders a
/// message that names the real cause instead of the one that happens to be
/// first in the list. A single boolean collapsed these onto "no engine
/// configured", which is false for two of the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RewriteFallback {
    /// The router holds no backend under its default name, so nothing was
    /// called. This is the only variant that means "configure an engine".
    NoBackend,
    /// The backend answered with an error, or the call passed
    /// [`CALL_TIMEOUT`]. The engine exists and was reached.
    CallFailed,
    /// The backend answered, and nothing was left once the reasoning blocks
    /// were removed. The engine exists, was reached, and produced text.
    EmptyAnswer,
}

/// Result returned to the caller. `fallback` is `None` when the LLM path
/// succeeded, and carries the reason when we fell back to returning the
/// original text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteInputResponse {
    pub rewritten_text: String,
    pub fallback: Option<RewriteFallback>,
}

/// Errors surfaced by [`rewrite_input`]. The caller should not abort on
/// these: the helper already returns the original text as fallback.
#[derive(Debug, thiserror::Error)]
pub enum RewriteInputError {
    #[error("LLM call failed: {0}")]
    Llm(#[from] LlmError),
    #[error("LLM returned an empty response")]
    EmptyResponse,
}

// ─────────────────────────────────────────────
// Prompt assembly
// ─────────────────────────────────────────────

const PROMPT_HEADER: &str = "\
You are a prompt improvement assistant inside Apollia OS. The user has typed \
a terse or ambiguous prompt and clicked \"Improve prompt\" to make it clearer \
before sending.

Your task: rewrite the user's input into a clearer, more specific, actionable \
request while staying faithful to their original intent. Do NOT invent tasks, \
tools, or context the user didn't mention. If the user says \"fix bug\", ask \
which bug or system, don't assume. If they mention a tool, use the exact tool \
name they gave.

Respond with ONLY the rewritten prompt text, no preamble, no quotes, no \
markdown. One or two sentences maximum.
";

fn build_work_context_block(ctx: &WorkContext) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(ref sector) = ctx.sector {
        if !sector.trim().is_empty() {
            lines.push(format!("- Sector: {}", sector.trim()));
        }
    }
    if let Some(ref team_size) = ctx.team_size {
        if !team_size.trim().is_empty() {
            lines.push(format!("- Team size: {}", team_size.trim()));
        }
    }
    if let Some(ref stack) = ctx.tech_stack {
        if !stack.trim().is_empty() {
            lines.push(format!("- Tech stack: {}", stack.trim()));
        }
    }
    if let Some(ref tools) = ctx.daily_tools {
        if !tools.trim().is_empty() {
            lines.push(format!("- Daily tools: {}", tools.trim()));
        }
    }
    if let Some(ref integrations) = ctx.integrations {
        if !integrations.trim().is_empty() {
            lines.push(format!("- Integrations: {}", integrations.trim()));
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(format!(
            "\n\n## User's work context\n{}\n\nUse this context to avoid inventing tools they don't use. If the rewrite mentions a tool, ensure it's from their daily tools or integrations above.",
            lines.join("\n")
        ))
    }
}

/// Removes `<think>...</think>` and `<reasoning>...</reasoning>` blocks from a
/// raw model response.
///
/// The composer receives this text verbatim, so a reasoning block that survives
/// here lands in the operator's input field in place of the rewrite. The local
/// engine is driven with `--reasoning-format none`, which keeps reasoning
/// inline in the content stream rather than in a separate field, so stripping
/// is the consumer's job.
///
/// Three shapes are handled:
/// - Paired block (`<think>...</think>Rewrite`): the block is dropped.
/// - Unterminated block: the response was cut off inside the reasoning, so
///   everything from the opening tag onward is dropped, leaving an empty string
///   that the caller treats as "no rewrite".
/// - Dangling closing tag (Qwen3 family): the chat template opens `<think>` in
///   the prompt itself, so the raw output starts with reasoning text and ends it
///   with a lone `</think>`. Everything up to that tag is dropped.
fn strip_reasoning_blocks(raw: &str) -> String {
    fn drop_block(text: &str, open: &str, close: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        loop {
            match (rest.find(open), rest.find(close)) {
                // Closing tag with no opening tag before it: the block was
                // opened by the chat template, not by the model.
                (None, Some(c)) => rest = &rest[c + close.len()..],
                (Some(o), Some(c)) if c < o => rest = &rest[c + close.len()..],
                (Some(o), _) => {
                    out.push_str(&rest[..o]);
                    let after_open = &rest[o + open.len()..];
                    match after_open.find(close) {
                        Some(end) => rest = &after_open[end + close.len()..],
                        // Unterminated block: nothing usable follows.
                        None => return out,
                    }
                }
                (None, None) => {
                    out.push_str(rest);
                    return out;
                }
            }
        }
    }
    let stripped = drop_block(raw, "<think>", "</think>");
    drop_block(&stripped, "<reasoning>", "</reasoning>")
}

fn build_system_prompt(req: &RewriteInputRequest) -> String {
    let mut prompt = PROMPT_HEADER.to_string();
    if let Some(ref ctx) = req.work_context {
        if let Some(work_block) = build_work_context_block(ctx) {
            prompt.push_str(&work_block);
        }
    }
    prompt
}

// ─────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────

/// Rewrite a user's input prompt to make it clearer and more actionable.
///
/// Always returns a `RewriteInputResponse`. On any LLM failure, timeout, or
/// empty response, the original text is returned unchanged, carrying the
/// [`RewriteFallback`] that says which of the three happened.
pub async fn rewrite_input(
    router: &Arc<LlmRouter>,
    request: RewriteInputRequest,
) -> RewriteInputResponse {
    // GIVEN: no backend configured
    let Some(backend) = router.get(None) else {
        // THEN: return original text, and say the engine is missing
        return RewriteInputResponse {
            rewritten_text: request.original_text,
            fallback: Some(RewriteFallback::NoBackend),
        };
    };

    let system = build_system_prompt(&request);
    let user = format!(
        "Original user input:\n\n{}\n\nRewrite this to be clearer and more actionable.",
        request.original_text.trim()
    );

    let llm_request = CompletionRequest {
        messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
        temperature: Some(0.3),
        max_tokens: Some(MAX_OUTPUT_TOKENS),
        ..Default::default()
    };

    // WHEN: LLM call with timeout
    let call = backend.complete(llm_request);
    let response = match tokio::time::timeout(CALL_TIMEOUT, call).await {
        Ok(Ok(r)) => r,
        _ => {
            // THEN: timeout or error, return original, and say the call failed
            return RewriteInputResponse {
                rewritten_text: request.original_text,
                fallback: Some(RewriteFallback::CallFailed),
            };
        }
    };

    // WHEN: the model prefixed its answer with a reasoning block
    // THEN: drop it, the composer must receive the rewrite only
    let stripped = strip_reasoning_blocks(&response.content);
    let rewritten = stripped.trim();
    // WHEN: empty response, including one that was nothing but reasoning
    if rewritten.is_empty() {
        // THEN: return original, and say the answer held nothing usable
        return RewriteInputResponse {
            rewritten_text: request.original_text,
            fallback: Some(RewriteFallback::EmptyAnswer),
        };
    }

    // THEN: success
    RewriteInputResponse {
        rewritten_text: rewritten.to_string(),
        fallback: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::pin::Pin;

    use futures::Stream;

    use crate::types::{CompletionResponse, FinishReason, StreamChunk, TokenUsage};

    /// Backend that answers every call with a fixed string.
    struct CannedBackend {
        content: String,
    }

    #[async_trait::async_trait]
    impl crate::types::CompletionModel for CannedBackend {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: self.content.clone(),
                tool_calls: vec![],
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
                engine_timings: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::once(async {
                Ok(StreamChunk::Text("unused".to_owned()))
            })))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "canned"
        }

        fn model_id(&self) -> &str {
            "canned"
        }
    }

    /// Backend that fails every call, the reachable-but-broken engine.
    struct FailingBackend;

    #[async_trait::async_trait]
    impl crate::types::CompletionModel for FailingBackend {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError(
                "upstream refused the call".to_owned(),
            ))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::InferenceError(
                "upstream refused the call".to_owned(),
            ))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "failing"
        }

        fn model_id(&self) -> &str {
            "failing"
        }
    }

    fn router_failing() -> Arc<LlmRouter> {
        let mut backends: HashMap<String, Arc<dyn crate::types::CompletionModel>> = HashMap::new();
        backends.insert("failing".to_owned(), Arc::new(FailingBackend));
        Arc::new(LlmRouter::with_backends(backends, "failing"))
    }

    fn router_answering(content: &str) -> Arc<LlmRouter> {
        let mut backends: HashMap<String, Arc<dyn crate::types::CompletionModel>> = HashMap::new();
        backends.insert(
            "canned".to_owned(),
            Arc::new(CannedBackend {
                content: content.to_owned(),
            }),
        );
        Arc::new(LlmRouter::with_backends(backends, "canned"))
    }

    #[test]
    fn test_strip_reasoning_blocks_paired_block() {
        // GIVEN: a response whose rewrite is preceded by a closed think block
        let raw = "<think>The user wants a bug fixed, ask which one.</think>\nWhich login bug should I fix?";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: only the rewrite is left
        assert_eq!(stripped.trim(), "Which login bug should I fix?");
        assert!(!stripped.contains("<think>"));
        assert!(!stripped.contains("</think>"));
    }

    #[test]
    fn test_strip_reasoning_blocks_dangling_close_tag() {
        // GIVEN: a Qwen3-style response opened by the chat template, so the
        // reasoning is terminated by a closing tag that has no opening tag
        let raw = "The request is terse, I should ask for the system.</think>Which system is failing to authenticate?";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: everything up to the closing tag is dropped
        assert_eq!(stripped.trim(), "Which system is failing to authenticate?");
        assert!(!stripped.contains("</think>"));
    }

    #[test]
    fn test_strip_reasoning_blocks_unterminated_block() {
        // GIVEN: a response cut off inside its reasoning block
        let raw = "<think>Let me consider what the user means by";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: nothing usable is left
        assert!(stripped.trim().is_empty());
    }

    #[test]
    fn test_strip_reasoning_blocks_reasoning_tag_variant() {
        // GIVEN: a response using <reasoning> instead of <think>
        let raw = "<reasoning>terse input</reasoning>Fix the CSV import crash in the billing job.";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: the block is dropped
        assert_eq!(
            stripped.trim(),
            "Fix the CSV import crash in the billing job."
        );
    }

    #[test]
    fn test_strip_reasoning_blocks_leaves_plain_text_untouched() {
        // GIVEN: a response from a model without reasoning
        let raw = "Fix the login bug that blocks authentication.";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: the text is returned verbatim
        assert_eq!(stripped, raw);
    }

    #[test]
    fn test_strip_reasoning_blocks_multibyte_text_around_the_tags() {
        // GIVEN: reasoning and rewrite that both carry multibyte characters,
        // the case where a byte-index slice would split a code point
        let raw = "Weighing the scope 🧠 of the request…</think>Fix the login bug that blocks café staff from signing in 🔐.";

        // WHEN: stripping reasoning
        let stripped = strip_reasoning_blocks(raw);

        // THEN: the rewrite survives intact and nothing panics
        assert_eq!(
            stripped,
            "Fix the login bug that blocks café staff from signing in 🔐."
        );
    }

    #[tokio::test]
    async fn test_rewrite_input_returns_rewrite_without_reasoning_block() {
        // GIVEN: a backend that answers with a reasoning block then the rewrite
        let router = router_answering(
            "<think>The user typed three words, I should ask for the scope.</think>\n\nFix the login bug that prevents users from authenticating.",
        );
        let request = RewriteInputRequest {
            original_text: "fix bug login".to_string(),
            work_context: None,
        };

        // WHEN: rewriting
        let response = rewrite_input(&router, request).await;

        // THEN: the composer receives the rewrite alone
        assert_eq!(
            response.rewritten_text,
            "Fix the login bug that prevents users from authenticating."
        );
        assert!(!response.rewritten_text.contains("<think>"));
        assert!(!response.rewritten_text.contains("</think>"));
        assert_eq!(response.fallback, None);
    }

    #[tokio::test]
    async fn test_rewrite_input_falls_back_when_answer_is_only_reasoning() {
        // GIVEN: a backend whose whole answer is an unterminated reasoning block
        let router = router_answering("<think>Let me think about what they mean by");
        let request = RewriteInputRequest {
            original_text: "fix bug login".to_string(),
            work_context: None,
        };

        // WHEN: rewriting
        let response = rewrite_input(&router, request).await;

        // THEN: the operator's text is handed back untouched, and the reason
        // says the engine answered, not that it is missing
        assert_eq!(response.rewritten_text, "fix bug login");
        assert_eq!(response.fallback, Some(RewriteFallback::EmptyAnswer));
    }

    #[tokio::test]
    async fn test_rewrite_input_reports_no_backend_when_the_router_is_empty() {
        // GIVEN: a router holding no backend at all
        let router = Arc::new(LlmRouter::empty());
        let request = RewriteInputRequest {
            original_text: "fix bug login".to_string(),
            work_context: None,
        };

        // WHEN: rewriting
        let response = rewrite_input(&router, request).await;

        // THEN: the fallback names the missing engine, the one case where
        // telling the operator to configure one is true
        assert_eq!(response.rewritten_text, "fix bug login");
        assert_eq!(response.fallback, Some(RewriteFallback::NoBackend));
    }

    #[tokio::test]
    async fn test_rewrite_input_reports_call_failed_when_the_backend_errors() {
        // GIVEN: a backend that exists and refuses the call
        let router = router_failing();
        let request = RewriteInputRequest {
            original_text: "fix bug login".to_string(),
            work_context: None,
        };

        // WHEN: rewriting
        let response = rewrite_input(&router, request).await;

        // THEN: the fallback says the call failed, not that no engine exists
        assert_eq!(response.rewritten_text, "fix bug login");
        assert_eq!(response.fallback, Some(RewriteFallback::CallFailed));
    }

    #[tokio::test]
    async fn test_the_three_fallbacks_are_distinguishable_from_one_another() {
        // GIVEN: the three ways the routine gives up, each on its own router
        let request = || RewriteInputRequest {
            original_text: "fix bug login".to_string(),
            work_context: None,
        };
        let no_backend = rewrite_input(&Arc::new(LlmRouter::empty()), request()).await;
        let call_failed = rewrite_input(&router_failing(), request()).await;
        let empty_answer = rewrite_input(&router_answering("<think>cut off here"), request()).await;

        // WHEN: the three reasons are collected
        let reasons = [
            no_backend.fallback,
            call_failed.fallback,
            empty_answer.fallback,
        ];

        // THEN: no two of them carry the same reason, so no single message can
        // stand for all three
        assert_eq!(reasons[0], Some(RewriteFallback::NoBackend));
        assert_eq!(reasons[1], Some(RewriteFallback::CallFailed));
        assert_eq!(reasons[2], Some(RewriteFallback::EmptyAnswer));
        assert_ne!(reasons[0], reasons[1]);
        assert_ne!(reasons[1], reasons[2]);
        assert_ne!(reasons[0], reasons[2]);
    }

    #[test]
    fn test_fallback_serialises_to_the_token_the_composer_switches_on() {
        // GIVEN: the three fallbacks and the success case
        let cases = [
            (Some(RewriteFallback::NoBackend), "\"noBackend\""),
            (Some(RewriteFallback::CallFailed), "\"callFailed\""),
            (Some(RewriteFallback::EmptyAnswer), "\"emptyAnswer\""),
            (None, "null"),
        ];

        // WHEN: each is serialised the way the IPC layer sends it
        // THEN: the wire token is the one the composer branches on
        for (fallback, expected) in cases {
            let json = serde_json::to_string(&fallback).expect("fallback serialises");
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_build_work_context_block_empty() {
        // GIVEN: empty work context
        let ctx = WorkContext::default();

        // WHEN: building work context block
        let result = build_work_context_block(&ctx);

        // THEN: returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_build_work_context_block_filled() {
        // GIVEN: filled work context
        let ctx = WorkContext {
            sector: Some("fintech".to_string()),
            team_size: Some("small team".to_string()),
            tech_stack: Some("Rust, Svelte, SQLite".to_string()),
            daily_tools: Some("Git, Notion".to_string()),
            integrations: Some("GitHub, Gmail".to_string()),
        };

        // WHEN: building work context block
        let result = build_work_context_block(&ctx);

        // THEN: returns formatted block
        assert!(result.is_some());
        let block = result.unwrap();
        assert!(block.contains("Sector: fintech"));
        assert!(block.contains("Tech stack: Rust, Svelte, SQLite"));
        assert!(block.contains("Daily tools: Git, Notion"));
        assert!(block.contains("Integrations: GitHub, Gmail"));
    }

    #[test]
    fn test_build_work_context_block_partial() {
        // GIVEN: partial work context (only sector and tools)
        let ctx = WorkContext {
            sector: Some("healthcare".to_string()),
            team_size: None,
            tech_stack: None,
            daily_tools: Some("Slack, Jira".to_string()),
            integrations: None,
        };

        // WHEN: building work context block
        let result = build_work_context_block(&ctx);

        // THEN: returns block with only filled fields
        assert!(result.is_some());
        let block = result.unwrap();
        assert!(block.contains("Sector: healthcare"));
        assert!(block.contains("Daily tools: Slack, Jira"));
        assert!(!block.contains("Tech stack"));
        assert!(!block.contains("Integrations"));
    }

    #[test]
    fn test_build_system_prompt_no_context() {
        // GIVEN: request with no work context
        let req = RewriteInputRequest {
            original_text: "fix bug".to_string(),
            work_context: None,
        };

        // WHEN: building system prompt
        let prompt = build_system_prompt(&req);

        // THEN: contains header only
        assert!(prompt.contains("prompt improvement assistant"));
        assert!(!prompt.contains("User's work context"));
    }

    #[test]
    fn test_build_system_prompt_with_context() {
        // GIVEN: request with work context
        let req = RewriteInputRequest {
            original_text: "fix bug".to_string(),
            work_context: Some(WorkContext {
                sector: Some("fintech".to_string()),
                tech_stack: Some("Python, Django".to_string()),
                ..Default::default()
            }),
        };

        // WHEN: building system prompt
        let prompt = build_system_prompt(&req);

        // THEN: contains header and work context
        assert!(prompt.contains("prompt improvement assistant"));
        assert!(prompt.contains("User's work context"));
        assert!(prompt.contains("Sector: fintech"));
        assert!(prompt.contains("Tech stack: Python, Django"));
    }
}
