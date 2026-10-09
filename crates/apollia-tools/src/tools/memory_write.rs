//! Memory write tool: an agent keeps a note in its own memory, or drops one.
//!
//! The counterpart of `memory_search`. The agent decides what is worth
//! keeping (a decision taken, a preference stated for its work, a result it
//! will need again) and writes it under a key; writing the same key again
//! replaces the note, and `forget` removes it. Only the agent's own namespace
//! is reachable: a shared namespace is read, never written. The notes are
//! searched by words, and by meaning once the background pass has given them
//! a vector.
//!
//! Nothing is written on the agent's behalf: a note exists because the agent
//! called this tool.

use std::path::PathBuf;

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Name of the memory write tool.
pub const MEMORY_WRITE_TOOL: &str = "memory_write";

/// Longest key a note is kept under, in characters.
pub const MAX_KEY_CHARS: usize = 120;

/// Longest note, in characters.
pub const MAX_VALUE_CHARS: usize = 2000;

/// What the entries written by this tool name as their source.
const NOTE_SOURCE: &str = "agent";

/// Keeps or drops a note in the agent's own memory namespace.
pub struct MemoryWriteTool {
    /// The agent's own namespace, the only one written.
    namespace: String,
    /// Base directory containing `<namespace>.db` files.
    base_dir: PathBuf,
}

/// Errors produced by [`MemoryWriteTool`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MemoryWriteToolError {
    /// The key is empty or longer than [`MAX_KEY_CHARS`].
    #[error("the key must hold between 1 and {MAX_KEY_CHARS} characters")]
    InvalidKey,
    /// A note to remember is empty or longer than [`MAX_VALUE_CHARS`].
    #[error("the value must hold between 1 and {MAX_VALUE_CHARS} characters")]
    InvalidValue,
    /// The memory could not be opened or written.
    #[error("memory write failed: {0}")]
    WriteFailed(String),
}

/// What the agent asks of its memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryWriteAction {
    /// Keep `value` under `key`, replacing what the key held.
    Remember,
    /// Drop the note kept under `key`.
    Forget,
}

/// Input of a memory write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryWriteInput {
    /// `remember` or `forget`.
    pub action: MemoryWriteAction,
    /// The key the note is kept under, for example `client.dupont.deadline`.
    pub key: String,
    /// The note; required to remember, ignored to forget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Output of a memory write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteOutput {
    /// What was asked.
    pub action: MemoryWriteAction,
    /// The key, trimmed.
    pub key: String,
    /// Whether the memory changed: always for a note kept, and for a note
    /// dropped only when the key held one.
    pub changed: bool,
}

impl MemoryWriteTool {
    /// A tool writing the namespace `namespace` under `base_dir`.
    pub fn new(namespace: String, base_dir: PathBuf) -> Self {
        Self {
            namespace,
            base_dir,
        }
    }

