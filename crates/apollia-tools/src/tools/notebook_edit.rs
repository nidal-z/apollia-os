//! Edit Jupyter notebook files via atomic cell operations.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::SandboxRoot;
use apollia_core::{JupyterCell, NotebookEditOp, SandboxProfile};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;

/// Edit a Jupyter `.ipynb` notebook via a sequence of atomic cell operations.
///
/// Reads the notebook, applies the operations in order, and writes the modified
/// JSON back to disk, preserving `metadata`, `nbformat`, and `nbformat_minor`.
/// Only nbformat v4 notebooks are supported.
#[derive(Debug, Clone)]
pub struct NotebookEdit {
    sandbox: SandboxRoot,
}

/// Errors produced by [`NotebookEdit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NotebookEditError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation {
        /// The offending path.
        path: String,
    },

    /// File not found at the specified path.
    #[error("not found: '{path}'")]
    NotFound {
        /// The missing path.
        path: String,
    },

    /// I/O error while reading or writing the file.
    #[error("I/O error on '{path}': {reason}")]
    Io {
        /// The affected path.
        path: String,
        /// Low-level OS error description.
        reason: String,
    },

    /// The file is not a valid nbformat v4 notebook.
    #[error("not a valid Jupyter notebook")]
    InvalidNotebook,

    /// A cell index is out of bounds.
    #[error("cell index {index} out of bounds ({count} cells)")]
    IndexOutOfBounds {
        /// The requested index.
        index: usize,
        /// The current cell count.
        count: usize,
    },
}

/// Input for a notebook edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEditInput {
    /// Relative path inside the sandbox to a `.ipynb` file.
    pub path: String,
    /// Ordered list of atomic operations to apply.
    pub operations: Vec<NotebookEditOp>,
}

/// Output of a notebook edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEditOutput {
    /// Number of operations successfully applied.
    pub operations_applied: usize,
    /// Resulting cell count after all operations.
    pub cell_count: usize,
}

impl NotebookEdit {
    /// Create a new [`NotebookEdit`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`NotebookEditError::Io`] if the sandbox root cannot be created
    /// or canonicalized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, NotebookEditError> {
        let sandbox = SandboxRoot::new(sandbox_root).map_err(|e| NotebookEditError::Io {
            path: "sandbox_root".to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self { sandbox })
    }

    /// Apply a single [`NotebookEditOp`] to a mutable cell list.
    ///
    /// Validates the cell index against the current length before mutating.
    /// `EditCell` clears `outputs` to maintain consistency between source and
    /// execution results (nbformat v4 spec, §Cell outputs).
    ///
    /// # Errors
    ///
    /// Returns [`NotebookEditError::IndexOutOfBounds`] when the index is
    /// invalid for the given operation type.
    pub fn apply_operation(
        cells: &mut Vec<JupyterCell>,
        op: &NotebookEditOp,
    ) -> Result<(), NotebookEditError> {
        match op {
            NotebookEditOp::EditCell { index, new_source } => {
                if *index >= cells.len() {
                    return Err(NotebookEditError::IndexOutOfBounds {
                        index: *index,
                        count: cells.len(),
                    });
                }
                cells[*index].source = new_source.clone();
                cells[*index].outputs.clear();
            }

            NotebookEditOp::InsertCell {
                index,
                cell_type,
                source,
            } => {
                if *index > cells.len() {
                    return Err(NotebookEditError::IndexOutOfBounds {
                        index: *index,
                        count: cells.len(),
                    });
                }
                let new_cell = JupyterCell {
                    cell_type: cell_type.clone(),
                    source: source.clone(),
                    outputs: Vec::new(),
                    metadata: serde_json::Value::Object(serde_json::Map::new()),
                };
                cells.insert(*index, new_cell);
            }

            NotebookEditOp::DeleteCell { index } => {
                if *index >= cells.len() {
                    return Err(NotebookEditError::IndexOutOfBounds {
                        index: *index,
                        count: cells.len(),
                    });
                }
                cells.remove(*index);
            }

            NotebookEditOp::EditCellMetadata { index, metadata } => {
                if *index >= cells.len() {
                    return Err(NotebookEditError::IndexOutOfBounds {
                        index: *index,
                        count: cells.len(),
                    });
                }
                cells[*index].metadata = metadata.clone();
            }
        }
        Ok(())
    }

