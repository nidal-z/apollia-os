//! Every consent answered and every call refused by the policy or by a rule
//! is written to the journal, with neither the call's argument nor anything
//! the tool returned.

use super::*;
use crate::audit_journal::{AuditJournalHandle, JournalEntry, JournalEntryKind};
use crate::governance::{
    governance_run_id, DecisionReason, GovernanceHandle, GovernanceState, PolicyRefusals,
};
use crate::test_support::poll_until;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use apollia_permissions::{PermissionScope, PrefixRule, RuleAction};
use std::sync::atomic::{AtomicU32, Ordering};

const TOOL: &str = "file_write";
const ARGUMENT: &str = "SECRET-ARGUMENT-42";
const OUTPUT: &str = "SECRET-OUTPUT-43";
const REASON: &str = "SECRET-REASON-44";
const PROMPT_WAIT: Duration = Duration::from_secs(5);

/// Streams one call to [`TOOL`] carrying [`ARGUMENT`], then final text.
struct OneCallThenText {
    iteration: AtomicU32,
    tool: &'static str,
}

#[async_trait::async_trait]
impl CompletionModel for OneCallThenText {
    async fn complete(
        &self,
        _req: CompletionRequest,
    ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
        Err(apollia_llm::types::LlmError::InferenceError(
            "streaming path only".into(),
        ))
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
        let chunk = if self.iteration.fetch_add(1, Ordering::SeqCst) == 0 {
            LlmStreamChunk::ToolCall(LlmToolCall {
                id: "c0".into(),
                name: self.tool.into(),
                arguments: serde_json::json!({ "path": ARGUMENT, "content": ARGUMENT }),
            })
        } else {
            LlmStreamChunk::Text("done".to_string())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-one-call"
    }
    fn model_id(&self) -> &str {
        "mock"
    }
}

struct OutputInvoker;

#[async_trait::async_trait]
impl ToolInvoker for OutputInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok(OUTPUT.to_string())
    }
}

/// Runs one exchange where the model calls [`TOOL`] once under `state`, the
/// operator answering `answer` if a prompt appears, and returns what the
/// journal holds for the agent.
async fn run_exchange(state: GovernanceState, answer: Option<ToolDecision>) -> Vec<JournalEntry> {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .expect("journal");
    let entries = run_exchange_into(&journal, state, answer).await;
    journal.shutdown().await;
    entries
}

/// Runs the same exchange, the model calling `tool` instead of [`TOOL`].
async fn run_exchange_with(tool: &'static str, state: GovernanceState) -> Vec<JournalEntry> {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .expect("journal");
    let entries = run_exchange_calling(&journal, tool, state, None).await;
    journal.shutdown().await;
    entries
}

/// Runs the same exchange against `journal`, which stays open so the caller
/// can verify it.
async fn run_exchange_into(
    journal: &AuditJournalHandle,
    state: GovernanceState,
    answer: Option<ToolDecision>,
) -> Vec<JournalEntry> {
    run_exchange_calling(journal, TOOL, state, answer).await
}

/// Runs the exchange against `journal`, the model calling `tool`.
async fn run_exchange_calling(
    journal: &AuditJournalHandle,
    tool: &'static str,
    state: GovernanceState,
    answer: Option<ToolDecision>,
) -> Vec<JournalEntry> {
    let governance = GovernanceHandle::detached(state, Some(journal.clone()));
    let model: Arc<dyn CompletionModel> = Arc::new(OneCallThenText {
        iteration: AtomicU32::new(0),
        tool,
    });
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (event_bus, _events) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    super::governed_test_support::register_writing(&tool_registry, tool).await;
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry: tool_registry.clone(),
        tool_invoker: Arc::new(OutputInvoker),
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    })
    .with_governance(Some(governance));
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let operator = tokio::spawn({
        let approvals = approvals.clone();
        async move {
            if let Some(answer) = answer {
                poll_until(PROMPT_WAIT, || {
                    approvals.resolve("sess-1::msg-1::c0", answer.clone())
                })
                .await;
            }
        }
    });
    agent
        .execute(
            "sess-1",
            "msg-1",
            &RunId::new(),
            "write it",
            &[],
            "assistant",
            &[tool.to_string()],
            &HashSet::new(),
            &approvals,
            &budget,
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
            None,
            None,
            None,
            None,
            CancellationToken::new(),
        )
        .await
        .expect("the exchange completes");
    operator.await.expect("operator joins");
    tool_registry.shutdown().await;
    journal
        .query_run(&governance_run_id(
            crate::chat::manager::APOLLIA_CHAT_AGENT_ID,
        ))
        .await
}

/// Asserts that no entry carries the call's argument, the tool's output, or
/// the reason typed with a refusal.
fn assert_no_content(entries: &[JournalEntry]) {
    for entry in entries {
        let payload = entry.payload.to_string();
        for secret in [ARGUMENT, OUTPUT, REASON] {
            assert!(
                !payload.contains(secret),
                "{} carries {secret}: {payload}",
                entry.kind.tag()
            );
        }
    }
}

