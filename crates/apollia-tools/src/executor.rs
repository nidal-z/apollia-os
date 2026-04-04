//! Unified tool execution interface for native tools.
//!
//! Provides [`ToolExecutor`] — a JSON-in / JSON-out async trait — and [`ToolDispatcher`],
//! which routes calls to the correct executor by tool name.
//!
//! Each native tool struct implements [`ToolExecutor`] by deserialising the input
//! [`serde_json::Value`] into its typed input struct, delegating to its `run()` method,
//! and serialising the output back to JSON. Domain errors are mapped to
//! [`ToolExecutionError::ExecutionFailed`] with a stable `code` string.

use apollia_core::manifest::AgentManifest;
use apollia_core::utils::truncate_middle;
use apollia_core::EventBusSender;
use apollia_permissions::{PermissionDecision, PermissionEngine};
use serde_json::Value;
use thiserror::Error;

use crate::tools::bash_executor::{BashExecutor, BashExecutorError, BashInput};
use crate::tools::file_edit::{FileEdit, FileEditError, FileEditInput};
use crate::tools::file_glob::{FileGlob, FileGlobError, FileGlobInput};
use crate::tools::file_grep::{FileGrep, FileGrepError, FileGrepInput};
use crate::tools::file_list::{FileList, FileListError, FileListInput};
use crate::tools::file_read::{FileRead, FileReadError, FileReadInput};
use crate::tools::file_write::{FileWrite, FileWriteError, FileWriteInput};
use crate::tools::python_executor::{PythonExecutor, PythonExecutorError, PythonInput};

#[cfg(feature = "http")]
use crate::tools::http_fetch::{HttpFetch, HttpFetchError, HttpFetchInput};

#[cfg(feature = "memory-search")]
use crate::tools::memory_search::{MemorySearchInput, MemorySearchTool, MemorySearchToolError};

/// Unified error type for tool execution via [`ToolExecutor`].
#[derive(Debug, Error)]
pub enum ToolExecutionError {
    /// The input JSON could not be deserialized into the expected schema.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Human-readable description of the deserialization failure.
        message: String,
    },

    /// The tool execution failed with a domain-specific error.
    #[error("execution failed: {code} — {message}")]
    ExecutionFailed {
        /// Stable snake_case error code (e.g. `"not_found"`, `"sandbox_violation"`).
        code: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The requested tool is not registered in the dispatcher.
    #[error("unknown tool: '{name}'")]
    UnknownTool {
        /// The unrecognised tool name.
        name: String,
    },

    /// The permission engine denied the invocation (AutoDenied*).
    ///
    /// Returned when `PermissionEngine::decide()` produces `AutoDeniedInjection`
    /// or `AutoDeniedPrefixRule`. The `reason` field contains a human-readable
    /// description of why the invocation was blocked.
    #[error("permission denied: {reason}")]
    PermissionDenied {
        /// Human-readable reason for the denial.
        reason: String,
    },

    /// The permission engine requires human approval before executing this tool.
    ///
    /// Returned when `PermissionEngine::decide()` produces `NeedsApproval`
    /// and no `EventBusSender` is configured to emit `PermissionRequired`.
    #[error("permission check failed: {0}")]
    PermissionError(String),
}

/// Trait for executing a native tool via a JSON-in / JSON-out interface.
///
/// Each native tool implements this trait. The dispatcher routes by tool name
/// and delegates to the corresponding executor instance.
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// The unique name identifying this tool (must match the descriptor name).
    fn name(&self) -> &str;

    /// Returns `true` if this tool does not modify any external state.
    ///
    /// Read-only tools may be executed concurrently alongside other read-only tools
    /// by [`ToolDispatcher::execute_batch`]. Defaults to `false`: if an executor
    /// forgets to override this method, it is conservatively treated as mutating
    /// and never parallelised.
    fn is_read_only(&self) -> bool {
        false
    }

    /// Execute the tool with the given JSON input and return the JSON output.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::InvalidInput`] if `input` cannot be deserialized into the
    ///   tool's expected schema.
    /// - [`ToolExecutionError::ExecutionFailed`] if the tool encounters a domain error.
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}

