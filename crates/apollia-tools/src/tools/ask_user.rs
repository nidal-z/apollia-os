//! Interactive user-input tool for agents.
//!
//! Allows an agent to pause and ask the user one or more structured questions.
//! Supports three question types:
//! - **open**: free-text response
//! - **single_choice**: select exactly one option from a list
//! - **multi_choice**: select one or more options from a list
//!
//! Multiple questions can be batched in a single call to minimise interaction
//! round-trips.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::executor::ToolExecutionError;
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

// ─── Public types ───────────────────────────────────────────────────────────

/// A single question posed to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuestion {
    /// Unique identifier for this question (e.g. `"stack"`, `"target_audience"`).
    pub id: String,
    /// The question text displayed to the user.
    pub question: String,
    /// Question type: `"open"`, `"single_choice"`, or `"multi_choice"`.
    #[serde(rename = "type")]
    pub question_type: QuestionType,
    /// Available options for choice questions. Ignored for `"open"` type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Optional hint or placeholder for the input field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// The type of user interaction expected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// Free-text answer.
    Open,
    /// Exactly one option must be selected.
    SingleChoice,
    /// One or more options may be selected.
    MultiChoice,
}

/// Input schema for the `ask_user` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserInput {
    /// List of questions to ask (1 to 10).
    pub questions: Vec<UserQuestion>,
    /// Optional context explaining why the agent needs this information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A single answer from the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    /// Matches the `id` of the corresponding question.
    pub id: String,
    /// The user's response text (for open questions) or selected option (for single_choice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Selected options (for multi_choice questions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// Whether the user skipped this question.
    #[serde(default)]
    pub skipped: bool,
}

/// Output returned to the agent after the user responds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserOutput {
    /// Answers in the same order as the questions.
    pub answers: Vec<UserAnswer>,
}

/// A pending ask_user request waiting for the user's response.
#[derive(Debug)]
pub struct PendingUserInput {
    /// The questions being asked.
    pub questions: Vec<UserQuestion>,
    /// Optional context from the agent.
    pub context: Option<String>,
    /// Channel to deliver the user's response.
    pub reply_tx: oneshot::Sender<AskUserOutput>,
}

// ─── Pending registry ───────────────────────────────────────────────────────

/// Registry for pending `ask_user` requests.
///
/// The tool executor registers a pending request here and blocks on the
/// oneshot receiver. The UI/API layer retrieves the request, shows it to the
/// user, collects answers, and sends them back via the oneshot sender.
#[derive(Debug, Clone)]
pub struct PendingUserInputs {
    tx: mpsc::Sender<(String, PendingUserInput)>,
    rx: Arc<Mutex<mpsc::Receiver<(String, PendingUserInput)>>>,
}

impl PendingUserInputs {
    /// Create a new pending registry with a bounded channel.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(64);
        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Register a pending request and return the receiver for the answer.
    ///
    /// Called by the `ask_user` tool executor.
    pub async fn register(
        &self,
        request_id: String,
        request: PendingUserInput,
    ) {
        let _ = self.tx.send((request_id, request)).await;
    }

    /// Retrieve the next pending request (called by the UI/API layer).
    pub async fn next_pending(&self) -> Option<(String, PendingUserInput)> {
        let mut rx = self.rx.lock().await;
        rx.recv().await
    }

    /// Get a sender handle for delivering requests from the tool executor.
    pub fn sender(&self) -> mpsc::Sender<(String, PendingUserInput)> {
        self.tx.clone()
    }
}

impl Default for PendingUserInputs {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tool descriptor ────────────────────────────────────────────────────────

/// The `ask_user` native tool.
///
/// Allows agents to ask the user structured questions and wait for responses.
/// Supports open-ended, single-choice, and multi-choice questions. Multiple
/// questions can be batched in a single call.
pub struct AskUser;

impl AskUser {
    /// Return the tool descriptor for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "ask_user".to_string(),
            version: "1.0.0".to_string(),
            description: "Ask the user one or more questions and wait for their responses. \
                          Supports open-ended questions (free text), single-choice \
                          (pick one from options), and multi-choice (pick several). \
                          Batch multiple questions in a single call to minimise \
                          back-and-forth.".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "required": ["questions"],
                "properties": {
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 10,
                        "description": "List of questions to ask the user.",
                        "items": {
                            "type": "object",
                            "required": ["id", "question", "type"],
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Unique identifier for this question (e.g. 'stack', 'target_audience')."
                                },
                                "question": {
                                    "type": "string",
                                    "description": "The question text to display to the user."
                                },
                                "type": {
                                    "type": "string",
                                    "enum": ["open", "single_choice", "multi_choice"],
                                    "description": "Question type: 'open' for free text, 'single_choice' for pick-one, 'multi_choice' for pick-many."
                                },
                                "options": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Available options (required for single_choice and multi_choice, ignored for open)."
                                },
                                "hint": {
                                    "type": "string",
                                    "description": "Optional placeholder or hint for the input field."
                                }
                            }
                        }
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional context explaining why the agent needs this information."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "answers": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "value": { "type": "string" },
                                "values": { "type": "array", "items": { "type": "string" } },
                                "skipped": { "type": "boolean" }
                            }
                        }
                    }
                }
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec!["user-interaction".to_string(), "hitl".to_string()],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
        }
    }
}

// ─── Executor ───────────────────────────────────────────────────────────────