// Each answer to a consent prompt is journaled as it took effect.
#[tokio::test]
async fn test_each_answer_is_journaled_without_content() {
    for (decision, expected) in [
        (ToolDecision::Accept, "once"),
        (ToolDecision::AlwaysAccept, "always"),
        (
            ToolDecision::Refuse {
                reason: Some(REASON.into()),
            },
            "refuse",
        ),
    ] {
        // GIVEN a writing tool nothing grants, and the operator answering
        // `decision` to the prompt
        // WHEN the model calls it with an argument
        let entries = run_exchange(GovernanceState::default(), Some(decision)).await;

        // THEN one consent entry records the answer, the tool and its family
        let consents: Vec<&JournalEntry> = entries
            .iter()
            .filter(|e| e.kind == JournalEntryKind::ConsentAnswered)
            .collect();
        assert_eq!(consents.len(), 1, "{expected}: {entries:?}");
        assert_eq!(consents[0].payload["answer"], expected);
        assert_eq!(consents[0].payload["access_id"], TOOL);
        assert_eq!(consents[0].payload["family"], "write_or_delete");
        // AND no entry carries the argument, the output or the reason
        assert_no_content(&entries);
    }
}

// A call a rule refuses is journaled as refused by that rule.
#[tokio::test]
async fn test_rule_refusal_is_journaled_without_content() {
    // GIVEN a deny rule on the tool
    let state = GovernanceState {
        rules: vec![PrefixRule {
            id: 9,
            tool_name: TOOL.into(),
            action: RuleAction::Deny,
            scope: PermissionScope::Global,
            ..PrefixRule::default()
        }],
        ..GovernanceState::default()
    };

    // WHEN the model calls it
    let entries = run_exchange(state, None).await;

    // THEN the journal records the refusal and the rule, nothing else
    let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(kinds, vec![JournalEntryKind::RuleRefused]);
    assert_eq!(entries[0].payload["rule_id"], 9);
    assert_no_content(&entries);
}

// A call the policy refuses is journaled as refused by the policy.
#[tokio::test]
async fn test_policy_refusal_is_journaled_without_content() {
    // GIVEN a policy that denies the tool
    let state = GovernanceState {
        policy: PolicyRefusals {
            denied: [TOOL.to_string()].into_iter().collect(),
            ..PolicyRefusals::default()
        },
        ..GovernanceState::default()
    };

    // WHEN the model calls it
    let entries = run_exchange(state, None).await;

    // THEN the journal records the refusal by the policy, nothing else
    let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(kinds, vec![JournalEntryKind::PolicyRefused]);
    assert_eq!(entries[0].payload["access_id"], TOOL);
    assert_no_content(&entries);
}

// The signed journal is where a decision is read: each of the three answers
// lands there signed, and the journal as a whole still verifies.
#[tokio::test]
async fn test_each_answer_reaches_the_signed_journal_and_the_chain_holds() {
    // GIVEN a journal signed with a key
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = AuditJournalHandle::open_with_key_bytes(
        &dir.path().join("audit_journal.db"),
        b"0123456789abcdef0123456789abcdef".to_vec(),
    )
    .await
    .expect("signed journal");

    // WHEN the operator answers once, always, then refuses, on three calls
    let mut answers = Vec::new();
    for decision in [
        ToolDecision::Accept,
        ToolDecision::AlwaysAccept,
        ToolDecision::Refuse {
            reason: Some(REASON.into()),
        },
    ] {
        let entries = run_exchange_into(&journal, GovernanceState::default(), Some(decision)).await;
        answers = entries
            .into_iter()
            .filter(|e| e.kind == JournalEntryKind::ConsentAnswered)
            .collect();
    }

    // THEN the journal holds the three answers, in the order they were given
    let given: Vec<&str> = answers
        .iter()
        .map(|e| e.payload["answer"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(given, vec!["once", "always", "refuse"]);
    // AND each one is signed
    for entry in &answers {
        assert!(entry.signature.is_some(), "unsigned: {entry:?}");
        assert!(entry.signing_key_id.is_some(), "no key id: {entry:?}");
    }
    // AND the whole journal verifies, signatures included
    let report = journal.verify_journal().await.expect("verification runs");
    assert!(report.ok, "{report:?}");
    assert!(report.head_matches_state, "{report:?}");
    assert!(report.entries_checked >= 3, "{report:?}");
    journal.shutdown().await;
}

// A call of a tool of a connection in pause is refused, the model told why,
// and journaled as refused by the pause.
#[tokio::test]
async fn test_paused_refusal_is_journaled_without_content() {
    // GIVEN the MCP server that brings the tool, in pause
    const PAUSED_TOOL: &str = "mcp:notion/create_page";
    let state = GovernanceState {
        paused: ["mcp:notion".to_string()].into_iter().collect(),
        ..GovernanceState::default()
    };

    // WHEN the model calls it
    let entries = run_exchange_with(PAUSED_TOOL, state).await;

    // THEN the journal records the refusal by the pause, naming the
    // connection, and nothing else
    let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(kinds, vec![JournalEntryKind::PausedRefused]);
    assert_eq!(entries[0].payload["access_id"], PAUSED_TOOL);
    assert_eq!(entries[0].payload["connection_id"], "mcp:notion");
    assert_no_content(&entries);
}

// What the model is told of a refusal by the pause names the connection and
// says it is paused.
#[test]
fn test_the_model_is_told_the_connection_is_paused() {
    // GIVEN a refusal by the pause of a server
    let reason = DecisionReason::Paused {
        connection_id: "mcp:notion".into(),
    };
    // WHEN it is put in words for the model
    let text = super::governed::refusal_text(&reason);
    // THEN it names the connection and the pause
    assert!(text.contains("mcp:notion"), "{text}");
    assert!(text.contains("paused"), "{text}");
}
