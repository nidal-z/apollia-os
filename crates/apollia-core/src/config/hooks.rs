use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// HooksConfig
// ─────────────────────────────────────────────

/// Identifies the lifecycle point at which a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    /// Fires before each tool call. Blocking: the handler can allow, deny, or
    /// rewrite the invocation.
    PreToolUse,
    /// Fires after each tool call. Non-blocking: the handler can inject
    /// additional context but cannot veto the result.
    PostToolUse,
    /// Fires before context compaction. Non-blocking.
    PreCompact,
    /// Fires after context compaction. Non-blocking.
    PostCompact,
    /// Fires when a sub-agent or A2A worker is started. Non-blocking.
    SubagentStart,
    /// Fires when a sub-agent or A2A worker stops, on success or error.
    /// Non-blocking.
    SubagentStop,
}

impl HookEventKind {
    /// Returns the snake_case wire name of this event.
    ///
    /// Stable identifier shared by the JSON payload `event` field, tracing
    /// output, and the CLI summary.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::PreCompact => "pre_compact",
            Self::PostCompact => "post_compact",
            Self::SubagentStart => "subagent_start",
            Self::SubagentStop => "subagent_stop",
        }
    }
}

/// How the runtime delivers a hook event to the handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookHandlerKind {
    /// Spawn an external process. The payload is written to stdin as JSON and
    /// the decision is read back from stdout. The process must exit within the
    /// handler timeout.
    Command {
        /// Argv. Must be non-empty. Index 0 is the executable.
        command: Vec<String>,
    },
    /// POST the payload as JSON to an HTTP endpoint. The endpoint must respond
    /// within the handler timeout.
    Http {
        /// Full URL, for example `"http://127.0.0.1:9000/hook"`.
        url: String,
    },
}

impl HookHandlerKind {
    /// Returns the snake_case wire name of this delivery mechanism
    /// (`"command"` or `"http"`).
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Http { .. } => "http",
        }
    }

    /// Returns a human-readable target: the argv joined by spaces for a command
    /// handler, or the URL for an HTTP handler.
    pub fn target(&self) -> String {
        match self {
            Self::Command { command } => command.join(" "),
            Self::Http { url } => url.clone(),
        }
    }
}

/// A single registered lifecycle hook handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHandlerConfig {
    /// Lifecycle events this handler subscribes to.
    ///
    /// A handler can subscribe to multiple events; the runtime invokes it once
    /// per matching event. Must contain at least one event.
    pub events: Vec<HookEventKind>,

    /// Delivery mechanism (command or http).
    #[serde(flatten)]
    pub kind: HookHandlerKind,

    /// Maximum time the runtime waits for a handler response, in milliseconds.
    ///
    /// Default: 5000. Bounds: [100, 30000]. For the blocking `PreToolUse` hook,
    /// timeout expiry falls back to `allow` and emits a warn-level trace event.
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
}

/// Hooks configuration (`[hooks]` section in `apollia.toml`).
///
/// Declares the set of lifecycle hook handlers the runtime invokes at defined
/// points in the agent execution loop. See [`HookEventKind`] for the available
/// lifecycle events and [`HookHandlerKind`] for the delivery mechanisms.
///
/// An absent `[hooks]` section is equivalent to an empty handler list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Registered hook handlers.
    ///
    /// Each entry subscribes to one or more [`HookEventKind`] events. The
    /// runtime invokes handlers in declaration order for a given event.
    #[serde(default)]
    pub handlers: Vec<HookHandlerConfig>,
}

impl HooksConfig {
    /// Validates the hooks configuration at startup (fail-fast).
    ///
    /// - Every handler must subscribe to at least one event.
    /// - `command` handlers must have a non-empty argv.
    /// - `http` handlers must have a non-empty URL.
    /// - `timeout_ms` must be in [100, 30000].
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidValue`] for an empty event list, an empty
    /// command argv, or an empty HTTP URL, and [`ConfigError::OutOfBounds`]
    /// when `timeout_ms` falls outside `[100, 30000]`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (i, handler) in self.handlers.iter().enumerate() {
            if handler.events.is_empty() {
                return Err(ConfigError::InvalidValue {
                    field: format!("hooks.handlers[{i}].events"),
                    reason: "a handler must subscribe to at least one event".to_string(),
                });
            }
            match &handler.kind {
                HookHandlerKind::Command { command } => {
                    if command.iter().all(|arg| arg.trim().is_empty()) {
                        return Err(ConfigError::InvalidValue {
                            field: format!("hooks.handlers[{i}].command"),
                            reason: "command argv must be non-empty".to_string(),
                        });
                    }
                }
                HookHandlerKind::Http { url } => {
                    if url.trim().is_empty() {
                        return Err(ConfigError::InvalidValue {
                            field: format!("hooks.handlers[{i}].url"),
                            reason: "http url must be non-empty".to_string(),
                        });
                    }
                }
            }
            validate_bounds(
                &format!("hooks.handlers[{i}].timeout_ms"),
                handler.timeout_ms,
                100,
                30_000,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn default_hook_timeout_ms() -> u64 {
    5_000
}