    /// Execute a notebook edit operation.
    ///
    /// Reads the file, validates it as an nbformat v4 notebook, applies all
    /// operations in order, and writes the result back to the same path.
    ///
    /// The reconstructed notebook preserves `nbformat`, `nbformat_minor`, and
    /// the top-level `metadata` field from the original file.
    ///
    /// # Errors
    ///
    /// - [`NotebookEditError::SandboxViolation`] if the path escapes the sandbox.
    /// - [`NotebookEditError::NotFound`] if the file does not exist.
    /// - [`NotebookEditError::Io`] on other I/O failures.
    /// - [`NotebookEditError::InvalidNotebook`] if the file is not a valid nbformat v4 notebook.
    /// - [`NotebookEditError::IndexOutOfBounds`] if any operation targets an invalid index.
    pub async fn run(
        &self,
        input: NotebookEditInput,
    ) -> Result<NotebookEditOutput, NotebookEditError> {
        let resolved =
            self.sandbox
                .resolve(&input.path)
                .map_err(|_| NotebookEditError::SandboxViolation {
                    path: input.path.clone(),
                })?;

        let raw = fs::read_to_string(&resolved)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => NotebookEditError::NotFound {
                    path: input.path.clone(),
                },
                _ => NotebookEditError::Io {
                    path: input.path.clone(),
                    reason: e.to_string(),
                },
            })?;

        let mut notebook: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| NotebookEditError::InvalidNotebook)?;

        let nbformat = notebook
            .get("nbformat")
            .and_then(|v| v.as_u64())
            .ok_or(NotebookEditError::InvalidNotebook)?;

        if nbformat != 4 {
            return Err(NotebookEditError::InvalidNotebook);
        }

        let raw_cells = notebook
            .get("cells")
            .and_then(|v| v.as_array())
            .ok_or(NotebookEditError::InvalidNotebook)?
            .clone();

        let mut cells: Vec<JupyterCell> =
            serde_json::from_value(serde_json::Value::Array(raw_cells))
                .map_err(|_| NotebookEditError::InvalidNotebook)?;

        for op in &input.operations {
            Self::apply_operation(&mut cells, op)?;
        }

        let cells_value = serde_json::to_value(&cells).map_err(|e| NotebookEditError::Io {
            path: input.path.clone(),
            reason: e.to_string(),
        })?;

        notebook["cells"] = cells_value;

        let serialized =
            serde_json::to_string_pretty(&notebook).map_err(|e| NotebookEditError::Io {
                path: input.path.clone(),
                reason: e.to_string(),
            })?;

        fs::write(&resolved, serialized)
            .await
            .map_err(|e| NotebookEditError::Io {
                path: input.path.clone(),
                reason: e.to_string(),
            })?;

        Ok(NotebookEditOutput {
            operations_applied: input.operations.len(),
            cell_count: cells.len(),
        })
    }

    /// Return the tool descriptor for registration in the ToolRegistry.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "notebook_edit".to_string(),
            version: "1.0.0".to_string(),
            description: "Edit a Jupyter .ipynb notebook via atomic cell operations: \
                          edit cell source (outputs cleared), insert cell, delete cell, \
                          or update cell metadata. Operations are applied in order. \
                          Only supports nbformat v4."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path inside the sandbox to a .ipynb file"
                    },
                    "operations": {
                        "type": "array",
                        "description": "Ordered list of atomic operations to apply",
                        "items": {
                            "type": "object",
                            "required": ["op"],
                            "properties": {
                                "op": {
                                    "type": "string",
                                    "enum": ["edit_cell", "insert_cell", "delete_cell", "edit_cell_metadata"]
                                }
                            }
                        }
                    }
                },
                "required": ["path", "operations"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "operations_applied": {
                        "type": "integer",
                        "description": "Number of operations successfully applied"
                    },
                    "cell_count": {
                        "type": "integer",
                        "description": "Resulting cell count after all operations"
                    }
                },
                "required": ["operations_applied", "cell_count"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![
                "notebook".to_string(),
                "jupyter".to_string(),
                "edit".to_string(),
            ],
            dangerous: false,
            is_read_only: false,
            risk_score: 3,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::CellType;
    use tempfile::TempDir;

    fn make_cells(specs: &[(&str, Vec<&str>, bool)]) -> Vec<JupyterCell> {
        specs
            .iter()
            .map(|(ct, src, has_output)| {
                let cell_type = match *ct {
                    "code" => CellType::Code,
                    "markdown" => CellType::Markdown,
                    _ => CellType::Raw,
                };
                JupyterCell {
                    cell_type,
                    source: src.iter().map(|s| s.to_string()).collect(),
                    outputs: if *has_output {
                        vec![json!({"output_type": "stream", "text": ["hello"]})]
                    } else {
                        vec![]
                    },
                    metadata: serde_json::Value::Object(serde_json::Map::new()),
                }
            })
            .collect()
    }

    fn make_notebook_file(dir: &TempDir, cells: &[(&str, Vec<&str>, bool)]) -> PathBuf {
        let cell_array: Vec<serde_json::Value> = cells
            .iter()
            .map(|(ct, src, has_output)| {
                json!({
                    "cell_type": ct,
                    "source": src,
                    "outputs": if *has_output { vec![json!({"output_type": "stream"})] } else { vec![] },
                    "metadata": {}
                })
            })
            .collect();

        let notebook = json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": cell_array
        });

        let path = dir.path().join("notebook.ipynb");
        std::fs::write(&path, serde_json::to_string_pretty(&notebook).unwrap()).unwrap();
        path
    }

    #[test]
    fn edit_cell_clears_outputs() {
        // GIVEN a cell with non-empty outputs
        let mut cells = make_cells(&[("code", vec!["df.describe()"], true)]);
        assert!(!cells[0].outputs.is_empty());

        // WHEN applying EditCell with a new source
        let op = NotebookEditOp::EditCell {
            index: 0,
            new_source: vec!["df.head(10)".to_string()],
        };
        NotebookEdit::apply_operation(&mut cells, &op).unwrap();

        // THEN source is updated and outputs are cleared
        assert_eq!(cells[0].source, vec!["df.head(10)"]);
        assert!(cells[0].outputs.is_empty());
    }

    #[test]
    fn insert_cell_shifts_indices() {
        // GIVEN 3 cells
        let mut cells = make_cells(&[
            ("code", vec!["a = 1"], false),
            ("code", vec!["b = 2"], false),
            ("code", vec!["c = 3"], false),
        ]);

        // WHEN inserting a markdown cell at index 1
        let op = NotebookEditOp::InsertCell {
            index: 1,
            cell_type: CellType::Markdown,
            source: vec!["# Titre".to_string()],
        };
        NotebookEdit::apply_operation(&mut cells, &op).unwrap();

        // THEN notebook has 4 cells and the new cell is at index 1
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[1].cell_type, CellType::Markdown);
        assert_eq!(cells[1].source, vec!["# Titre"]);
    }

    #[test]
    fn delete_cell_removes_and_shifts() {
        // GIVEN 3 cells
        let mut cells = make_cells(&[
            ("code", vec!["a = 1"], false),
            ("markdown", vec!["# note"], false),
            ("code", vec!["b = 2"], false),
        ]);

        // WHEN deleting cell at index 2
        let op = NotebookEditOp::DeleteCell { index: 2 };
        NotebookEdit::apply_operation(&mut cells, &op).unwrap();

        // THEN notebook has 2 cells
        assert_eq!(cells.len(), 2);
    }

    #[test]
    fn index_out_of_bounds_returns_error() {
        // GIVEN 3 cells
        let mut cells = make_cells(&[
            ("code", vec!["a"], false),
            ("code", vec!["b"], false),
            ("code", vec!["c"], false),
        ]);

        // WHEN deleting cell at index 5
        let op = NotebookEditOp::DeleteCell { index: 5 };
        let result = NotebookEdit::apply_operation(&mut cells, &op);

        // THEN IndexOutOfBounds error is returned
        assert!(matches!(
            result,
            Err(NotebookEditError::IndexOutOfBounds { index: 5, count: 3 })
        ));
    }

    #[test]
    fn insert_at_end_is_valid() {
        // GIVEN 2 cells
        let mut cells = make_cells(&[("code", vec!["a"], false), ("code", vec!["b"], false)]);

        // WHEN inserting at index == len (append)
        let op = NotebookEditOp::InsertCell {
            index: 2,
            cell_type: CellType::Code,
            source: vec!["c".to_string()],
        };
        NotebookEdit::apply_operation(&mut cells, &op).unwrap();

        // THEN new cell is appended
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[2].source, vec!["c"]);
    }

    #[test]
    fn insert_out_of_bounds_returns_error() {
        // GIVEN 2 cells
        let mut cells = make_cells(&[("code", vec!["a"], false), ("code", vec!["b"], false)]);

        // WHEN inserting at index 5 (beyond len)
        let op = NotebookEditOp::InsertCell {
            index: 5,
            cell_type: CellType::Code,
            source: vec!["x".to_string()],
        };
        let result = NotebookEdit::apply_operation(&mut cells, &op);

        // THEN IndexOutOfBounds error is returned
        assert!(matches!(
            result,
            Err(NotebookEditError::IndexOutOfBounds { .. })
        ));
    }

    #[tokio::test]
    async fn run_edit_cell_clears_outputs_on_disk() {
        // GIVEN a notebook file with one code cell that has outputs
        let dir = TempDir::new().unwrap();
        make_notebook_file(&dir, &[("code", vec!["df.describe()"], true)]);

        let tool = NotebookEdit::new(dir.path().to_path_buf()).unwrap();

        // WHEN applying EditCell
        let input = NotebookEditInput {
            path: "notebook.ipynb".to_string(),
            operations: vec![NotebookEditOp::EditCell {
                index: 0,
                new_source: vec!["df.head(10)".to_string()],
            }],
        };
        let output = tool.run(input).await.unwrap();

        // THEN operation applied and outputs cleared
        assert_eq!(output.operations_applied, 1);
        assert_eq!(output.cell_count, 1);

        // Verify written file has cleared outputs
        let written = std::fs::read_to_string(dir.path().join("notebook.ipynb")).unwrap();
        let notebook: serde_json::Value = serde_json::from_str(&written).unwrap();
        let outputs = notebook["cells"][0]["outputs"].as_array().unwrap();
        assert!(outputs.is_empty());
    }

    #[tokio::test]
    async fn run_invalid_notebook_returns_error() {
        // GIVEN a JSON file that is not a notebook
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("bad.ipynb"), r#"{"hello": "world"}"#).unwrap();

        let tool = NotebookEdit::new(dir.path().to_path_buf()).unwrap();

        // WHEN editing it
        let result = tool
            .run(NotebookEditInput {
                path: "bad.ipynb".to_string(),
                operations: vec![],
            })
            .await;

        // THEN InvalidNotebook error is returned
        assert!(matches!(result, Err(NotebookEditError::InvalidNotebook)));
    }

    #[test]
    fn descriptor_is_valid() {
        let desc = NotebookEdit::descriptor();
        assert!(desc.validate().is_ok());
        assert!(!desc.is_read_only);
    }
}