/// A single tool invocation in a batch dispatched by [`ToolDispatcher::execute_batch`].
#[derive(Debug)]
pub struct ToolBatchCall {
    /// The registered name of the tool to invoke.
    pub tool_name: String,
    /// JSON input payload for the tool.
    pub input: Value,
}

/// Taille maximale par défaut d'un output d'outil transmis au LLM, en bytes UTF-8.
const DEFAULT_MAX_OUTPUT_CHARS: usize = 30_000;

/// Maximum number of read-only tool calls driven concurrently in [`ToolDispatcher::execute_batch`].
///
/// Caps the parallelism to avoid saturating file descriptors or exhausting OS
/// resources when a batch contains many read operations (e.g. 20 grep calls).
const MAX_CONCURRENT_READ_TOOLS: usize = 10;

/// Routes tool calls to the appropriate executor by name.
///
/// The dispatcher holds a list of pre-built executors and delegates each call to the
/// one whose [`ToolExecutor::name`] matches `tool_name`. After execution, outputs
/// exceeding `max_output_chars` are automatically trimmed via the middle-trim strategy:
/// the start and end of the output are preserved, the middle is replaced with a
/// truncation marker indicating the number of discarded lines.
///
/// An optional [`PermissionEngine`] can be wired in via [`with_permission_engine`].
/// When present, `dispatch()` evaluates the 3-layer permission policy before executing
/// the tool. If the engine returns `AutoDenied*`, the call is blocked and a
/// [`ToolExecutionError::PermissionDenied`] is returned. If it returns `NeedsApproval`,
/// a [`RuntimeEvent::PermissionRequired`] is emitted on the `EventBusSender` (fire-and-forget)
/// and [`ToolExecutionError::PermissionDenied`] is returned.
///
/// [`with_permission_engine`]: ToolDispatcher::with_permission_engine
pub struct ToolDispatcher {
    executors: Vec<Box<dyn ToolExecutor>>,
    max_output_chars: usize,
    /// Optional 3-layer permission engine. `None` = no permission check (backward-compat).
    permission_engine: Option<std::sync::Mutex<PermissionEngine>>,
    /// EventBus sender for emitting `PermissionRequired` events.
    event_bus: Option<EventBusSender>,
    /// Agent manifest used by `PermissionEngine::decide()`.
    agent_manifest: Option<AgentManifest>,
}

impl ToolDispatcher {
    /// Create a new dispatcher from a list of pre-built executors.
    ///
    /// Uses [`DEFAULT_MAX_OUTPUT_CHARS`] (30 000 bytes) as the output limit.
    /// Call [`with_max_output_chars`] to override from config.
    ///
    /// [`with_max_output_chars`]: ToolDispatcher::with_max_output_chars
    pub fn new(executors: Vec<Box<dyn ToolExecutor>>) -> Self {
        Self {
            executors,
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
            permission_engine: None,
            event_bus: None,
            agent_manifest: None,
        }
    }

    /// Override the maximum output size for middle-trim truncation.
    ///
    /// Should be set from `apollia.toml` `[tools] max_output_chars` via
    /// [`ToolsConfig`]. Returns `self` for ergonomic chaining.
    ///
    /// [`ToolsConfig`]: apollia_core::ToolsConfig
    pub fn with_max_output_chars(mut self, max_output_chars: usize) -> Self {
        self.max_output_chars = max_output_chars;
        self
    }

    /// Wire the 3-layer permission engine into the dispatcher.
    ///
    /// When set, every `dispatch()` call evaluates `PermissionEngine::decide()`
    /// before executing the tool. Requires a corresponding [`AgentManifest`] to be
    /// provided via [`with_agent_manifest`].
    ///
    /// [`with_agent_manifest`]: ToolDispatcher::with_agent_manifest
    pub fn with_permission_engine(mut self, engine: PermissionEngine) -> Self {
        self.permission_engine = Some(std::sync::Mutex::new(engine));
        self
    }

