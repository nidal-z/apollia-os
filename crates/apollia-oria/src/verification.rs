//! Post-run verification: programmed checks plus an optional LLM critic pass.
//!
//! Before a run is declared done at a sufficient autonomy tier, the runtime can
//! re-read the result and confirm it. This module hosts the two independent
//! pieces (ADR-029):
//!
//! - [`VerificationLoop`]: runs the shell check commands declared by the agent
//!   manifest (tests, lint) via an injected invoker and aggregates a
//!   [`VerificationReport`].
//! - [`CriticPass`]: an optional, degradable LLM pass that compares the agent
//!   output to the stated objective and proposes structured [`Correction`]s.
//!
//! The source of the check commands follows ADR-029: the manifest field
//! `check_commands` is the primary source; a project-config fallback applies
//! only when the manifest declares none. When no command is resolved, the loop
//! is a no-op that reports success.
//!
//! One actor, one responsibility: this module only coordinates and aggregates.
//! Actual command execution is delegated to the injected [`CheckInvoker`], and
//! the LLM call is delegated to the configured backend.

use std::sync::Arc;

use apollia_llm::{ChatMessage, CompletionRequest, LlmRouter};

/// Raw outcome from a single check command invocation.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// Exit code of the process.
    pub exit_code: i32,
    /// Captured standard error.
    pub stderr: String,
}

/// A single check command that failed.
#[derive(Debug, Clone)]
pub struct CheckFailure {
    /// The command string that was invoked.
    pub command: String,
    /// Exit code returned by the process, or `-1` when the invoker returned an error.
    pub exit_code: i32,
    /// Standard error output captured from the command, or the invoker error message.
    pub stderr: String,
}

/// Aggregated result of a verification pass.
#[derive(Debug, Clone)]
pub struct VerificationReport {
    /// True if all declared commands exited with code 0 (or no commands were declared).
    pub passed: bool,
    /// Non-empty only when `passed` is false.
    pub failures: Vec<CheckFailure>,
}

/// Errors produced by [`VerificationLoop`] setup itself (not by individual commands).
///
/// Individual command failures are reported as [`CheckFailure`] entries inside a
/// [`VerificationReport`], never as this error.
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    /// The manifest provided check commands but the injected invoker could not
    /// parse or dispatch a command string.
    #[error("verification setup failed: {0}")]
    SetupFailed(String),
}

/// Minimal invocation contract for a verification check command.
///
/// Injected into [`VerificationLoop::run`]; the concrete implementation delegates
/// to `bash_executor` or an equivalent native tool. Test implementations use a mock.
///
/// Uses return-position `impl Trait` in trait (RPITIT) to avoid `#[async_trait]`
/// boxing. As a consequence the trait is not dyn-compatible, so
/// [`VerificationLoop::run`] is generic over the invoker rather than taking a
/// trait object.
pub trait CheckInvoker: Send + Sync {
    /// Invoke `command` as a shell string.
    ///
    /// Returns the exit code and captured stderr on success, or an error string
    /// when the invoker itself fails (process not found, timeout).
    fn invoke_check(
        &self,
        command: &str,
    ) -> impl std::future::Future<Output = Result<CheckOutcome, String>> + Send;
}

/// Orchestrates verification check commands after a completed agent run.
///
/// One actor, one responsibility: it coordinates invocations and aggregates
/// [`CheckFailure`] entries. Actual execution is delegated to the injected
/// [`CheckInvoker`].
pub struct VerificationLoop {
    /// Resolved command list (manifest commands, or the project fallback when
    /// the manifest declares none).
    commands: Vec<String>,
}

impl VerificationLoop {
    /// Create a `VerificationLoop` from manifest commands and a project-config fallback.
    ///
    /// When `manifest_commands` is non-empty it is used directly and
    /// `fallback_commands` is ignored. Decision documented in ADR-029: the
    /// manifest wins; the fallback provides a project-level safety net without
    /// requiring each agent to redeclare its checks.
    pub fn new(manifest_commands: Vec<String>, fallback_commands: Vec<String>) -> Self {
        let commands = if manifest_commands.is_empty() {
            fallback_commands
        } else {
            manifest_commands
        };
        Self { commands }
    }

    /// Whether any check command is resolved (manifest or project fallback).
    ///
    /// When `false`, [`run`](Self::run) is a no-op that always passes.
    pub fn has_commands(&self) -> bool {
        !self.commands.is_empty()
    }

