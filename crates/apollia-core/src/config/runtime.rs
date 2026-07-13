use serde::Deserialize;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// RuntimeConfig
// ─────────────────────────────────────────────

/// Core runtime configuration (`[runtime]` section in `apollia.toml`).
///
/// Controls the capacity of the internal communication infrastructure: the
/// EventBus broadcast channel and the actor mailboxes. Every field has a sane
/// default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// EventBus broadcast channel capacity.
    ///
    /// Maximum number of buffered events before slow receivers get
    /// [`tokio::sync::broadcast::error::RecvError::Lagged`].
    /// Default: 1024. Bounds: [64, 65536].
    #[serde(default = "default_eventbus_capacity")]
    pub eventbus_capacity: usize,

    /// Maximum capacity of an actor mailbox.
    ///
    /// Maximum number of pending messages per agent in the [`AgentMailbox`].
    /// Beyond it, `send()` returns `MailboxError::QueueFull`.
    /// Default: 100. Bounds: [10, 10000].
    #[serde(default = "default_mailbox_capacity")]
    pub mailbox_capacity: usize,

    /// Visibility timeout of a leased mailbox message, in seconds.
    ///
    /// When an agent receives a message it is leased (in-flight) rather than
    /// deleted. If it is not acknowledged before this timeout, it becomes
    /// deliverable again (at-least-once redelivery).
    /// Default: 60. Bounds: [1, 3600].
    #[serde(default = "default_mailbox_visibility_timeout_secs")]
    pub mailbox_visibility_timeout_secs: u64,

    /// Time-to-live of a never-received mailbox message, in seconds.
    ///
    /// A pending message older than this is evicted by the sweeper and an
    /// `AgentMessageDropped { reason: "expired" }` event is emitted.
    /// Default: 86400 (24 h). Bounds: [60, 2592000].
    #[serde(default = "default_mailbox_message_ttl_secs")]
    pub mailbox_message_ttl_secs: u64,

    /// Maximum number of mailbox sends allowed per run (anti-spam guard).
    ///
    /// A send beyond this quota is refused and a `MailboxGuardTriggered` event
    /// is emitted. Enforced in the actor, not bypassable from Python.
    /// Default: 50. Bounds: [1, 100000].
    #[serde(default = "default_mailbox_send_quota_per_run")]
    pub mailbox_send_quota_per_run: u32,

    /// Maximum serialized payload size of a mailbox message, in bytes.
    ///
    /// A send with a larger payload is rejected with `MailboxError::PayloadTooLarge`
    /// before any write, to keep the durable store bounded.
    /// Default: 65536 (64 KiB). Bounds: [1024, 16777216].
    #[serde(default = "default_mailbox_max_payload_bytes")]
    pub mailbox_max_payload_bytes: usize,

    /// Whether the audit journal records the full message payload.
    ///
    /// When `false` (default), only the SHA-256 hash is journaled (privacy and
    /// size). When `true`, the full payload is recorded (regulated / high
    /// assurance).
    #[serde(default)]
    pub mailbox_audit_full_payload: bool,

    /// Runtime startup timeout in seconds.
    ///
    /// Maximum time allotted to load every component at startup, including
    /// local LLM models. Large models (e.g. 70B to 400B) can take several
    /// minutes.
    /// Default: 300. No upper bound (0 disables the timeout).
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [runtime]
    /// startup_timeout_secs = 600
    /// ```
    #[serde(default = "default_startup_timeout_secs")]
    pub startup_timeout_secs: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            eventbus_capacity: default_eventbus_capacity(),
            mailbox_capacity: default_mailbox_capacity(),
            mailbox_visibility_timeout_secs: default_mailbox_visibility_timeout_secs(),
            mailbox_message_ttl_secs: default_mailbox_message_ttl_secs(),
            mailbox_send_quota_per_run: default_mailbox_send_quota_per_run(),
            mailbox_max_payload_bytes: default_mailbox_max_payload_bytes(),
            mailbox_audit_full_payload: false,
            startup_timeout_secs: default_startup_timeout_secs(),
        }
    }
}

impl RuntimeConfig {
    /// Validates the runtime configuration bounds at startup (fail-fast).
    ///
    /// - `eventbus_capacity`: must be in [64, 65536].
    /// - `mailbox_capacity`: must be in [10, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "runtime.eventbus_capacity",
            self.eventbus_capacity,
            64,
            65536,
        )?;
        validate_bounds("runtime.mailbox_capacity", self.mailbox_capacity, 10, 10000)?;
        validate_bounds(
            "runtime.mailbox_visibility_timeout_secs",
            self.mailbox_visibility_timeout_secs as usize,
            1,
            3600,
        )?;
        validate_bounds(
            "runtime.mailbox_message_ttl_secs",
            self.mailbox_message_ttl_secs as usize,
            60,
            2_592_000,
        )?;
        validate_bounds(
            "runtime.mailbox_send_quota_per_run",
            self.mailbox_send_quota_per_run as usize,
            1,
            100_000,
        )?;
        validate_bounds(
            "runtime.mailbox_max_payload_bytes",
            self.mailbox_max_payload_bytes,
            1024,
            16_777_216,
        )?;
        Ok(())
    }
}

fn default_startup_timeout_secs() -> u64 {
    300
}

fn default_mailbox_visibility_timeout_secs() -> u64 {
    60
}

fn default_mailbox_message_ttl_secs() -> u64 {
    86_400
}

fn default_mailbox_send_quota_per_run() -> u32 {
    50
}

fn default_mailbox_max_payload_bytes() -> usize {
    65_536
}

fn default_eventbus_capacity() -> usize {
    1024
}

fn default_mailbox_capacity() -> usize {
    100
}
