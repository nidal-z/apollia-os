//! Python agent module loader via PyO3.
//!
//! Loads a `.py` file containing an AIP-compatible agent object.
//! The file must define a module-level `agent` variable (e.g. `agent = MyAgent()`).
//! The parent directory is added to `sys.path` so relative imports work.

use std::ffi::CString;
use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Errors that can occur when loading a Python agent module.
#[derive(Debug, thiserror::Error)]
pub enum AIPLoaderError {
    /// The specified file does not exist on disk.
    #[error("file not found: {0}")]
    FileNotFound(String),

    /// The path does not point to a `.py` file.
    #[error("invalid path (expected .py file): {0}")]
    InvalidPath(String),

    /// Python import failed (syntax error, missing import, etc.).
    #[error("Python import failed for '{module}': {reason}")]
    ImportFailed {
        /// Name of the module that failed to import.
        module: String,
        /// Human-readable reason (includes Python traceback).
        reason: String,
    },

    /// The module loaded successfully but has no `agent` attribute.
    #[error("no 'agent' attribute found in module '{0}'")]
    NoAgentFound(String),

    /// Generic Python runtime error.
    #[error("Python error: {0}")]
    PythonError(String),
}

/// Loads a Python agent module from a `.py` file.
///
/// The file must contain a module-level variable named `agent`.
/// The parent directory of the file is prepended to `sys.path`
/// so that relative imports within the module work correctly.
///
/// # Errors
///
/// - [`AIPLoaderError::InvalidPath`] if the extension is not `.py`
/// - [`AIPLoaderError::FileNotFound`] if the file does not exist
/// - [`AIPLoaderError::ImportFailed`] if Python execution fails
/// - [`AIPLoaderError::NoAgentFound`] if the module has no `agent` attribute
pub fn load_agent_module(path: &Path) -> Result<Py<PyAny>, AIPLoaderError> {
    load_agent_module_with_sys_paths(path, &[])
}

/// Like [`load_agent_module`], but inserts additional directories into
/// `sys.path` before importing.
///
/// Used by the package installer to make the agent's per-package venv
/// `site-packages` visible to the embedded Python interpreter — otherwise
/// duck-typing fails with `ModuleNotFoundError` for any package the agent
/// imports at top level (e.g. `matplotlib`, `openpyxl`).
///
/// Extra paths are inserted at the front of `sys.path`, but always behind the
/// agent's own parent directory (so local imports still take priority).
pub fn load_agent_module_with_sys_paths(
    path: &Path,
    extra_sys_paths: &[std::path::PathBuf],
) -> Result<Py<PyAny>, AIPLoaderError> {
    // 1. Validate .py extension
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if extension != "py" {
        return Err(AIPLoaderError::InvalidPath(path.display().to_string()));
    }

    // 2. Check file exists
    if !path.exists() {
        return Err(AIPLoaderError::FileNotFound(path.display().to_string()));
    }

    // 3. Read file contents
    let code = std::fs::read_to_string(path)
        .map_err(|e| AIPLoaderError::FileNotFound(format!("{}: {e}", path.display())))?;

    // 4. Derive module and file names
    let module_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("agent_module");
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("agent.py");

    // 5. Load via PyO3
    Python::with_gil(|py| {
        let sys = py
            .import("sys")
            .map_err(|e| AIPLoaderError::PythonError(format!("failed to import sys: {e}")))?;
        let sys_path = sys
            .getattr("path")
            .map_err(|e| AIPLoaderError::PythonError(format!("failed to get sys.path: {e}")))?;

        // Insert venv site-packages first. Iterate in reverse so the first
        // element of `extra_sys_paths` ends up at the front of sys.path.
        for extra in extra_sys_paths.iter().rev() {
            sys_path
                .call_method1("insert", (0, extra.to_string_lossy().as_ref()))
                .map_err(|e| {
                    AIPLoaderError::PythonError(format!(
                        "failed to insert venv path into sys.path: {e}"
                    ))
                })?;
        }

        // Add parent directory to sys.path AFTER venv paths so it takes
        // priority for local imports.
        let parent = path.parent().unwrap_or(Path::new("."));
        sys_path
            .call_method1("insert", (0, parent.to_string_lossy().as_ref()))
            .map_err(|e| {
                AIPLoaderError::PythonError(format!("failed to insert into sys.path: {e}"))
            })?;

        // Execute the module code
        let code_c = CString::new(code.as_bytes())
            .map_err(|e| AIPLoaderError::PythonError(format!("code contains NUL byte: {e}")))?;
        let file_c = CString::new(file_name).map_err(|e| {
            AIPLoaderError::PythonError(format!("file name contains NUL byte: {e}"))
        })?;
        let module_c = CString::new(module_name).map_err(|e| {
            AIPLoaderError::PythonError(format!("module name contains NUL byte: {e}"))
        })?;
        let module = PyModule::from_code(py, &code_c, &file_c, &module_c).map_err(|e| {
            AIPLoaderError::ImportFailed {
                module: module_name.to_owned(),
                reason: e.to_string(),
            }
        })?;

        // Extract the 'agent' attribute
        let agent = module
            .getattr("agent")
            .map_err(|_| AIPLoaderError::NoAgentFound(module_name.to_owned()))?;

        Ok(agent.into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temporary `.py` file with the given content.
    fn create_temp_py(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(".py")
            .tempfile()
            .expect("failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("failed to write temp file");
        file
    }

    #[test]
    fn test_load_valid_agent_module() {
        // GIVEN a .py file with a valid agent object
        let file = create_temp_py("class MonAgent:\n    pass\nagent = MonAgent()\n");

        // WHEN we load the module
        let result = load_agent_module(file.path());

        // THEN we get a valid Py<PyAny>
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_file_not_found() {
        // GIVEN a path to a non-existent file
        let path = Path::new("/tmp/apollia_test_inexistant_xyz.py");

        // WHEN we attempt to load
        let result = load_agent_module(path);

        // THEN we get FileNotFound
        assert!(matches!(result, Err(AIPLoaderError::FileNotFound(_))));
    }

    #[test]
    fn test_load_import_failed_syntax_error() {
        // GIVEN a .py file with a syntax error
        let file = create_temp_py("def broken(\n");

        // WHEN we attempt to load
        let result = load_agent_module(file.path());

        // THEN we get ImportFailed
        assert!(matches!(result, Err(AIPLoaderError::ImportFailed { .. })));
    }

    #[test]
    fn test_load_no_agent_attribute() {
        // GIVEN a valid .py file without an 'agent' attribute
        let file = create_temp_py("x = 42\n");

        // WHEN we attempt to load
        let result = load_agent_module(file.path());

        // THEN we get NoAgentFound
        assert!(matches!(result, Err(AIPLoaderError::NoAgentFound(_))));
    }

    #[test]
    fn test_load_invalid_path_not_py() {
        // GIVEN a path to a file without .py extension
        let file = tempfile::Builder::new()
            .suffix(".txt")
            .tempfile()
            .expect("failed to create temp file");

        // WHEN we attempt to load
        let result = load_agent_module(file.path());

        // THEN we get InvalidPath
        assert!(matches!(result, Err(AIPLoaderError::InvalidPath(_))));
    }
}