    /// Run all resolved check commands via the injected invoker and collect failures.
    ///
    /// Returns `VerificationReport { passed: true, failures: vec![] }` immediately
    /// when no commands are resolved (manifest empty AND fallback empty). Does NOT
    /// short-circuit on the first failure: every command is executed so the report
    /// lists all failures at once. An invoker-level error becomes a
    /// [`CheckFailure`] with `exit_code = -1`.
    pub async fn run<I: CheckInvoker>(&self, invoker: &I) -> VerificationReport {
        let mut failures = Vec::new();

        for command in &self.commands {
            match invoker.invoke_check(command).await {
                Ok(outcome) => {
                    tracing::info!(
                        command = %command,
                        exit_code = outcome.exit_code,
                        "verification.check.done"
                    );
                    if outcome.exit_code != 0 {
                        failures.push(CheckFailure {
                            command: command.clone(),
                            exit_code: outcome.exit_code,
                            stderr: outcome.stderr,
                        });
                    }
                }
                Err(message) => {
                    tracing::warn!(
                        command = %command,
                        error = %message,
                        "verification.check.invoker_error"
                    );
                    failures.push(CheckFailure {
                        command: command.clone(),
                        exit_code: -1,
                        stderr: message,
                    });
                }
            }
        }

        VerificationReport {
            passed: failures.is_empty(),
            failures,
        }
    }
}

/// A single correction proposed by the LLM critic.
///
/// Represents one actionable discrepancy between the agent's output and the
/// stated objective, as identified by the critic LLM.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Correction {
    /// Short identifier for the discrepancy (e.g. "missing_file", "wrong_format").
    pub kind: String,
    /// Human-readable description of the issue.
    pub description: String,
    /// Concrete suggestion the agent can act on in its retry turn.
    pub suggestion: String,
}

/// Result of one LLM critic pass.
#[derive(Debug, Clone)]
pub struct CriticReport {
    /// True if no corrections were found (or the pass was skipped).
    pub passed: bool,
    /// Corrections proposed by the critic.
    pub corrections: Vec<Correction>,
    /// True when the pass was skipped (no LLM backend, or routing/inference error).
    pub skipped: bool,
}

/// Wire shape parsed from the critic LLM response.
#[derive(Debug, serde::Deserialize)]
struct CriticResponse {
    #[serde(default)]
    corrections: Vec<Correction>,
}

/// Internal English-only system prompt for the critic pass. Not user-configurable.
const CRITIC_SYSTEM_PROMPT: &str =
    "You are a strict verification critic. Compare the AGENT OUTPUT \
to the stated OBJECTIVE and report only concrete, actionable discrepancies. Reply with a single \
JSON object and nothing else, in the exact shape: \
{\"corrections\":[{\"kind\":\"...\",\"description\":\"...\",\"suggestion\":\"...\"}]}. \
Use an empty corrections array when the output already satisfies the objective. \
Do not add any prose before or after the JSON.";

/// Orchestrates one LLM critic pass comparing agent output to the stated objective.
///
/// Stateless: all inputs are passed to [`CriticPass::run`]. The LLM backend is
/// resolved lazily via [`LlmRouter::route_precise`] at call time. The pass is
/// degradable: with no router, or on any routing or inference error, it returns
/// a skipped report instead of failing the run (principle #1 and #4).
pub struct CriticPass {
    /// LLM router, `None` when the runtime has no backend configured.
    router: Option<Arc<LlmRouter>>,
}

impl CriticPass {
    /// Build a `CriticPass` with a configured router.
    pub fn new(router: Arc<LlmRouter>) -> Self {
        Self {
            router: Some(router),
        }
    }

    /// Build a `CriticPass` that always skips (no LLM backend).
    pub fn disabled() -> Self {
        Self { router: None }
    }

    /// Run one critic pass: call the LLM and parse corrections.
    ///
    /// - `objective`: the original user request / task description.
    /// - `agent_output`: the text output produced by the agent at end of run.
    ///
    /// Returns a skipped report when no router is configured or when routing or
    /// inference fails. When the LLM responds with unparseable content, returns
    /// an empty, non-skipped report (logged at `warn`) rather than panicking.
    pub async fn run(&self, objective: &str, agent_output: &str) -> CriticReport {
        let Some(router) = self.router.as_ref() else {
            return CriticReport {
                passed: true,
                corrections: Vec::new(),
                skipped: true,
            };
        };

        let model = match router.route_precise() {
            Ok(model) => model,
            Err(error) => {
                tracing::warn!(error = %error, "critic.pass.llm_unavailable");
                return CriticReport {
                    passed: true,
                    corrections: Vec::new(),
                    skipped: true,
                };
            }
        };

        let user_content = format!("OBJECTIVE:\n{objective}\n\nAGENT OUTPUT:\n{agent_output}");
        let request = CompletionRequest {
            messages: vec![
                ChatMessage::system(CRITIC_SYSTEM_PROMPT),
                ChatMessage::user(user_content),
            ],
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = match model.complete(request).await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(error = %error, "critic.pass.llm_unavailable");
                return CriticReport {
                    passed: true,
                    corrections: Vec::new(),
                    skipped: true,
                };
            }
        };

