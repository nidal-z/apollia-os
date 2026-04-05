//! Read and format Jupyter notebook files for LLM consumption.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::SandboxRoot;
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;

/// Read and format the cells of a Jupyter `.ipynb` notebook.
///
/// Validates that the file is a valid nbformat v4 notebook, then returns a
/// human-readable, line-numbered representation of all cells suitable for LLM
/// consumption. This tool is read-only and never modifies the notebook file.
#[derive(Debug, Clone)]
pub struct NotebookRead {
    sandbox: SandboxRoot,
}

/// Errors produced by [`NotebookRead`].
#[derive(Debug, Error)]
pub enum NotebookReadError {
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

    /// I/O error while reading the file.
    #[error("I/O error on '{path}': {reason}")]
    Io {
        /// The affected path.
        path: String,
        /// Low-level OS error description.
        reason: String,
    },

    /// The file is not a valid Jupyter notebook (missing or wrong `nbformat`).
    #[error("not a valid Jupyter notebook")]
    InvalidNotebook,
}

/// Input for a notebook read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookReadInput {
    /// Relative path inside the sandbox to a `.ipynb` file.
    pub path: String,
}

/// Output of a notebook read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookReadOutput {
    /// Formatted cell listing suitable for LLM consumption.
    ///
    /// Each cell is prefixed with `Cell N [type]:` followed by the
    /// source lines indented by two spaces.
    pub content: String,
    /// Total number of cells in the notebook.
    pub cell_count: usize,
}

impl NotebookRead {
    /// Create a new [`NotebookRead`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`NotebookReadError::Io`] if the sandbox root cannot be created
    /// or canonicalized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, NotebookReadError> {
        let sandbox = SandboxRoot::new(sandbox_root).map_err(|e| NotebookReadError::Io {
            path: "sandbox_root".to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self { sandbox })
    }

    /// Parse a raw JSON string and produce a formatted [`NotebookReadOutput`].
    ///
    /// Separated from file I/O so the formatting logic can be unit-tested
    /// without touching the filesystem.
    ///
    /// # Errors
    ///
    /// - [`NotebookReadError::InvalidNotebook`] if `json` cannot be parsed or
    ///   does not contain `"nbformat": 4`.
    pub fn parse_and_format(json: &str) -> Result<NotebookReadOutput, NotebookReadError> {
        let notebook: serde_json::Value =
            serde_json::from_str(json).map_err(|_| NotebookReadError::InvalidNotebook)?;

        let nbformat = notebook
            .get("nbformat")
            .and_then(|v| v.as_u64())
            .ok_or(NotebookReadError::InvalidNotebook)?;

        if nbformat != 4 {
            return Err(NotebookReadError::InvalidNotebook);
        }

        let cells = notebook
            .get("cells")
            .and_then(|v| v.as_array())
            .ok_or(NotebookReadError::InvalidNotebook)?;

        let mut parts: Vec<String> = Vec::with_capacity(cells.len());

        for (idx, cell) in cells.iter().enumerate() {
            let cell_type = cell
                .get("cell_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let source: String = cell
                .get("source")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();

            let indented = source
                .lines()
                .map(|line| format!("  {line}"))
                .collect::<Vec<_>>()
                .join("\n");

            parts.push(format!("Cell {idx} [{cell_type}]:\n{indented}"));
        }

        let content = parts.join("\n");

        Ok(NotebookReadOutput {
            content,
            cell_count: cells.len(),
        })
    }

    /// Execute a notebook read operation.
    ///
    /// Reads the file at `input.path` (resolved inside the sandbox), validates
    /// that it is a valid nbformat v4 notebook, and returns formatted cell
    /// content.
    ///
    /// # Errors
    ///
    /// - [`NotebookReadError::SandboxViolation`] if the path escapes the sandbox.
    /// - [`NotebookReadError::NotFound`] if the file does not exist.
    /// - [`NotebookReadError::Io`] on other I/O failures.
    /// - [`NotebookReadError::InvalidNotebook`] if the file is not a valid nbformat v4 notebook.
    pub async fn run(
        &self,
        input: NotebookReadInput,
    ) -> Result<NotebookReadOutput, NotebookReadError> {
        let resolved =
            self.sandbox
                .resolve(&input.path)
                .map_err(|_| NotebookReadError::SandboxViolation {
                    path: input.path.clone(),
                })?;

        let raw = fs::read_to_string(&resolved)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => NotebookReadError::NotFound {
                    path: input.path.clone(),
                },
                _ => NotebookReadError::Io {
                    path: input.path.clone(),
                    reason: e.to_string(),
                },
            })?;

