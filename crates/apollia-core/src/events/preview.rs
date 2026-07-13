/// Content preview of a filesystem operation for the HITL modal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilesystemPreview {
    /// Diff between the file's current state and the proposed content.
    Diff {
        /// Current content (empty if the file does not exist yet).
        before: String,
        /// Proposed content.
        after: String,
        /// `true` if either content was truncated.
        truncated: bool,
    },
    /// Display of the current content (delete, sensitive read operation).
    Content {
        /// Current content of the file.
        content: String,
        /// Actual file size in bytes.
        size_bytes: u64,
        /// `true` if the content was truncated.
        truncated: bool,
    },
    /// Permission change (chmod).
    Mode {
        /// Current mode in octal.
        before: u32,
        /// Proposed mode in octal.
        after: u32,
    },
}