        match parse_critic_response(&response.content) {
            Some(corrections) => {
                tracing::info!(corrections_count = corrections.len(), "critic.pass.done");
                CriticReport {
                    passed: corrections.is_empty(),
                    corrections,
                    skipped: false,
                }
            }
            None => CriticReport {
                passed: true,
                corrections: Vec::new(),
                skipped: false,
            },
        }
    }
}

/// Parse the critic response into corrections.
///
/// Tries the raw content first, then falls back to the first `{` .. last `}`
/// slice (models sometimes wrap JSON in prose). Returns `None` and logs at
/// `warn` when no valid JSON object can be extracted.
fn parse_critic_response(content: &str) -> Option<Vec<Correction>> {
    if let Ok(parsed) = serde_json::from_str::<CriticResponse>(content) {
        return Some(parsed.corrections);
    }

    if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
        if start < end {
            if let Ok(parsed) = serde_json::from_str::<CriticResponse>(&content[start..=end]) {
                return Some(parsed.corrections);
            }
        }
    }

    tracing::warn!(parse_error = %"no valid JSON object", "critic.pass.json_parse_failed");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---- Mock CheckInvoker ----

    struct MockCheckInvoker {
        responses: HashMap<String, Result<CheckOutcome, String>>,
    }

    impl MockCheckInvoker {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
            }
        }

        fn with(mut self, cmd: &str, outcome: Result<CheckOutcome, String>) -> Self {
            self.responses.insert(cmd.to_string(), outcome);
            self
        }
    }

    impl CheckInvoker for MockCheckInvoker {
        async fn invoke_check(&self, command: &str) -> Result<CheckOutcome, String> {
            self.responses
                .get(command)
                .cloned()
                .unwrap_or_else(|| Err(format!("command not registered: {command}")))
        }
    }

    fn ok(exit_code: i32) -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code,
            stderr: String::new(),
        })
    }

    // AC-1: every declared check passes.
    #[tokio::test]
    async fn test_ac1_all_checks_pass() {
        // GIVEN two declared commands that both exit 0
        let invoker = MockCheckInvoker::new()
            .with("cargo test -p my-agent", ok(0))
            .with("cargo clippy -p my-agent", ok(0));
        let verification = VerificationLoop::new(
            vec![
                "cargo test -p my-agent".into(),
                "cargo clippy -p my-agent".into(),
            ],
            vec![],
        );

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the report passes with no failures
        assert!(report.passed);
        assert!(report.failures.is_empty());
    }

    // AC-2: one check fails and the loop does not short-circuit.
    #[tokio::test]
    async fn test_ac2_one_failure_no_short_circuit() {
        // GIVEN a passing first command and a failing second command
        let invoker = MockCheckInvoker::new()
            .with("cargo test -p my-agent", ok(0))
            .with(
                "cargo clippy",
                Ok(CheckOutcome {
                    exit_code: 1,
                    stderr: "warning treated as error".into(),
                }),
            );
        let verification = VerificationLoop::new(
            vec!["cargo test -p my-agent".into(), "cargo clippy".into()],
            vec![],
        );

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN only the failing command appears, after the first ran too
        assert!(!report.passed);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].command, "cargo clippy");
        assert_eq!(report.failures[0].exit_code, 1);
    }

    // AC-3: no declared command is a no-op success.
    #[tokio::test]
    async fn test_ac3_no_commands_noop() {
        // GIVEN no manifest commands and no fallback
        let invoker = MockCheckInvoker::new();
        let verification = VerificationLoop::new(vec![], vec![]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN it passes immediately without invoking anything
        assert!(report.passed);
        assert!(report.failures.is_empty());
    }

    // AC-4 (error case): an invoker-level error becomes a CheckFailure with exit_code -1.
    #[tokio::test]
    async fn test_ac4_invoker_error_becomes_failure() {
        // GIVEN an invoker that returns an error for the command
        let invoker = MockCheckInvoker::new().with("cargo test", Err("executor not found".into()));
        let verification = VerificationLoop::new(vec!["cargo test".into()], vec![]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the failure carries exit_code -1 and the invoker message
        assert!(!report.passed);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].exit_code, -1);
        assert!(report.failures[0].stderr.contains("executor not found"));
    }

    // Fallback: manifest commands empty, project fallback is used.
    #[tokio::test]
    async fn test_fallback_used_when_manifest_empty() {
        // GIVEN no manifest commands but a project fallback command
        let invoker = MockCheckInvoker::new().with("make check", ok(0));
        let verification = VerificationLoop::new(vec![], vec!["make check".into()]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the fallback command is executed and the report passes
        assert!(report.passed);
    }
}

