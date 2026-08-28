//! Python agent module loader via PyO3.
//!
//! Loads a `.py` file containing an AIP-compatible agent object.
//! The file must define a module-level `agent` variable (e.g. `agent = MyAgent()`).
//! The parent directory is added to `sys.path` so relative imports work.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Errors that can occur when loading a Python agent module.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
/// `site-packages` visible to the embedded Python interpreter; otherwise
/// duck-typing fails with `ModuleNotFoundError` for any package the agent
/// imports at top level (e.g. `matplotlib`, `openpyxl`).
///
/// Extra paths are inserted at the front of `sys.path`, but always behind the
/// agent's own parent directory (so local imports still take priority).
///
/// Every load starts by dropping the cached modules of the agent directory
/// (see [`purge_stale_modules`]), so a second load after a file change serves
/// the new code, helpers included.
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

        let sys_modules = sys
            .getattr("modules")
            .map_err(|e| AIPLoaderError::PythonError(format!("failed to get sys.modules: {e}")))?;

        // The embedded interpreter is process-wide and outlives every agent
        // start, so `sys.modules` is what makes a reload serve stale code.
        let parent = path.parent().unwrap_or(Path::new("."));
        purge_stale_modules(py, &sys_modules, parent, extra_sys_paths);

        // Invalidate finder caches so the next import re-reads from disk.
        if let Ok(importlib) = py.import("importlib") {
            let _ = importlib.call_method0("invalidate_caches");
        }

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

/// Drops from `sys.modules` every module the next import must re-read from
/// disk: the SDK, and anything the agent directory owns or shadows.
///
/// The interpreter is shared by every agent of the process, so a module
/// imported at the first start stays cached forever. Replacing an agent file
/// on disk and restarting the agent therefore reloaded the entry module while
/// its own helpers kept running the code of the previous version. The entry
/// module is re-executed by `PyModule::from_code` on every load, its siblings
/// are not, which is exactly the discrepancy this purge closes.
///
/// Selection goes through each module's `__file__`, never through its name: an
/// agent names its helpers as it pleases, and a name prefix would miss them.
fn purge_stale_modules(
    py: Python<'_>,
    sys_modules: &Bound<'_, PyAny>,
    load_dir: &Path,
    extra_sys_paths: &[PathBuf],
) {
    let load_dir = canonical_or_owned(load_dir);
    // A root that contains the agent directory would protect the very modules
    // this purge exists for, so it is dropped from the protection list.
    let protected: Vec<PathBuf> = interpreter_owned_roots(py, extra_sys_paths)
        .into_iter()
        .filter(|root| !load_dir.starts_with(root))
        .collect();

    // Materialise the pairs before reading them: a module attribute lookup can
    // run Python code, and a view iterated while the mapping changes raises.
    let snapshot = sys_modules
        .call_method0("items")
        .and_then(|items| py.import("builtins")?.call_method1("list", (items,)));
    let Ok(snapshot) = snapshot.and_then(|list| list.try_iter()) else {
        return;
    };

    let mut doomed: Vec<String> = Vec::new();
    for item in snapshot.flatten() {
        let Ok((name, module)) = item.extract::<(String, Bound<'_, PyAny>)>() else {
            continue;
        };
        // Cached `apollia.*` modules are dropped unconditionally: the SDK is
        // installed editable, so an update to it must be picked up without
        // restarting the daemon.
        if name == "apollia" || name.starts_with("apollia.") {
            doomed.push(name);
            continue;
        }
        let file = module
            .getattr("__file__")
            .ok()
            .and_then(|f| f.extract::<String>().ok());
        let Some(file) = file else {
            continue;
        };
        if is_stale_local_module(&name, Path::new(&file), &load_dir, &protected) {
            doomed.push(name);
        }
    }

    let purged = doomed.len();
    for name in doomed {
        let _ = sys_modules.call_method1("pop", (name, py.None()));
    }
    if purged > 0 {
        tracing::debug!(
            load_dir = %load_dir.display(),
            purged,
            "aip.loader.modules_purged"
        );
    }
}

/// Decides whether a cached module must leave `sys.modules` before an agent
/// living in `load_dir` is (re)loaded.
///
/// Two reasons, both of them about the file on disk:
/// 1. the cached module was read from `load_dir`, so the agent owns it and a
///    reload must re-read it,
/// 2. `load_dir` provides a source file for that dotted name while the cached
///    copy came from elsewhere, so the cached copy is a twin the import system
///    would shadow anyway (the agent directory sits at the front of
///    `sys.path`). This is the case of validating a file from the operator's
///    own folder while the previous version of the same helper is still
///    cached from the install directory.
///
/// Modules owned by the interpreter (standard library, installed packages) are
/// never dropped: a long-running process must not end up holding two copies of
/// a standard module. `load_dir` and `protected_roots` are expected in
/// canonical form, as [`interpreter_owned_roots`] returns them.
fn is_stale_local_module(
    name: &str,
    file: &Path,
    load_dir: &Path,
    protected_roots: &[PathBuf],
) -> bool {
    let file = canonical_or_owned(file);
    if protected_roots.iter().any(|root| file.starts_with(root)) {
        return false;
    }
    if file.starts_with(load_dir) {
        return true;
    }
    load_dir_defines(load_dir, name)
}