    /// Set the `EventBusSender` used to emit `RuntimeEvent::PermissionRequired`.
    ///
    /// When set alongside a permission engine, `NeedsApproval` decisions trigger
    /// a fire-and-forget broadcast on this sender before returning `PermissionDenied`.
    pub fn with_event_bus(mut self, sender: EventBusSender) -> Self {
        self.event_bus = Some(sender);
        self
    }

    /// Set the agent manifest forwarded to `PermissionEngine::decide()`.
    ///
    /// Required when a permission engine is configured.
    pub fn with_agent_manifest(mut self, manifest: AgentManifest) -> Self {
        self.agent_manifest = Some(manifest);
        self
    }

    /// Dispatch a tool call to the executor registered under `tool_name`.
    ///
    /// When a `PermissionEngine` is configured, the 3-layer policy is evaluated
    /// before execution. Blocked invocations return `PermissionDenied` immediately.
    ///
    /// After a successful execution, the output is serialized to a JSON string.
    /// If the serialized length exceeds `max_output_chars`, the middle is trimmed
    /// and the result is returned as a [`Value::String`] containing the truncated
    /// content with a marker indicating the number of discarded lines.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::PermissionDenied`] if the permission engine blocks or
    ///   suspends the invocation.
    /// - [`ToolExecutionError::PermissionError`] if the permission engine itself fails.
    /// - [`ToolExecutionError::UnknownTool`] if no executor is registered for `tool_name`.
    /// - [`ToolExecutionError::ExecutionFailed`] with code `"serialization_error"` if the
    ///   executor output cannot be serialized.
    /// - All other errors are forwarded unchanged from the matched executor.
    pub async fn dispatch(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Result<Value, ToolExecutionError> {
        // ── Permission check (optional) ───────────────────────────────────────
        if let Some(engine_mutex) = &self.permission_engine {
            let manifest = self.agent_manifest.as_ref().ok_or_else(|| {
                ToolExecutionError::PermissionError(
                    "permission engine configured but no agent manifest provided".to_string(),
                )
            })?;

            let decision = {
                let mut engine = engine_mutex.lock().map_err(|_| {
                    ToolExecutionError::PermissionError(
                        "permission engine mutex poisoned".to_string(),
                    )
                })?;
                engine
                    .decide(tool_name, &input, manifest)
                    .map_err(|e| ToolExecutionError::PermissionError(e.to_string()))?
            };

            match &decision {
                PermissionDecision::AutoAllowedSafeList
                | PermissionDecision::AutoAllowedPrefixRule { .. } => {
                    // Invocation approuvée — continuer normalement.
                    tracing::debug!(tool = %tool_name, "permission: auto-allowed");
                }
                PermissionDecision::AutoDeniedInjection { pattern } => {
                    tracing::warn!(
                        tool = %tool_name,
                        pattern = %pattern,
                        "permission: injection detected, invocation blocked"
                    );
                    return Err(ToolExecutionError::PermissionDenied {
                        reason: format!(
                            "injection pattern detected: '{}' — invocation blocked",
                            pattern
                        ),
                    });
                }
                PermissionDecision::AutoDeniedPrefixRule { rule_id } => {
                    tracing::warn!(
                        tool = %tool_name,
                        rule_id,
                        "permission: denied by prefix rule"
                    );
                    return Err(ToolExecutionError::PermissionDenied {
                        reason: format!("denied by prefix rule {}", rule_id),
                    });
                }
                PermissionDecision::NeedsApproval => {
                    // Émettre PermissionRequired (fire-and-forget) si un EventBus est disponible.
                    if let Some(sender) = &self.event_bus {
                        let request_id = uuid::Uuid::new_v4().to_string();
                        let event = apollia_core::RuntimeEvent::PermissionRequired {
                            tool_name: tool_name.to_string(),
                            input: input.clone(),
                            request_id: request_id.clone(),
                        };
                        let _ = sender.send(event);
                        tracing::info!(
                            tool = %tool_name,
                            request_id = %request_id,
                            "permission: NeedsApproval — PermissionRequired event emitted"
                        );
                    } else {
                        tracing::info!(
                            tool = %tool_name,
                            "permission: NeedsApproval — no event bus configured"
                        );
                    }
                    return Err(ToolExecutionError::PermissionDenied {
                        reason: "human approval required before executing this tool".to_string(),
                    });
                }
            }
        }

        // ── Tool execution ───────────────────────────────────────────────────
        let executor = self
            .executors
            .iter()
            .find(|e| e.name() == tool_name)
            .ok_or_else(|| ToolExecutionError::UnknownTool {
                name: tool_name.to_string(),
            })?;

        let value = executor.execute(input).await?;

        let raw =
            serde_json::to_string(&value).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialization_error".to_string(),
                message: e.to_string(),
            })?;

