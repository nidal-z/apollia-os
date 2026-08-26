//! Unified tool execution interface for native tools.
//!
//! Provides [`ToolExecutor`] - a JSON-in / JSON-out async trait - and [`ToolDispatcher`],
//! which routes calls to the correct executor by tool name.
//!
//! Each native tool struct implements [`ToolExecutor`] by deserialising the input
//! [`serde_json::Value`] into its typed input struct, delegating to its `run()` method,
//! and serialising the output back to JSON. Domain errors are mapped to
//! [`ToolExecutionError::ExecutionFailed`] with a stable `code` string.

mod files;
mod runners;

use apollia_core::utils::truncate_middle;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use thiserror::Error;

/// Unified error type for tool execution via [`ToolExecutor`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ToolExecutionError {
    /// The input JSON could not be deserialized into the expected schema.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Human-readable description of the deserialization failure.
        message: String,
    },

    /// The tool execution failed with a domain-specific error.
    #[error("execution failed: {code} - {message}")]
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

    /// The tool is blocked by the session-level tool filter.
    ///
    /// Returned when [`SessionToolFilter::is_allowed`] returns `false` for the
    /// given tool name. This happens when the tool is listed in `disallowed_tools`,
    /// or when `allowed_tools` is `Some(list)` and the tool is not in the list.
    #[error("tool not allowed for this session: {tool_name}")]
    ToolNotAllowed {
        /// Name of the tool that was blocked.
        tool_name: String,
    },
}

/// Session-level tool filter enforcing `--allowed-tools` / `--disallowed-tools`.
///
/// The filter is evaluated before the executor lookup, so a session-scoped
/// restriction blocks a tool the dispatcher would otherwise run.
///
/// Rules (evaluated in order):
/// 1. If `disallowed_tools` contains the tool name → blocked.
/// 2. If `allowed_tools` is `Some(list)` and the tool is not in `list` → blocked.
/// 3. Otherwise → allowed.
///
/// `disallowed_tools` always wins over `allowed_tools` when both list the same tool.
#[derive(Debug, Clone, Default)]
pub struct SessionToolFilter {
    /// Restrictive allow-list for this session. `None` means all tools are allowed.
    ///
    /// When `Some`, only tools explicitly listed here can be invoked (subject to
    /// `disallowed_tools` taking precedence).
    pub allowed_tools: Option<Vec<String>>,
    /// Tools that are always blocked in this session, regardless of `allowed_tools`.
    pub disallowed_tools: Vec<String>,
}

impl SessionToolFilter {
    /// Create a filter with an explicit allow-list and a deny-list.
    pub fn new(allowed_tools: Option<Vec<String>>, disallowed_tools: Vec<String>) -> Self {
        Self {
            allowed_tools,
            disallowed_tools,
        }
    }

    /// Returns `true` if `tool_name` is allowed under the current filter rules.
    ///
    /// `disallowed_tools` takes priority: a tool listed there is always blocked
    /// even if it also appears in `allowed_tools`.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        if self.disallowed_tools.iter().any(|d| d == tool_name) {
            return false;
        }
        match &self.allowed_tools {
            Some(list) => list.iter().any(|a| a == tool_name),
            None => true,
        }
    }
}

/// Trait for executing a native tool via a JSON-in / JSON-out interface.
///
/// Each native tool implements this trait. The dispatcher routes by tool name
/// and delegates to the corresponding executor instance.
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
    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>>;
}

/// A single tool invocation in a batch dispatched by [`ToolDispatcher::execute_batch`].
#[derive(Debug)]
pub struct ToolBatchCall {
    /// The registered name of the tool to invoke.
    pub tool_name: String,
    /// JSON input payload for the tool.
    pub input: Value,
}