        Self::parse_and_format(&raw)
    }

    /// Return the tool descriptor for registration in the ToolRegistry.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "notebook_read".to_string(),
            version: "1.0.0".to_string(),
            description: "Read and format the cells of a Jupyter .ipynb notebook. \
                          Returns a human-readable listing of all cells (type + source) \
                          for LLM consumption. Only supports nbformat v4."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path inside the sandbox to a .ipynb file"
                    }
                },
                "required": ["path"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Formatted cell listing: 'Cell N [type]:\\n  source...'"
                    },
                    "cell_count": {
                        "type": "integer",
                        "description": "Total number of cells in the notebook"
                    }
                },
                "required": ["content", "cell_count"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![
                "notebook".to_string(),
                "jupyter".to_string(),
                "read".to_string(),
            ],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_notebook_json(cells: &[(&str, &str)]) -> String {
        let cell_array: Vec<serde_json::Value> = cells
            .iter()
            .map(|(cell_type, source)| {
                serde_json::json!({
                    "cell_type": cell_type,
                    "source": source.lines()
                        .enumerate()
                        .map(|(i, l)| {
                            if i < source.lines().count() - 1 {
                                format!("{l}\n")
                            } else {
                                l.to_string()
                            }
                        })
                        .collect::<Vec<_>>(),
                    "outputs": [],
                    "metadata": {}
                })
            })
            .collect();

        serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": cell_array
        })
        .to_string()
    }

    #[test]
    fn read_valid_notebook_formats_cells() {
        // GIVEN a valid ipynb v4 JSON with 3 cells (code + markdown + code)
        let json = make_notebook_json(&[
            ("code", "import pandas as pd\ndf = pd.read_csv('data.csv')"),
            ("markdown", "# Analyse"),
            ("code", "df.describe()"),
        ]);

        // WHEN parsing and formatting
        let output = NotebookRead::parse_and_format(&json).unwrap();

        // THEN all three cell headers appear in the output
        assert!(output.content.contains("Cell 0 [code]:"));
        assert!(output.content.contains("Cell 1 [markdown]:"));
        assert!(output.content.contains("Cell 2 [code]:"));
        assert_eq!(output.cell_count, 3);
    }

    #[test]
    fn not_jupyter_notebook_returns_invalid() {
        // GIVEN a JSON object without nbformat: 4
        let json = r#"{"some_field": "value"}"#;

        // WHEN parsing
        let result = NotebookRead::parse_and_format(json);

        // THEN InvalidNotebook error is returned
        assert!(matches!(result, Err(NotebookReadError::InvalidNotebook)));
    }

    #[test]
    fn wrong_nbformat_version_returns_invalid() {
        // GIVEN a JSON with nbformat: 3
        let json = r#"{"nbformat": 3, "cells": []}"#;

        // WHEN parsing
        let result = NotebookRead::parse_and_format(json);

        // THEN InvalidNotebook error is returned
        assert!(matches!(result, Err(NotebookReadError::InvalidNotebook)));
    }

    #[tokio::test]
    async fn run_nonexistent_file_returns_not_found() {
        // GIVEN a sandbox with no files
        let dir = TempDir::new().unwrap();
        let tool = NotebookRead::new(dir.path().to_path_buf()).unwrap();

        // WHEN reading a missing file
        let result = tool
            .run(NotebookReadInput {
                path: "missing.ipynb".to_string(),
            })
            .await;

        // THEN NotFound error is returned
        assert!(matches!(result, Err(NotebookReadError::NotFound { .. })));
    }

    #[tokio::test]
    async fn run_valid_notebook_file_returns_content() {
        // GIVEN a valid .ipynb file on disk
        let dir = TempDir::new().unwrap();
        let json = make_notebook_json(&[("code", "x = 1"), ("markdown", "## Section")]);
        let path = dir.path().join("test.ipynb");
        std::fs::write(&path, &json).unwrap();

        let tool = NotebookRead::new(dir.path().to_path_buf()).unwrap();

        // WHEN running the tool
        let output = tool
            .run(NotebookReadInput {
                path: "test.ipynb".to_string(),
            })
            .await
            .unwrap();

        // THEN cells are formatted correctly
        assert!(output.content.contains("Cell 0 [code]:"));
        assert!(output.content.contains("Cell 1 [markdown]:"));
        assert_eq!(output.cell_count, 2);
    }

    #[test]
    fn descriptor_is_valid() {
        let desc = NotebookRead::descriptor();
        assert!(desc.validate().is_ok());
        assert!(desc.is_read_only);
    }
}