        let (output, truncated) = truncate_middle(&raw, self.max_output_chars);
        if let Some(lines) = truncated {
            tracing::debug!(
                lines_truncated = lines,
                tool = %tool_name,
                "output tronqué middle-trim"
            );
            Ok(Value::String(output))
        } else {
            Ok(value)
        }
    }

    /// Execute a batch of tool calls, parallelising when all tools are read-only.
    ///
    /// **Parallel path** — when every call in `calls` resolves to a registered
    /// read-only executor (`is_read_only() == true`): all calls are driven
    /// concurrently via `futures::stream::StreamExt::buffered` with a cap of
    /// [`MAX_CONCURRENT_READ_TOOLS`] (10) simultaneous executions.
    ///
    /// **Serial path** — when at least one call targets an unknown tool
    /// (`get_executor()` returns `None`) or a mutating executor: every call is
    /// executed in the input order to preserve effect ordering.
    ///
    /// In both paths the output Vec is in the same order as `calls`.
    /// Middle-trim truncation is applied to each result via the inner
    /// [`dispatch`](Self::dispatch) call, exactly as for single dispatches.
    pub async fn execute_batch(
        &self,
        calls: Vec<ToolBatchCall>,
    ) -> Vec<Result<Value, ToolExecutionError>> {
        use futures::stream::{self, StreamExt};

        if calls.is_empty() {
            return vec![];
        }

        let all_read_only = calls.iter().all(|c| {
            self.executors
                .iter()
                .find(|e| e.name() == c.tool_name)
                .map(|e| e.is_read_only())
                // Unknown tool → conservative: treat as mutating → force serial path.
                .unwrap_or(false)
        });

        if all_read_only {
            // Build a stream of tool_name / input pairs and drive up to
            // MAX_CONCURRENT_READ_TOOLS futures in-flight at any time.
            // `buffered` preserves input order in the output Vec.
            let tool_names: Vec<String> = calls.iter().map(|c| c.tool_name.clone()).collect();
            let inputs: Vec<Value> = calls.into_iter().map(|c| c.input).collect();

            stream::iter(
                (0..tool_names.len()).map(|i| self.dispatch(&tool_names[i], inputs[i].clone())),
            )
            .buffered(MAX_CONCURRENT_READ_TOOLS)
            .collect::<Vec<_>>()
            .await
        } else {
            let mut results = Vec::with_capacity(calls.len());
            for call in calls {
                results.push(self.dispatch(&call.tool_name, call.input).await);
            }
            results
        }
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — file tools
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ToolExecutor for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileReadInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileReadError::SandboxViolation { .. } => "sandbox_violation",
                FileReadError::NotFound { .. } => "not_found",
                FileReadError::IoError { .. } => "io_error",
                FileReadError::BinaryFile { .. } => "binary_file",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileWriteInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileWriteError::SandboxViolation { .. } => "sandbox_violation",
                FileWriteError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({}))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileEditInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileEditError::SandboxViolation { .. } => "sandbox_violation",
                FileEditError::NotFound { .. } => "not_found",
                FileEditError::PatternNotFound { .. } => "pattern_not_found",
                FileEditError::AmbiguousMatch { .. } => "ambiguous_match",
                FileEditError::NoChange => "no_change",
                FileEditError::BinaryFile { .. } => "binary_file",
                FileEditError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileList {
    fn name(&self) -> &str {
        "file_list"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileListInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileListError::SandboxViolation { .. } => "sandbox_violation",
                FileListError::NotFound { .. } => "not_found",
                FileListError::NotADirectory { .. } => "not_a_directory",
                FileListError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileGlob {
    fn name(&self) -> &str {
        "file_glob"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileGlobInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileGlobError::SandboxViolation { .. } => "sandbox_violation",
                FileGlobError::InvalidPattern(_) => "invalid_pattern",
                FileGlobError::IoError(_) => "io_error",
                FileGlobError::GlobLimitExceeded { .. } => "glob_limit_exceeded",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileGrep {
    fn name(&self) -> &str {
        "file_grep"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileGrepInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileGrepError::SandboxViolation { .. } => "sandbox_violation",
                FileGrepError::InvalidRegex(_) => "invalid_regex",
                FileGrepError::IoError(_) => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — network tool (feature = "http")
// ---------------------------------------------------------------------------

#[cfg(feature = "http")]
#[async_trait::async_trait]
impl ToolExecutor for HttpFetch {
    fn name(&self) -> &str {
        "http_fetch"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: HttpFetchInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                HttpFetchError::HostNotAllowed { .. } => "host_not_allowed",
                HttpFetchError::NoAllowlist => "no_allowlist",
                HttpFetchError::InvalidUrl(_) => "invalid_url",
                HttpFetchError::RequestFailed(_) => "request_failed",
                HttpFetchError::ResponseTooLarge { .. } => "response_too_large",
                HttpFetchError::Timeout { .. } => "timeout",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — memory tool (feature = "memory-search")
// ---------------------------------------------------------------------------

#[cfg(feature = "memory-search")]
#[async_trait::async_trait]
impl ToolExecutor for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: MemorySearchInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                MemorySearchToolError::EmptyQuery => "empty_query",
                MemorySearchToolError::NamespaceNotAllowed(_) => "namespace_not_allowed",
                MemorySearchToolError::SearchFailed(_) => "search_failed",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — process tools (manual JSON I/O)
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ToolExecutor for BashExecutor {
    fn name(&self) -> &str {
        "bash_executor"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "missing required field 'command'".to_string(),
            })?
            .to_string();

        let timeout_secs =
            input["timeout_secs"]
                .as_u64()
                .ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "missing required field 'timeout_secs'".to_string(),
                })?;

        let working_dir = input["working_dir"].as_str().map(std::path::PathBuf::from);

        let bash_input = BashInput {
            command,
            timeout_secs,
            working_dir,
        };

        let output = self.run(bash_input).await.map_err(|e| {
            let code = match &e {
                BashExecutorError::EmptyCommand => "empty_command",
                BashExecutorError::WorkingDirNotFound(_) => "working_dir_not_found",
                BashExecutorError::Timeout { .. } => "timeout",
                BashExecutorError::SpawnFailed(_) => "spawn_failed",
                BashExecutorError::OutputCaptureFailed(_) => "output_capture_failed",
                BashExecutorError::SyntaxError { .. } => "syntax_error",
                BashExecutorError::RiskyCommand { .. } => "risky_command",
                BashExecutorError::SyntaxValidationTimeout => "syntax_validation_timeout",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
        }))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for PythonExecutor {
    fn name(&self) -> &str {
        "python_executor"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let code = input["code"]
            .as_str()
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "missing required field 'code'".to_string(),
            })?
            .to_string();

        let timeout_secs =
            input["timeout_secs"]
                .as_u64()
                .ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "missing required field 'timeout_secs'".to_string(),
                })?;

        let python_input = PythonInput { code, timeout_secs };

        let output = self.run(python_input).await.map_err(|e| {
            let code_str = match &e {
                PythonExecutorError::EmptyCode => "empty_code",
                PythonExecutorError::PythonUnavailable => "python_unavailable",
                PythonExecutorError::VenvCreationFailed(_) => "venv_creation_failed",
                PythonExecutorError::PackageInstallFailed { .. } => "package_install_failed",
                PythonExecutorError::Timeout { .. } => "timeout",
                PythonExecutorError::SpawnFailed(_) => "spawn_failed",
                PythonExecutorError::OutputCaptureFailed(_) => "output_capture_failed",
                PythonExecutorError::TempFileFailed(_) => "temp_file_failed",
            };
            ToolExecutionError::ExecutionFailed {
                code: code_str.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    /// Minimal executor used to verify dispatcher routing without file I/O.
    struct EchoExecutor {
        tool_name: &'static str,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for EchoExecutor {
        fn name(&self) -> &str {
            self.tool_name
        }

        async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn dispatcher_routes_to_correct_executor() {
        // GIVEN: dispatcher with a single executor named "test_tool"
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "test_tool",
        })]);
        let payload = json!({"key": "value"});

        // WHEN: dispatch("test_tool", payload)
        let result = dispatcher.dispatch("test_tool", payload.clone()).await;

        // THEN: the echo executor returns the input unchanged
        assert_eq!(result.expect("dispatch should succeed"), payload);
    }

    #[tokio::test]
    async fn dispatcher_unknown_tool_returns_error() {
        // GIVEN: dispatcher with no registered executors
        let dispatcher = ToolDispatcher::new(vec![]);

        // WHEN: dispatch("unknown", ...)
        let result = dispatcher.dispatch("unknown", json!({})).await;

        // THEN: Err(UnknownTool { name: "unknown" })
        match result {
            Err(ToolExecutionError::UnknownTool { name }) => {
                assert_eq!(name, "unknown");
            }
            other => panic!("expected UnknownTool, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn executor_invalid_input_returns_error() {
        // GIVEN: FileRead executor with a valid sandbox
        let tmp = TempDir::new().expect("tempdir");
        let executor =
            FileRead::new(tmp.path().to_path_buf()).expect("FileRead::new should succeed");

        // WHEN: execute with JSON that is missing the required "path" field
        let result = executor.execute(json!({"invalid": true})).await;

        // THEN: Err(InvalidInput)
        assert!(
            matches!(result, Err(ToolExecutionError::InvalidInput { .. })),
            "expected InvalidInput, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn executor_domain_error_maps_to_execution_failed() {
        // GIVEN: FileRead executor with an empty sandbox (no "nonexistent.txt")
        let tmp = TempDir::new().expect("tempdir");
        let executor =
            FileRead::new(tmp.path().to_path_buf()).expect("FileRead::new should succeed");

        // WHEN: execute with a valid input schema but a file that does not exist
        let result = executor.execute(json!({"path": "nonexistent.txt"})).await;

        // THEN: Err(ExecutionFailed { code: "not_found", ... })
        match result {
            Err(ToolExecutionError::ExecutionFailed { code, .. }) => {
                assert_eq!(code, "not_found");
            }
            other => panic!("expected ExecutionFailed(not_found), got: {other:?}"),
        }
    }

    /// Executor that returns a fixed string payload of configurable length.
    struct LargeOutputExecutor {
        size: usize,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for LargeOutputExecutor {
        fn name(&self) -> &str {
            "large_tool"
        }

        async fn execute(&self, _input: Value) -> Result<Value, ToolExecutionError> {
            Ok(Value::String("x".repeat(self.size)))
        }
    }

    #[tokio::test]
    async fn dispatcher_output_under_limit_returned_as_value() {
        // GIVEN: a dispatcher with a 100-char limit and an executor producing 10 chars
        let dispatcher = ToolDispatcher::new(vec![Box::new(LargeOutputExecutor { size: 10 })])
            .with_max_output_chars(100);

        // WHEN
        let result = dispatcher.dispatch("large_tool", json!({})).await;

        // THEN: the original Value is returned unchanged (no truncation)
        let val = result.expect("should succeed");
        assert_eq!(val, Value::String("x".repeat(10)));
    }

    #[tokio::test]
    async fn dispatcher_output_over_limit_truncated_with_marker() {
        // GIVEN: a dispatcher with a 40-char limit and an executor producing a 200-char JSON string
        // The serialized form of Value::String("x".repeat(200)) is `"xxx...xxx"` = 202 chars (with quotes)
        let dispatcher = ToolDispatcher::new(vec![Box::new(LargeOutputExecutor { size: 200 })])
            .with_max_output_chars(40);

        // WHEN
        let result = dispatcher.dispatch("large_tool", json!({})).await;

        // THEN: truncated output is a Value::String containing the truncation marker
        let val = result.expect("should succeed");
        let content = val
            .as_str()
            .expect("truncated output must be a JSON string");
        assert!(
            content.contains("lignes tronquées"),
            "expected truncation marker, got: {content}"
        );
    }

    #[tokio::test]
    async fn dispatcher_with_max_output_chars_builder() {
        // GIVEN: default dispatcher then overridden to 50 chars
        let dispatcher = ToolDispatcher::new(vec![Box::new(LargeOutputExecutor { size: 100 })])
            .with_max_output_chars(50);

        // WHEN
        let result = dispatcher.dispatch("large_tool", json!({})).await;

        // THEN: truncated (serialized "x".repeat(100) is 102 chars > 50)
        let val = result.expect("should succeed");
        assert!(val.is_string());
        assert!(val.as_str().unwrap().contains("lignes tronquées"));
    }

    // -----------------------------------------------------------------------
    // execute_batch tests
    // -----------------------------------------------------------------------

    /// Executor that echoes input and tracks invocation timestamps.
    struct TimedEchoExecutor {
        tool_name: &'static str,
        read_only: bool,
        delay_ms: u64,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for TimedEchoExecutor {
        fn name(&self) -> &str {
            self.tool_name
        }

        fn is_read_only(&self) -> bool {
            self.read_only
        }

        async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
            if self.delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
            }
            Ok(input)
        }
    }

    fn make_read_only_dispatcher(n: usize, delay_ms: u64) -> (ToolDispatcher, Vec<ToolBatchCall>) {
        let executors: Vec<Box<dyn ToolExecutor>> = (0..n)
            .map(|i| -> Box<dyn ToolExecutor> {
                Box::new(TimedEchoExecutor {
                    tool_name: Box::leak(format!("ro_{i}").into_boxed_str()),
                    read_only: true,
                    delay_ms,
                })
            })
            .collect();
        let calls = (0..n)
            .map(|i| ToolBatchCall {
                tool_name: format!("ro_{i}"),
                input: json!({"i": i}),
            })
            .collect();
        (ToolDispatcher::new(executors), calls)
    }

    #[tokio::test]
    async fn test_execute_batch_read_only_is_parallel() {
        // GIVEN — 5 read-only executors each sleeping 50 ms
        let (dispatcher, calls) = make_read_only_dispatcher(5, 50);
        let start = std::time::Instant::now();

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN — total < 2× 50 ms (confirms concurrency)
        let elapsed = start.elapsed();
        assert_eq!(results.len(), 5);
        for r in &results {
            assert!(r.is_ok(), "unexpected error: {r:?}");
        }
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "parallel execution too slow: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_execute_batch_results_ordered() {
        // GIVEN — 4 read-only executors; each echoes its index
        let (dispatcher, calls) = make_read_only_dispatcher(4, 0);

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN — output order matches input order
        assert_eq!(results.len(), 4);
        for (i, result) in results.iter().enumerate() {
            let val = result.as_ref().expect("should succeed");
            assert_eq!(val["i"], json!(i), "wrong order at position {i}");
        }
    }

    #[tokio::test]
    async fn test_execute_batch_mixed_is_serial() {
        // GIVEN — [read_only, write, read_only] — write tool makes the batch serial
        let executors: Vec<Box<dyn ToolExecutor>> = vec![
            Box::new(TimedEchoExecutor {
                tool_name: "ro_a",
                read_only: true,
                delay_ms: 0,
            }),
            Box::new(TimedEchoExecutor {
                tool_name: "write_b",
                read_only: false,
                delay_ms: 0,
            }),
            Box::new(TimedEchoExecutor {
                tool_name: "ro_c",
                read_only: true,
                delay_ms: 0,
            }),
        ];
        let calls = vec![
            ToolBatchCall {
                tool_name: "ro_a".into(),
                input: json!({"pos": 0}),
            },
            ToolBatchCall {
                tool_name: "write_b".into(),
                input: json!({"pos": 1}),
            },
            ToolBatchCall {
                tool_name: "ro_c".into(),
                input: json!({"pos": 2}),
            },
        ];
        let dispatcher = ToolDispatcher::new(executors);

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN — all succeed, order preserved
        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            let val = r.as_ref().expect("should succeed");
            assert_eq!(val["pos"], json!(i));
        }
    }

    #[tokio::test]
    async fn test_execute_batch_unknown_tool_forces_serial_and_errors() {
        // GIVEN — first call targets an unknown tool; second is registered and read-only
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor { tool_name: "known" })]);
        let calls = vec![
            ToolBatchCall {
                tool_name: "unknown".into(),
                input: json!({}),
            },
            ToolBatchCall {
                tool_name: "known".into(),
                input: json!({"k": 1}),
            },
        ];

        // WHEN — unknown tool forces serial path; each call is dispatched independently
        let results = dispatcher.execute_batch(calls).await;

        // THEN — first result is UnknownTool error, second succeeds
        assert_eq!(results.len(), 2);
        assert!(matches!(
            results[0],
            Err(ToolExecutionError::UnknownTool { .. })
        ));
        assert!(results[1].is_ok());
    }

    #[tokio::test]
    async fn test_execute_batch_semaphore_limits_concurrency() {
        // GIVEN — 15 read-only executors each sleeping 10 ms
        // With MAX_CONCURRENT_READ_TOOLS=10 the total should be >= 2 batches
        let (dispatcher, calls) = make_read_only_dispatcher(15, 10);

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN — all 15 results returned
        assert_eq!(results.len(), 15);
        for r in &results {
            assert!(r.is_ok(), "unexpected error: {r:?}");
        }
    }

    #[tokio::test]
    async fn test_execute_batch_empty_returns_empty() {
        // GIVEN
        let dispatcher = ToolDispatcher::new(vec![]);
        // WHEN
        let results = dispatcher.execute_batch(vec![]).await;
        // THEN
        assert!(results.is_empty());
    }

    #[test]
    fn test_tool_descriptor_default_is_read_only_false() {
        // GIVEN — default ToolDescriptor fields (via executor default)
        let executor = TimedEchoExecutor {
            tool_name: "write_tool",
            read_only: false,
            delay_ms: 0,
        };
        // THEN — tools that don't override is_read_only return false
        assert!(!executor.is_read_only());
    }

    #[test]
    fn test_read_only_executors_return_true() {
        // GIVEN
        let ro_executor = TimedEchoExecutor {
            tool_name: "ro",
            read_only: true,
            delay_ms: 0,
        };
        // THEN
        assert!(ro_executor.is_read_only());
    }
}
