use serde::{Deserialize, Serialize};

/// Why a resident local model was unloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadReason {
    /// No request used it for the delay of `[llm.residency]`, and nothing
    /// held it.
    Idle,
    /// The last open conversation that designated it was closed.
    LastConversation,
    /// The user asked for it to be unloaded.
    Requested,
    /// Another model needed its memory to load within the load guardrail.
    MakeRoom,
    /// The compatible server loaded another model, and keeps only the last
    /// one it loaded.
    ServerReplaced,
}

impl ModelUnloadReason {
    /// Wire name, as serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::LastConversation => "last_conversation",
            Self::Requested => "requested",
            Self::MakeRoom => "make_room",
            Self::ServerReplaced => "server_replaced",
        }
    }
}

impl std::fmt::Display for ModelUnloadReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