/// Runtime executor for the `ask_user` tool.
///
/// Holds a sender into the [`PendingUserInputs`] registry. When invoked,
/// it posts the questions, blocks on a oneshot, and returns the answers
/// once the user has responded.
pub struct AskUserExecutor {
    pending_tx: mpsc::Sender<(String, PendingUserInput)>,
}

impl AskUserExecutor {
    /// Create a new executor wired to the given pending registry.
    pub fn new(pending: &PendingUserInputs) -> Self {
        Self {
            pending_tx: pending.sender(),
        }
    }
}

#[async_trait::async_trait]
impl crate::executor::ToolExecutor for AskUserExecutor {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let parsed: AskUserInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        if parsed.questions.is_empty() {
            return Err(ToolExecutionError::InvalidInput {
                message: "at least one question is required".to_string(),
            });
        }

        if parsed.questions.len() > 10 {
            return Err(ToolExecutionError::InvalidInput {
                message: "maximum 10 questions per call".to_string(),
            });
        }

        // Validate choice questions have options
        for q in &parsed.questions {
            if (q.question_type == QuestionType::SingleChoice
                || q.question_type == QuestionType::MultiChoice)
                && q.options.is_empty()
            {
                return Err(ToolExecutionError::InvalidInput {
                    message: format!(
                        "question '{}' is {:?} but has no options",
                        q.id, q.question_type
                    ),
                });
            }
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        let request_id = uuid::Uuid::new_v4().to_string();

        let pending = PendingUserInput {
            questions: parsed.questions,
            context: parsed.context,
            reply_tx,
        };

        self.pending_tx
            .send((request_id.clone(), pending))
            .await
            .map_err(|_| ToolExecutionError::ExecutionFailed {
                code: "channel_closed".to_string(),
                message: "pending user input channel is closed".to_string(),
            })?;

        let output = reply_rx.await.map_err(|_| ToolExecutionError::ExecutionFailed {
            code: "response_dropped".to_string(),
            message: "user input response was dropped (timeout or session closed)".to_string(),
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ToolExecutor;

    #[test]
    fn descriptor_is_valid() {
        let d = AskUser::descriptor();
        assert_eq!(d.name, "ask_user");
        assert!(d.is_read_only);
        assert!(!d.dangerous);
        assert_eq!(d.risk_score, 0);
        d.validate().expect("descriptor should be valid");
    }

    #[tokio::test]
    async fn execute_with_valid_input_and_response() {
        // GIVEN a pending registry and executor
        let pending = PendingUserInputs::new();
        let executor = AskUserExecutor::new(&pending);

        let input = serde_json::to_value(AskUserInput {
            questions: vec![
                UserQuestion {
                    id: "stack".to_string(),
                    question: "Quelle stack technique ?".to_string(),
                    question_type: QuestionType::Open,
                    options: vec![],
                    hint: Some("Ex: Next.js, Django, Rails...".to_string()),
                },
                UserQuestion {
                    id: "deploy".to_string(),
                    question: "Où déployer ?".to_string(),
                    question_type: QuestionType::SingleChoice,
                    options: vec![
                        "Vercel".to_string(),
                        "Cloudflare".to_string(),
                        "AWS".to_string(),
                    ],
                    hint: None,
                },
            ],
            context: Some("Besoin de contexte pour la spec".to_string()),
        })
        .unwrap();

        // WHEN we spawn the executor and respond
        let exec_handle = tokio::spawn(async move { executor.execute(input).await });

        // Simulate the UI layer picking up the pending request and responding
        let (request_id, request) = pending.next_pending().await.unwrap();
        assert!(!request_id.is_empty());
        assert_eq!(request.questions.len(), 2);
        assert_eq!(request.questions[0].id, "stack");

        let _ = request.reply_tx.send(AskUserOutput {
            answers: vec![
                UserAnswer {
                    id: "stack".to_string(),
                    value: Some("Next.js 15 + Tailwind".to_string()),
                    values: vec![],
                    skipped: false,
                },
                UserAnswer {
                    id: "deploy".to_string(),
                    value: Some("Cloudflare".to_string()),
                    values: vec![],
                    skipped: false,
                },
            ],
        });

        // THEN the executor returns the answers
        let result = exec_handle.await.unwrap().unwrap();
        let output: AskUserOutput = serde_json::from_value(result).unwrap();
        assert_eq!(output.answers.len(), 2);
        assert_eq!(output.answers[0].value.as_deref(), Some("Next.js 15 + Tailwind"));
        assert_eq!(output.answers[1].value.as_deref(), Some("Cloudflare"));
    }

    #[tokio::test]
    async fn execute_rejects_empty_questions() {
        // GIVEN an executor
        let pending = PendingUserInputs::new();
        let executor = AskUserExecutor::new(&pending);

        let input = json!({ "questions": [] });

        // WHEN executed with no questions
        let result = executor.execute(input).await;

        // THEN it fails with InvalidInput
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ToolExecutionError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn execute_rejects_choice_without_options() {
        // GIVEN an executor
        let pending = PendingUserInputs::new();
        let executor = AskUserExecutor::new(&pending);

        let input = json!({
            "questions": [{
                "id": "choice",
                "question": "Pick one",
                "type": "single_choice",
                "options": []
            }]
        });

        // WHEN executed with a choice question but no options
        let result = executor.execute(input).await;

        // THEN it fails
        assert!(result.is_err());
    }
}
