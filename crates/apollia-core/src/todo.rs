//! Per-session todo state tracked by the agent during a chat run.
//!
//! The agent maintains an explicit task list through the `todo_write` built-in
//! tool. The list lives outside the LLM message buffer so it survives context
//! compaction. Persistence and the at-most-one-`InProgress` invariant are
//! enforced by a dedicated actor in `apollia-runtime`; this module only owns the
//! data types shared across crates.

/// A single todo item tracked by the agent during a session.
///
/// Invariant: at most one item per session may have
/// `status == TodoStatus::InProgress`. The invariant is enforced by the
/// persistence actor before any write; callers receive
/// [`TodoError::MultipleInProgress`] otherwise.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    /// Stable identifier for this item within the session. Free-form string.
    pub id: String,
    /// Human-readable description of the work to be done.
    pub content: String,
    /// Current lifecycle status.
    pub status: TodoStatus,
    /// Optional ordering: ids of items that should be completed before this one
    /// can move to `InProgress`. Mirrors the `depends_on` pattern of plan steps.
    /// Stored verbatim; referential consistency is not validated.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Lifecycle status of a [`TodoItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    /// Not yet started.
    Pending,
    /// Currently being worked on.
    ///
    /// At most one item per session may hold this status; the persistence actor
    /// rejects any write that would create a second one.
    InProgress,
    /// Done.
    Completed,
}

impl TodoStatus {
    /// Returns the stable string used for SQLite storage and JSON payloads.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "pending",
            TodoStatus::InProgress => "in_progress",
            TodoStatus::Completed => "completed",
        }
    }

    /// Parses a [`TodoStatus`] from its stable string form.
    ///
    /// Returns `None` for any value other than `pending`, `in_progress`, or
    /// `completed`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(TodoStatus::Pending),
            "in_progress" => Some(TodoStatus::InProgress),
            "completed" => Some(TodoStatus::Completed),
            _ => None,
        }
    }
}

/// Errors produced by the todo subsystem.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TodoError {
    /// More than one item carried `InProgress` in a single write.
    #[error("invariant violated: at most one item may be in_progress, found {count}")]
    MultipleInProgress {
        /// Number of `InProgress` items found in the rejected write.
        count: usize,
    },
    /// A SQLite operation failed in the todo store.
    #[error("sqlite error in todo store")]
    Sqlite(#[source] rusqlite::Error),
    /// The todo actor channel is closed (the actor has stopped).
    #[error("todo actor channel closed")]
    ActorGone,
}

impl From<rusqlite::Error> for TodoError {
    fn from(source: rusqlite::Error) -> Self {
        TodoError::Sqlite(source)
    }
}

/// Counts the number of items with `status == InProgress`.
///
/// Used by the persistence actor to enforce the at-most-one invariant before a
/// write. Exposed here so the rule has a single, testable definition.
#[must_use]
pub fn count_in_progress(items: &[TodoItem]) -> usize {
    items
        .iter()
        .filter(|i| i.status == TodoStatus::InProgress)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_status_round_trips_through_str() {
        // GIVEN every status variant
        for status in [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
        ] {
            // WHEN serializing to the stable string and parsing it back
            let parsed = TodoStatus::parse(status.as_str());

            // THEN the original variant is recovered
            assert_eq!(parsed, Some(status));
        }
    }

    #[test]
    fn test_todo_status_from_str_rejects_unknown() {
        // GIVEN an unrecognized status string
        // WHEN parsing it
        let parsed = TodoStatus::parse("blocked");

        // THEN no variant is produced
        assert_eq!(parsed, None);
    }

    #[test]
    fn test_count_in_progress_counts_only_in_progress() {
        // GIVEN a mixed list with two in_progress items
        let items = vec![
            TodoItem {
                id: "a".into(),
                content: "first".into(),
                status: TodoStatus::Pending,
                depends_on: vec![],
            },
            TodoItem {
                id: "b".into(),
                content: "second".into(),
                status: TodoStatus::InProgress,
                depends_on: vec![],
            },
            TodoItem {
                id: "c".into(),
                content: "third".into(),
                status: TodoStatus::InProgress,
                depends_on: vec![],
            },
        ];

        // WHEN counting in_progress items
        let count = count_in_progress(&items);

        // THEN both are counted
        assert_eq!(count, 2);
    }

    #[test]
    fn test_todo_item_deserializes_without_depends_on() {
        // GIVEN a JSON item omitting depends_on
        let raw = r#"{"id":"t1","content":"work","status":"in_progress"}"#;

        // WHEN deserializing
        let item: TodoItem = serde_json::from_str(raw).expect("valid payload");

        // THEN depends_on defaults to empty and status parses
        assert_eq!(item.status, TodoStatus::InProgress);
        assert!(item.depends_on.is_empty());
    }
}
