//! What a tool call returns to the model, and how the call itself is made.
//!
//! An output too long for the context is not cut: it is written whole to a
//! file of the tool output directory, and the model receives its beginning
//! with the path of the rest, which it can read like any other file. Only when
//! no directory is set, or the file cannot be written, is the output cut.
//!
//! A call goes through the circuit breaker of its tool: after three
//! consecutive transient failures the tool is refused at once until a cooldown
//! has passed. A read-only tool is retried on a transient failure with an
//! exponential backoff; a tool that changes something is never called twice
//! for one request.

use std::path::{Path, PathBuf};

use apollia_core::RetryAttempt;
use apollia_oria::resilience::{ErrorClass, RetryContext, RetryPolicy};
use apollia_oria::tool_offload::{OffloadRef, ToolOffloadStore};

use super::*;

/// Name of the directory, under the data directory, where long tool outputs
/// are kept.
pub const TOOL_OUTPUT_DIR: &str = "tool-output";

/// The tool output directory of this machine, when the data directory is
/// known.
pub fn default_tool_output_dir() -> Option<PathBuf> {
    apollia_core::paths::data_dir().map(|dir| dir.join(TOOL_OUTPUT_DIR))
}

/// What the model receives for `output`.
///
/// Under [`TOOL_OUTPUT_MAX_LEN`] characters, `output` itself. Above, with a
/// directory, its beginning followed by the path of the file holding all of
/// it; without one, or when the file cannot be written, `output` cut with a
/// notice.
pub fn context_tool_output(output: &str, dir: Option<&Path>) -> String {
    if output.len() <= TOOL_OUTPUT_MAX_LEN {
        return output.to_string();
    }
    let Some(dir) = dir else {
        return truncate_tool_output(output);
    };
    match offload(output, dir) {
        Ok(path) => format!(
            "{head}\n\n[Output kept whole on disk: {total} characters, {lines} lines. \
             The rest is in {path}; read that file to see it.]",
            head = head_of(output, TOOL_OUTPUT_MAX_LEN),
            total = output.len(),
            lines = output.lines().count(),
            path = path.display(),
        ),
        Err(e) => {
            warn!(error = %e, "chat.tool.output.offload_failed");
            truncate_tool_output(output)
        }
    }
}

/// Write `output` to a new file of `dir`, created when missing, and return
/// its path.
fn offload(output: &str, dir: &Path) -> Result<PathBuf, apollia_oria::tool_offload::OffloadError> {
    std::fs::create_dir_all(dir).map_err(|source| {
        apollia_oria::tool_offload::OffloadError::Io {
            path: dir.to_path_buf(),
            source,
        }
    })?;
    let oref = OffloadRef::new_unique();
    ToolOffloadStore::new(dir.to_path_buf()).write(&oref, output)?;
    Ok(dir.join(oref.filename()))
}

/// The first `max_len` bytes of `s`, cut on a character boundary.
fn head_of(s: &str, max_len: usize) -> &str {
    let mut end = max_len.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// How a tool error reads for the breaker: refused, invalid or out of budget
/// errors are permanent, anything else may pass on a second try.
fn classify_tool_error(err: &str) -> ErrorClass {
    let lower = err.to_lowercase();
    if lower.contains("budget") {
        ErrorClass::BudgetExceeded
    } else if lower.contains("sandbox")
        || lower.contains("path traversal")
        || lower.contains("unauthorized")
        || lower.contains("refused")
        || lower.contains("not granted")
    {
        ErrorClass::SandboxViolation
    } else if lower.contains("not found")
        || lower.contains("invalid input")
        || lower.contains("invalid argument")
    {
        ErrorClass::Permanent
    } else {
        ErrorClass::Transient
    }
}

impl BuiltInChatAgent {
    /// Call `call` through the breaker of its tool, retrying a read-only tool
    /// on a transient failure. Returns the outcome and the attempts made when
    /// there was more than one.
    pub(in crate::chat::builtin_agent) async fn invoke_through_breaker(
        &self,
        call: &ToolCall,
    ) -> (Result<String, String>, Vec<RetryAttempt>) {
        self.resilience.ensure_tool(&call.name);
        let read_only = self
            .tool_registry
            .describe(&call.name)
            .await
            .is_some_and(|d| d.is_read_only);
        let policy = if read_only {
            RetryPolicy::default()
        } else {
            RetryPolicy {
                max_attempts: 1,
                ..RetryPolicy::default()
            }
        };
        let (outcome, attempts) = self
            .resilience
            .execute_with_observability(
                RetryContext {
                    tool_name: &call.name,
                    tool_call_id: &call.id,
                    retry_policy: &policy,
                    bus: Some(&self.event_bus),
                },
                classify_tool_error,
                || self.tool_invoker.invoke(&call.name, &call.arguments),
            )
            .await;
        let attempts = if attempts.len() > 1 {
            attempts
        } else {
            Vec::new()
        };
        let outcome = outcome.map_err(|e| match e {
            apollia_oria::ResilienceError::ExecutionFailed(message) => message,
            other => other.to_string(),
        });
        (outcome, attempts)
    }
}

#[cfg(test)]
mod tests;
