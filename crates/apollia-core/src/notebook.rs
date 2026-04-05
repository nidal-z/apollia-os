//! Jupyter notebook types for Apollia OS.
//!
//! Covers the nbformat v4 cell model and the atomic edit operations
//! applied by the notebook tools in `apollia-tools`.

use serde::{Deserialize, Serialize};

/// Type of a Jupyter notebook cell (nbformat v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    /// Executable code cell.
    Code,
    /// Markdown prose cell.
    Markdown,
    /// Raw text cell (no transformation applied).
    Raw,
}

impl std::fmt::Display for CellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellType::Code => write!(f, "code"),
            CellType::Markdown => write!(f, "markdown"),
            CellType::Raw => write!(f, "raw"),
        }
    }
}

/// A cell in a Jupyter notebook (ipynb v4 format).
///
/// Source lines retain their trailing newlines as stored in the `.ipynb` file.
/// Outputs are present only for `code` cells; `markdown` and `raw` cells carry
/// an empty `outputs` array per the nbformat v4 spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupyterCell {
    /// Type of the cell.
    pub cell_type: CellType,
    /// Source lines of the cell (each line may end with `\n`).
    #[serde(default)]
    pub source: Vec<String>,
    /// Execution outputs (empty for non-code cells).
    #[serde(default)]
    pub outputs: Vec<serde_json::Value>,
    /// Cell-level metadata object.
    #[serde(default = "default_cell_metadata")]
    pub metadata: serde_json::Value,
}

fn default_cell_metadata() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Atomic operation applied to a Jupyter notebook by `NotebookEdit`.
///
/// Operations are applied sequentially in the order provided. Each operation
/// validates its index against the current cell count before mutating state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum NotebookEditOp {
    /// Replace the source of a cell and clear its outputs.
    ///
    /// Outputs are cleared to maintain consistency between source and execution
    /// results, following the nbformat v4 specification.
    EditCell {
        /// Zero-based index of the cell to modify.
        index: usize,
        /// New source lines replacing the existing content.
        new_source: Vec<String>,
    },
    /// Insert a new cell at the given position.
    ///
    /// All cells at `index` and beyond are shifted down by one.
    InsertCell {
        /// Zero-based index at which to insert. Must be `<= cells.len()`.
        index: usize,
        /// Type of the cell to create.
        cell_type: CellType,
        /// Initial source lines for the new cell.
        source: Vec<String>,
    },
    /// Remove the cell at the given index.
    ///
    /// All cells beyond `index` are shifted up by one.
    DeleteCell {
        /// Zero-based index of the cell to remove.
        index: usize,
    },
    /// Replace the metadata object of a cell without touching its source or outputs.
    EditCellMetadata {
        /// Zero-based index of the cell to update.
        index: usize,
        /// New metadata value (must be a JSON object).
        metadata: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_type_serialization_roundtrip() {
        assert_eq!(serde_json::to_string(&CellType::Code).unwrap(), "\"code\"");
        assert_eq!(
            serde_json::to_string(&CellType::Markdown).unwrap(),
            "\"markdown\""
        );
        assert_eq!(serde_json::to_string(&CellType::Raw).unwrap(), "\"raw\"");

        let decoded: CellType = serde_json::from_str("\"code\"").unwrap();
        assert_eq!(decoded, CellType::Code);
    }

    #[test]
    fn notebook_edit_op_tagged_serialization() {
        let op = NotebookEditOp::DeleteCell { index: 2 };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"op\":\"delete_cell\""));
        assert!(json.contains("\"index\":2"));
    }
}