/// True when `load_dir` holds the source of the dotted module `name`, either
/// as `a/b.py` or as the `a/b/__init__.py` of a sub-package.
fn load_dir_defines(load_dir: &Path, name: &str) -> bool {
    let mut relative = PathBuf::new();
    for part in name.split('.') {
        if part.is_empty() || part == "." || part == ".." || part.contains('/') {
            return false;
        }
        relative.push(part);
    }
    let base = load_dir.join(relative);
    let mut module_file = base.clone().into_os_string();
    module_file.push(".py");
    Path::new(&module_file).is_file() || base.join("__init__.py").is_file()
}

/// Directory roots whose modules belong to the interpreter rather than to an
/// agent: standard library, platform library, and the `site-packages` of both
/// the running interpreter and the agent venvs.
fn interpreter_owned_roots(py: Python<'_>, extra_sys_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(sysconfig) = py.import("sysconfig") {
        if let Ok(paths) = sysconfig.call_method0("get_paths") {
            for key in ["stdlib", "platstdlib", "purelib", "platlib"] {
                if let Ok(value) = paths.get_item(key) {
                    if let Ok(dir) = value.extract::<String>() {
                        roots.push(PathBuf::from(dir));
                    }
                }
            }
        }
    }
    if let Ok(sys) = py.import("sys") {
        for attr in ["prefix", "base_prefix"] {
            if let Ok(value) = sys.getattr(attr) {
                if let Ok(dir) = value.extract::<String>() {
                    roots.push(PathBuf::from(dir));
                }
            }
        }
    }
    roots.extend(extra_sys_paths.iter().cloned());

    roots
        .into_iter()
        // A root of "/" or "" would protect the whole filesystem and turn the
        // purge into a no-op, so only real subdirectories are kept.
        .filter(|root| root.components().count() > 1)
        .map(|root| canonical_or_owned(&root))
        .collect()
}