#[cfg(test)]
mod critic_tests {
    use super::*;
    use apollia_llm::{
        CompletionModel, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
    };
    use futures::Stream;
    use std::collections::HashMap;
    use std::pin::Pin;

    // ---- Mock CompletionModel ----

    struct MockCriticModel {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCriticModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::InferenceError("mock no stream".into()))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-critic"
        }
        fn model_id(&self) -> &str {
            "mock-critic"
        }
    }

    fn router_with(response: &str) -> Arc<LlmRouter> {
        let model = Arc::new(MockCriticModel {
            response: response.to_string(),
        });
        let backends: HashMap<String, Arc<dyn CompletionModel>> =
            HashMap::from([("mock-critic".to_string(), model as Arc<dyn CompletionModel>)]);
        Arc::new(LlmRouter::with_backends(backends, "mock-critic"))
    }

    // AC-1: corrections returned, passed=false.
    #[tokio::test]
    async fn test_ac1_corrections_found() {
        // GIVEN a critic LLM returning two structured corrections
        let json = r#"{"corrections":[
            {"kind":"missing_file","description":"output.csv absent","suggestion":"create output.csv"},
            {"kind":"wrong_format","description":"invalid JSON format","suggestion":"fix the structure"}
        ]}"#;
        let pass = CriticPass::new(router_with(json));

        // WHEN the pass runs
        let report = pass.run("Generate a CSV file", "Here is the result.").await;

        // THEN both corrections are reported and the pass does not pass
        assert!(!report.passed);
        assert!(!report.skipped);
        assert_eq!(report.corrections.len(), 2);
    }

    // AC-2: empty corrections, passed=true.
    #[tokio::test]
    async fn test_ac2_no_corrections_passed() {
        // GIVEN a critic LLM returning no corrections
        let pass = CriticPass::new(router_with(r#"{"corrections":[]}"#));

        // WHEN the pass runs
        let report = pass.run("Generate a report", "Full report.").await;

        // THEN the pass passes with no corrections
        assert!(report.passed);
        assert!(!report.skipped);
        assert!(report.corrections.is_empty());
    }

    // AC-3 (degradation): no router, pass is skipped.
    #[tokio::test]
    async fn test_ac3_no_router_skips() {
        // GIVEN a disabled critic pass
        let pass = CriticPass::disabled();

        // WHEN the pass runs
        let report = pass.run("objective", "output").await;

        // THEN it is skipped without error
        assert!(report.passed);
        assert!(report.skipped);
        assert!(report.corrections.is_empty());
    }

    // AC-4 (error case): malformed JSON yields empty corrections, no panic.
    #[tokio::test]
    async fn test_ac4_malformed_json_no_panic() {
        // GIVEN a critic LLM returning non-JSON content
        let pass = CriticPass::new(router_with("I do not know"));

        // WHEN the pass runs
        let report = pass.run("objective", "output").await;

        // THEN it returns empty, not skipped, and does not panic
        assert!(report.passed);
        assert!(!report.skipped);
        assert!(report.corrections.is_empty());
    }

    // JSON wrapped in prose is still extracted via the brace fallback.
    #[tokio::test]
    async fn test_json_wrapped_in_prose_is_extracted() {
        // GIVEN a critic LLM that wraps the JSON object in prose
        let wrapped = "Here is my verdict: {\"corrections\":[{\"kind\":\"k\",\
            \"description\":\"d\",\"suggestion\":\"s\"}]} Done.";
        let pass = CriticPass::new(router_with(wrapped));

        // WHEN the pass runs
        let report = pass.run("objective", "output").await;

        // THEN the embedded correction is recovered
        assert!(!report.passed);
        assert!(!report.skipped);
        assert_eq!(report.corrections.len(), 1);
    }
}
