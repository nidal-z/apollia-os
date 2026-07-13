use std::fmt;

use serde::{Deserialize, Serialize};

/// Unique identifier of an agent in the runtime (UUID v4 or slug name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// Create a new AgentId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for AgentId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for AgentId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Unique identifier of a task in the runtime (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Create a new TaskId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s.to_string())
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for TaskId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for TaskId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for TaskId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Stable identifier for a single chat run (one user message, one response cycle).
///
/// Generated once at the start of `BuiltInChatAgent::execute()` and propagated
/// through the `RuntimeEvent` variants emitted during that run. Distinct from
/// the session id (the conversation lifetime) and from the per-message id
/// (one individual message). A run groups the events of a single user turn so
/// the plan-control flow can correlate gate, approval, and execution on a
/// precise run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    /// Generates a new random RunId (UUID v4).
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrows the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for RunId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RunId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for RunId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