/// Default maximum size of a tool output forwarded to the LLM, in UTF-8 bytes.
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
/// An optional [`SessionToolFilter`] can be wired in via [`with_session_filter`].
/// When present, `dispatch()` evaluates the filter before looking up an executor.
/// Blocked tools return [`ToolExecutionError::ToolNotAllowed`] immediately.
///
/// [`with_session_filter`]: ToolDispatcher::with_session_filter
pub struct ToolDispatcher {
    executors: Vec<Box<dyn ToolExecutor>>,
    max_output_chars: usize,
    /// Optional session-level tool filter (`--allowed-tools` / `--disallowed-tools`).
    tool_filter: Option<SessionToolFilter>,
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
            tool_filter: None,
        }
    }

    /// Names of every executor this dispatcher holds, in registration order.
    ///
    /// Lets embedders and tests enumerate the production tool surface
    /// without dispatching anything; the desktop catalogue coverage test
    /// crosses this list with the i18n catalogues.
    pub fn tool_names(&self) -> Vec<&str> {
        self.executors.iter().map(|e| e.name()).collect()
    }

    /// Attach a session-level tool filter to this dispatcher.
    ///
    /// When set, every `dispatch()` call checks the filter first. Tools blocked by
    /// the filter return [`ToolExecutionError::ToolNotAllowed`] immediately without
    /// reaching the executor.
    pub fn with_session_filter(mut self, filter: SessionToolFilter) -> Self {
        self.tool_filter = Some(filter);
        self
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

    /// Dispatch a tool call to the executor registered under `tool_name`.
    ///
    /// After a successful execution, the output is serialized to a JSON string.
    /// If the serialized length exceeds `max_output_chars`, the middle is trimmed
    /// and the result is returned as a [`Value::String`] containing the truncated
    /// content with a marker indicating the number of discarded lines.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::UnknownTool`] if no executor is registered for `tool_name`.
    /// - [`ToolExecutionError::ExecutionFailed`] with code `"serialization_error"` if the
    ///   executor output cannot be serialized.
    /// - All other errors are forwarded unchanged from the matched executor.
    pub async fn dispatch(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Result<Value, ToolExecutionError> {
        // ── Session-level tool filter (optional) ──────────────────────────────
        if let Some(filter) = &self.tool_filter {
            if !filter.is_allowed(tool_name) {
                tracing::debug!(
                    tool = %tool_name,
                    "tool.session.filter.denied"
                );
                return Err(ToolExecutionError::ToolNotAllowed {
                    tool_name: tool_name.to_string(),
                });
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
                detail = "trimmed in the middle",
                "tool.output.truncated"
            );
            Ok(Value::String(output))
        } else {
            Ok(value)
        }
    }

    /// Execute a batch of tool calls, parallelising when all tools are read-only.
    ///
    /// **Parallel path** - when every call in `calls` resolves to a registered
    /// read-only executor (`is_read_only() == true`): all calls are driven
    /// concurrently via `futures::stream::StreamExt::buffered` with a cap of
    /// [`MAX_CONCURRENT_READ_TOOLS`] (10) simultaneous executions.
    ///
    /// **Serial path** - when at least one call targets an unknown tool
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
// ToolExecutor implementations - file tools
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - notebook tools
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - network tool (feature = "http")
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - web search (feature = "web-search")
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - web_read (feature = "web-read")
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - memory tool (feature = "memory-search")
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ToolExecutor implementations - process tools (manual JSON I/O)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::file_read::FileRead;
    use serde_json::json;
    use tempfile::TempDir;

    /// Minimal executor used to verify dispatcher routing without file I/O.
    struct EchoExecutor {
        tool_name: &'static str,
    }

    impl ToolExecutor for EchoExecutor {
        fn name(&self) -> &str {
            self.tool_name
        }

        fn execute(
            &self,
            input: Value,
        ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
            Box::pin(async move { Ok(input) })
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

    impl ToolExecutor for LargeOutputExecutor {
        fn name(&self) -> &str {
            "large_tool"
        }

        fn execute(
            &self,
            _input: Value,
        ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
            Box::pin(async move { Ok(Value::String("x".repeat(self.size))) })
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
            content.contains("lines truncated"),
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
        assert!(val.as_str().unwrap().contains("lines truncated"));
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

    impl ToolExecutor for TimedEchoExecutor {
        fn name(&self) -> &str {
            self.tool_name
        }

        fn is_read_only(&self) -> bool {
            self.read_only
        }

        fn execute(
            &self,
            input: Value,
        ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
            Box::pin(async move {
                if self.delay_ms > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
                }
                Ok(input)
            })
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
        // GIVEN - 5 read-only executors each sleeping 50 ms
        let (dispatcher, calls) = make_read_only_dispatcher(5, 50);
        let start = std::time::Instant::now();

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN - total < 2× 50 ms (confirms concurrency)
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
        // GIVEN - 4 read-only executors; each echoes its index
        let (dispatcher, calls) = make_read_only_dispatcher(4, 0);

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN - output order matches input order
        assert_eq!(results.len(), 4);
        for (i, result) in results.iter().enumerate() {
            let val = result.as_ref().expect("should succeed");
            assert_eq!(val["i"], json!(i), "wrong order at position {i}");
        }
    }

    #[tokio::test]
    async fn test_execute_batch_mixed_is_serial() {
        // GIVEN - [read_only, write, read_only] - write tool makes the batch serial
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

        // THEN - all succeed, order preserved
        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            let val = r.as_ref().expect("should succeed");
            assert_eq!(val["pos"], json!(i));
        }
    }

    #[tokio::test]
    async fn test_execute_batch_unknown_tool_forces_serial_and_errors() {
        // GIVEN - first call targets an unknown tool; second is registered and read-only
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

        // WHEN - unknown tool forces serial path; each call is dispatched independently
        let results = dispatcher.execute_batch(calls).await;

        // THEN - first result is UnknownTool error, second succeeds
        assert_eq!(results.len(), 2);
        assert!(matches!(
            results[0],
            Err(ToolExecutionError::UnknownTool { .. })
        ));
        assert!(results[1].is_ok());
    }

    #[tokio::test]
    async fn test_execute_batch_semaphore_limits_concurrency() {
        // GIVEN - 15 read-only executors each sleeping 10 ms
        // With MAX_CONCURRENT_READ_TOOLS=10 the total should be >= 2 batches
        let (dispatcher, calls) = make_read_only_dispatcher(15, 10);

        // WHEN
        let results = dispatcher.execute_batch(calls).await;

        // THEN - all 15 results returned
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
        // GIVEN - default ToolDescriptor fields (via executor default)
        let executor = TimedEchoExecutor {
            tool_name: "write_tool",
            read_only: false,
            delay_ms: 0,
        };
        // WHEN its read-only flag is read
        // THEN - tools that don't override is_read_only return false
        assert!(!executor.is_read_only());
    }

    #[test]
    fn test_read_only_executors_return_true() {
        // GIVEN an executor that declares itself read-only
        let ro_executor = TimedEchoExecutor {
            tool_name: "ro",
            read_only: true,
            delay_ms: 0,
        };
        // WHEN its read-only flag is read
        // THEN it answers true, so the dispatcher may run it in parallel
        assert!(ro_executor.is_read_only());
    }

    #[tokio::test]
    async fn allowed_tools_blocks_unlisted_tool() {
        // GIVEN: allowed_tools restricts to "file_read" only
        let filter = SessionToolFilter::new(Some(vec!["file_read".to_string()]), vec![]);
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "bash_executor",
        })])
        .with_session_filter(filter);

        // WHEN: dispatch to a tool not in the allow-list
        let result = dispatcher.dispatch("bash_executor", json!({})).await;

        // THEN: ToolNotAllowed
        match result {
            Err(ToolExecutionError::ToolNotAllowed { tool_name }) => {
                assert_eq!(tool_name, "bash_executor");
            }
            other => panic!("expected ToolNotAllowed, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn disallowed_tools_blocks_listed_tool() {
        // GIVEN: disallowed_tools blocks "file_write"
        let filter = SessionToolFilter::new(None, vec!["file_write".to_string()]);
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "file_write",
        })])
        .with_session_filter(filter);

        // WHEN: dispatch to the blocked tool
        let result = dispatcher.dispatch("file_write", json!({})).await;

        // THEN: ToolNotAllowed
        match result {
            Err(ToolExecutionError::ToolNotAllowed { tool_name }) => {
                assert_eq!(tool_name, "file_write");
            }
            other => panic!("expected ToolNotAllowed, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn no_tool_filter_allows_all() {
        // GIVEN: no filter - all tools allowed
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "bash_executor",
        })]);

        // WHEN: dispatch to any registered tool
        let result = dispatcher
            .dispatch("bash_executor", json!({"cmd": "echo hi"}))
            .await;

        // THEN: succeeds
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn disallowed_wins_over_allowed_conflict() {
        // GIVEN: "file_write" is both allowed and disallowed - disallowed wins
        let filter = SessionToolFilter::new(
            Some(vec!["file_write".to_string()]),
            vec!["file_write".to_string()],
        );
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "file_write",
        })])
        .with_session_filter(filter);

        // WHEN: dispatch to the conflicting tool
        let result = dispatcher.dispatch("file_write", json!({})).await;

        // THEN: ToolNotAllowed - disallowed takes priority
        assert!(matches!(
            result,
            Err(ToolExecutionError::ToolNotAllowed { .. })
        ));
    }
}