    /// Keep or drop the note `input` names.
    ///
    /// # Errors
    ///
    /// [`MemoryWriteToolError::InvalidKey`] and
    /// [`MemoryWriteToolError::InvalidValue`] before anything is opened, and
    /// [`MemoryWriteToolError::WriteFailed`] when the memory cannot be
    /// written.
    pub async fn run(
        &self,
        input: MemoryWriteInput,
    ) -> Result<MemoryWriteOutput, MemoryWriteToolError> {
        let key = input.key.trim().to_owned();
        if key.is_empty() || key.chars().count() > MAX_KEY_CHARS {
            return Err(MemoryWriteToolError::InvalidKey);
        }
        let value = match input.action {
            MemoryWriteAction::Remember => {
                let value = input.value.as_deref().unwrap_or_default().trim().to_owned();
                if value.is_empty() || value.chars().count() > MAX_VALUE_CHARS {
                    return Err(MemoryWriteToolError::InvalidValue);
                }
                Some(value)
            }
            MemoryWriteAction::Forget => None,
        };
        let action = input.action;
        let namespace = self.namespace.clone();
        let base_dir = self.base_dir.clone();
        let stored_key = key.clone();
        let failed = |e: &dyn std::fmt::Display| MemoryWriteToolError::WriteFailed(e.to_string());
        // SQLite work is offloaded to a blocking thread.
        let changed = tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&base_dir).map_err(|e| failed(&e))?;
            let store =
                apollia_memory::store::MemoryStore::open(&base_dir.join(format!("{namespace}.db")))
                    .map_err(|e| failed(&e))?;
            let memory = apollia_memory::semantic::SemanticMemory::new(&store);
            match value {
                Some(value) => memory
                    .remember(apollia_memory::semantic::RememberInput {
                        namespace: &namespace,
                        key: &stored_key,
                        value: &serde_json::Value::String(value),
                        confidence: 1.0,
                        source: Some(NOTE_SOURCE),
                        expires_at: None,
                    })
                    .map(|_| true)
                    .map_err(|e| failed(&e)),
                None => memory
                    .forget(&namespace, &stored_key)
                    .map_err(|e| failed(&e)),
            }
        })
        .await
        .map_err(|e| failed(&e))??;
        Ok(MemoryWriteOutput {
            action,
            key,
            changed,
        })
    }

    /// Returns the [`ToolDescriptor`] for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: MEMORY_WRITE_TOOL.to_string(),
            version: "1.0.0".to_string(),
            description: "Keep a note in your own memory, or drop one. Use `remember` for \
                what you will need in a later conversation: a decision taken, a preference \
                the person stated for your work, a result worth finding again. One fact per \
                key; writing a key again replaces its note. Use `forget` when a note is no \
                longer true. Find your notes again with `memory_search`."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["remember", "forget"],
                        "description": "`remember` keeps the value under the key; `forget` drops the note of the key."
                    },
                    "key": {
                        "type": "string",
                        "maxLength": MAX_KEY_CHARS,
                        "description": "Short name of the note, from the general to the particular, for example `client.dupont.deadline`."
                    },
                    "value": {
                        "type": "string",
                        "maxLength": MAX_VALUE_CHARS,
                        "description": "The note, as one self-contained sentence or two. Required to remember."
                    }
                },
                "required": ["action", "key"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "What was asked: remember or forget" },
                    "key": { "type": "string", "description": "The key of the note" },
                    "changed": { "type": "boolean", "description": "Whether the memory changed" }
                },
                "required": ["action", "key", "changed"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["memory".to_string(), "write".to_string()],
            // The agent's own memory, a space the person opened for it by
            // checking Memory on its sheet: neither the user's data nor the
            // machine is reached.
            family: crate::descriptor::ActionFamily::Exempt,
            family_source: crate::descriptor::FamilySource::Builtin,
            target_arg: None,
            is_read_only: false,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::memory_search::{MemorySearchInput, MemorySearchTool};

    fn remember(key: &str, value: &str) -> MemoryWriteInput {
        MemoryWriteInput {
            action: MemoryWriteAction::Remember,
            key: key.into(),
            value: Some(value.into()),
        }
    }

    async fn found(dir: &std::path::Path, query: &str) -> Vec<String> {
        MemorySearchTool::new("notes".into(), Vec::new(), dir.to_path_buf())
            .run(MemorySearchInput {
                query: query.into(),
                namespace: None,
                limit: None,
                source: None,
                min_relevance: None,
            })
            .await
            .unwrap()
            .results
            .into_iter()
            .map(|r| r.content)
            .collect()
    }

    #[tokio::test]
    async fn test_a_note_kept_is_found_again_by_the_search() {
        // GIVEN an agent whose memory holds nothing, not even a file
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("memory");
        let tool = MemoryWriteTool::new("notes".into(), base.clone());

        // WHEN it keeps a note, its key padded
        let kept = tool
            .run(remember(
                "  client.dupont.deadline ",
                "Dupont wants the quotes before Friday.",
            ))
            .await
            .unwrap();

        // THEN the memory changed under the trimmed key, and the search of
        // the same agent finds the note
        assert_eq!(
            kept,
            MemoryWriteOutput {
                action: MemoryWriteAction::Remember,
                key: "client.dupont.deadline".into(),
                changed: true,
            }
        );
        let notes = found(&base, "Dupont quotes").await;
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("before Friday"), "{notes:?}");
    }

    #[tokio::test]
    async fn test_writing_a_key_again_replaces_its_note() {
        // GIVEN a note kept under a key
        let dir = tempfile::tempdir().unwrap();
        let tool = MemoryWriteTool::new("notes".into(), dir.path().to_path_buf());
        tool.run(remember("format", "The person wants a table."))
            .await
            .unwrap();

        // WHEN the same key is written again
        tool.run(remember("format", "The person wants a bulleted list."))
            .await
            .unwrap();

        // THEN the memory holds the second note only
        assert!(found(dir.path(), "table").await.is_empty());
        assert_eq!(found(dir.path(), "bulleted").await.len(), 1);
    }

    #[tokio::test]
    async fn test_forgetting_drops_the_note_and_says_whether_there_was_one() {
        // GIVEN a note kept under a key
        let dir = tempfile::tempdir().unwrap();
        let tool = MemoryWriteTool::new("notes".into(), dir.path().to_path_buf());
        tool.run(remember("format", "The person wants a table."))
            .await
            .unwrap();
        let forget = || MemoryWriteInput {
            action: MemoryWriteAction::Forget,
            key: "format".into(),
            value: None,
        };

        // WHEN the key is forgotten, twice
        let first = tool.run(forget()).await.unwrap();
        let second = tool.run(forget()).await.unwrap();

        // THEN the first call dropped the note, the second found none, and
        // the search finds nothing
        assert!(first.changed);
        assert!(!second.changed);
        assert!(found(dir.path(), "table").await.is_empty());
    }

    #[tokio::test]
    async fn test_an_empty_or_oversized_note_is_refused_before_anything_is_written() {
        // GIVEN a memory directory that does not exist yet
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("memory");
        let tool = MemoryWriteTool::new("notes".into(), base.clone());

        // WHEN a blank key, a blank value and a value over the limit are sent
        let no_key = tool.run(remember("  ", "A note.")).await.unwrap_err();
        let no_value = tool.run(remember("format", "  ")).await.unwrap_err();
        let too_long = tool
            .run(remember("format", &"x".repeat(MAX_VALUE_CHARS + 1)))
            .await
            .unwrap_err();

        // THEN each is refused and nothing was created
        assert!(matches!(no_key, MemoryWriteToolError::InvalidKey));
        assert!(matches!(no_value, MemoryWriteToolError::InvalidValue));
        assert!(matches!(too_long, MemoryWriteToolError::InvalidValue));
        assert!(!base.exists());
    }

    #[test]
    fn test_the_tool_acts_on_the_memory_of_the_agent_alone() {
        // GIVEN the descriptor of the tool
        // WHEN it is read
        let descriptor = MemoryWriteTool::descriptor();

        // THEN it is named as the dispatcher names it, takes no target and
        // belongs to the family that asks no consent
        assert_eq!(descriptor.name, MEMORY_WRITE_TOOL);
        assert_eq!(descriptor.family, crate::descriptor::ActionFamily::Exempt);
        assert!(!descriptor.is_read_only);
        assert_eq!(descriptor.target_arg, None);
    }
}