/// Canonical form of `path`, falling back to the path itself when it cannot be
/// resolved (deleted file, permission denied).
fn canonical_or_owned(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temporary `.py` file in a directory of its own, and
    /// return that directory alongside the file so the caller keeps it alive.
    ///
    /// The directory is what matters. [`load_agent_module`] treats the parent
    /// of the file as the agent directory, and [`purge_stale_modules`] then
    /// drops from `sys.modules` every cached module whose source lives under
    /// it. A file created straight in the system temporary directory makes
    /// that parent the system temporary directory itself, so one test's load
    /// evicts the modules every other test loaded from its own tempdir under
    /// the same root. `sys.modules` is shared by the whole process, and
    /// CPython's `_bootstrap._load` pops the module it has just executed:
    /// popped by another thread first, the import raises
    /// `KeyError: '<module>'`, which is what the reload tests read under load.
    ///
    /// `module_name` is per-test because the stem becomes the module name, and
    /// two tests importing under one name would share a `sys.modules` entry.
    fn create_temp_py(module_name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = write_py(dir.path(), &format!("{module_name}.py"), content);
        (dir, path)
    }

    #[test]
    fn test_load_valid_agent_module() {
        // GIVEN a .py file with a valid agent object
        let (_dir, file) = create_temp_py(
            "valid_agent_module",
            "class MonAgent:\n    pass\nagent = MonAgent()\n",
        );

        // WHEN we load the module
        let result = load_agent_module(&file);

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
        let (_dir, file) = create_temp_py("syntax_error_module", "def broken(\n");

        // WHEN we attempt to load
        let result = load_agent_module(&file);

        // THEN we get ImportFailed
        assert!(matches!(result, Err(AIPLoaderError::ImportFailed { .. })));
    }

    #[test]
    fn test_load_no_agent_attribute() {
        // GIVEN a valid .py file without an 'agent' attribute
        let (_dir, file) = create_temp_py("no_agent_attribute_module", "x = 42\n");

        // WHEN we attempt to load
        let result = load_agent_module(&file);

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

    // ─── Stale module purge ───────────────────────────────────────────────

    /// Helper: write `content` into `dir/name`.
    fn write_py(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("failed to create py file");
        file.write_all(content.as_bytes())
            .expect("failed to write py file");
        path
    }

    /// Helper: read the integer `value` attribute of a loaded agent object.
    fn agent_value(agent: &Py<PyAny>) -> i64 {
        Python::with_gil(|py| {
            agent
                .bind(py)
                .getattr("value")
                .expect("agent has no 'value' attribute")
                .extract::<i64>()
                .expect("agent.value is not an integer")
        })
    }

    #[test]
    fn test_reload_serves_the_new_code_of_a_sibling_module() {
        pyo3::prepare_freethreaded_python();

        // GIVEN an agent whose entry module reads its answer from a sibling
        // module, loaded once so both modules sit in sys.modules
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        write_py(dir.path(), "purge_sibling_helper.py", "VALUE = 1\n");
        let entry = write_py(
            dir.path(),
            "purge_sibling_agent.py",
            "from purge_sibling_helper import VALUE\n\
             class SiblingAgent:\n\
             \x20   def __init__(self):\n\
             \x20       self.value = VALUE\n\
             agent = SiblingAgent()\n",
        );
        let first = load_agent_module(&entry).expect("first load failed");
        assert_eq!(agent_value(&first), 1);

        // WHEN the sibling is replaced on disk and the agent is loaded again,
        // exactly what the update command does before restarting the agent.
        // The new source differs in length from the old one: CPython validates
        // its bytecode cache on the pair (source size, source mtime in whole
        // seconds), and a same-size rewrite inside the same second would be
        // served from that cache whatever the purge does.
        write_py(dir.path(), "purge_sibling_helper.py", "VALUE = 222\n");
        let second = load_agent_module(&entry).expect("second load failed");

        // THEN the reloaded agent serves the new sibling code
        assert_eq!(agent_value(&second), 222);
    }

    #[test]
    fn test_reload_serves_the_new_code_of_a_sub_package() {
        pyo3::prepare_freethreaded_python();

        // GIVEN an agent importing from a sub-package of its own directory
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let package = dir.path().join("purge_sibling_pkg");
        std::fs::create_dir_all(&package).expect("failed to create package dir");
        write_py(&package, "__init__.py", "");
        write_py(&package, "rules.py", "VALUE = 10\n");
        let entry = write_py(
            dir.path(),
            "purge_package_agent.py",
            "from purge_sibling_pkg.rules import VALUE\n\
             class PackageAgent:\n\
             \x20   def __init__(self):\n\
             \x20       self.value = VALUE\n\
             agent = PackageAgent()\n",
        );
        let first = load_agent_module(&entry).expect("first load failed");
        assert_eq!(agent_value(&first), 10);

        // WHEN a module of the sub-package changes and the agent is reloaded
        write_py(&package, "rules.py", "VALUE = 2000\n");
        let second = load_agent_module(&entry).expect("second load failed");

        // THEN the new sub-package code is the one that answers
        assert_eq!(agent_value(&second), 2000);
    }

    #[test]
    fn test_purge_selects_a_module_owned_by_the_agent_directory() {
        // GIVEN a cached module whose source file lives in the agent directory
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let helper = write_py(dir.path(), "helper.py", "VALUE = 1\n");

        // WHEN the purge decision is taken for that directory
        let stale = is_stale_local_module("helper", &helper, dir.path(), &[]);

        // THEN the module is dropped so the next import re-reads the file
        assert!(stale);
    }

    #[test]
    fn test_purge_selects_a_twin_cached_from_another_directory() {
        // GIVEN a helper cached from an install directory, and an operator
        // folder that provides its own file of the same dotted name
        let installed = tempfile::tempdir().expect("failed to create temp dir");
        let cached = write_py(installed.path(), "helper.py", "VALUE = 1\n");
        let source = tempfile::tempdir().expect("failed to create temp dir");
        write_py(source.path(), "helper.py", "VALUE = 2\n");

        // WHEN the purge decision is taken for the operator folder
        let stale = is_stale_local_module("helper", &cached, source.path(), &[]);

        // THEN the cached twin goes, because validation must read the new file
        assert!(stale);
    }

    #[test]
    fn test_purge_keeps_an_unrelated_module() {
        // GIVEN a cached module unrelated to the agent directory
        let elsewhere = tempfile::tempdir().expect("failed to create temp dir");
        let cached = write_py(elsewhere.path(), "unrelated.py", "VALUE = 1\n");
        let dir = tempfile::tempdir().expect("failed to create temp dir");

        // WHEN the purge decision is taken for the agent directory
        let stale = is_stale_local_module("unrelated", &cached, dir.path(), &[]);

        // THEN it stays cached
        assert!(!stale);
    }

    #[test]
    fn test_purge_keeps_a_module_owned_by_the_interpreter() {
        // GIVEN a module cached from a protected root, shadowed by a file of
        // the same name sitting in the agent directory
        let library = tempfile::tempdir().expect("failed to create temp dir");
        let cached = write_py(library.path(), "json.py", "VALUE = 1\n");
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        write_py(dir.path(), "json.py", "VALUE = 2\n");
        let protected = vec![canonical_or_owned(library.path())];

        // WHEN the purge decision is taken for the agent directory
        let stale = is_stale_local_module("json", &cached, dir.path(), &protected);

        // THEN the standard module survives: the process must never hold two
        // copies of it
        assert!(!stale);
    }

    #[test]
    fn test_load_dir_defines_rejects_a_traversing_name() {
        // GIVEN a directory and a dotted name with an empty and a relative part
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        write_py(dir.path(), "helper.py", "VALUE = 1\n");

        // WHEN the directory is asked whether it defines these names
        let empty_part = load_dir_defines(dir.path(), "helper..sub");
        let traversal = load_dir_defines(dir.path(), "..helper");

        // THEN neither is treated as a module the directory provides
        assert!(!empty_part);
        assert!(!traversal);
    }
}
