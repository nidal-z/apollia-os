//! ctx.datasources: runtime YAML datasources access.
//!
//! Loads the YAML files declared in `@agent(datasources=...)` from
//! `<agent_dir>/datasources/<name>.yaml` at agent startup, caches the parsed
//! values as `serde_json::Value`, and exposes them to Python via
//! `ctx.datasources.get("name")`, which returns a Python `dict`/`list`/scalar
//! directly.
//!
//! Gating uses the `declared` list propagated from the manifest: an undeclared
//! datasource triggers `FileNotFoundError` even if the file exists on disk
//! (least-privilege principle).
//!
//! Passing Python structures uses `json.loads()` to avoid an external
//! `pythonize` dependency (we stay on the Python stdlib for the conversion).
//! This is sufficient because datasources are loaded once at boot and read
//! rarely.

use std::collections::HashMap;
use std::path::Path;

use pyo3::exceptions::PyFileNotFoundError;
use pyo3::prelude::*;

/// Internal errors of the datasources layer.
#[derive(Debug, thiserror::Error)]
pub enum DatasourcesError {
    /// The YAML file is missing despite being declared in the manifest.
    #[error("datasource file not found: {0}")]
    FileNotFound(String),
    /// The YAML content could not be parsed.
    #[error("invalid YAML in datasource '{name}': {reason}")]
    InvalidYaml {
        /// Logical datasource name (manifest key).
        name: String,
        /// Error message from the YAML parser.
        reason: String,
    },
    /// The JSON to Python conversion failed on the `json.loads` side.
    #[error("python conversion failed for '{name}': {reason}")]
    PythonConversion {
        /// Name of the datasource involved.
        name: String,
        /// Python detail.
        reason: String,
    },
}

/// Read-only interface exposed to the agent via `ctx.datasources`.
///
/// During bootstrap, the runtime calls [`Self::load_from_dir`] to fill the
/// `values` cache. The agent only sees the declared datasources; `declared`
/// acts as a guard rail even if the filesystem contains other files.
#[pyclass(name = "DatasourcesInterface", module = "apollia._native")]
pub struct DatasourcesInterface {
    /// Parsed YAML values, keyed by logical name (manifest key).
    ///
    /// `serde_json::Value` is used as the pivot representation to allow
    /// conversion to Python via `json.loads` without an extra dependency.
    values: HashMap<String, serde_json::Value>,
    /// List of allowed datasources, a copy of the manifest. Any key not
    /// present here triggers `FileNotFoundError`, regardless of the disk.
    declared: Vec<String>,
}

#[pymethods]
impl DatasourcesInterface {
    /// Returns the parsed content of datasource `name` as a native Python
    /// object (dict, list, scalar depending on the YAML).
    ///
    /// # Python errors
    /// - `FileNotFoundError` if `name` is not in the manifest's declared list.
    /// - `FileNotFoundError` if `name` is declared but the YAML file is absent
    ///   or was not loaded (parse error).
    /// - `RuntimeError` if the JSON to Python conversion via `json.loads`
    ///   fails (should never happen for well-formed YAML).
    fn get(&self, py: Python<'_>, name: &str) -> PyResult<PyObject> {
        if !self.declared.iter().any(|d| d == name) {
            return Err(PyFileNotFoundError::new_err(format!(
                "Datasource '{name}' not declared in @agent(datasources=...)"
            )));
        }
        let value = self.values.get(name).ok_or_else(|| {
            PyFileNotFoundError::new_err(format!(
                "Datasource '{name}' declared but not found on disk \
                 (expected: datasources/{name}.yaml)"
            ))
        })?;

        // Convert serde_json::Value to PyObject via json.loads (avoids the
        // pythonize dependency).
        let json_str = serde_json::to_string(value).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "JSON serialization failed for '{name}': {e}"
            ))
        })?;
        let json_mod = py.import("json")?;
        let py_obj = json_mod.call_method1("loads", (json_str,))?.unbind();
        Ok(py_obj)
    }

    /// Lists the logical names of the datasources declared in the manifest.
    ///
    /// Always returned in manifest order. Lets the agent check its own
    /// configuration at startup without hardcoding names.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }

    /// `True` if the datasource is declared AND loaded successfully.
    ///
    /// Useful for agents that want to degrade gracefully when a YAML file is
    /// absent (`ctx.datasources.has("foo")` is more idiomatic than a
    /// try/except around `get`).
    fn has(&self, name: &str) -> bool {
        self.declared.iter().any(|d| d == name) && self.values.contains_key(name)
    }
}

impl DatasourcesInterface {
    /// Builds the interface with the manifest's declared list. The `values`
    /// cache stays empty until [`Self::load_from_dir`] has been called.
    pub fn new(declared: Vec<String>) -> Self {
        Self {
            values: HashMap::new(),
            declared,
        }
    }

    /// Loads all declared datasources from
    /// `<agent_dir>/datasources/<name>.yaml`.
    ///
    /// Parse errors are logged (`warn!`) but not globally fatal: a corrupt
    /// file makes the datasource invisible to Python (`FileNotFoundError` on
    /// the `get()` call). This is consistent with fail-fast because the agent
    /// gets a clear error on first access, not a silently empty value.
    ///
    /// Returns the number of datasources loaded successfully.
    pub fn load_from_dir(&mut self, agent_dir: &Path) -> usize {
        let dir = agent_dir.join("datasources");
        let mut loaded = 0usize;
        for name in self.declared.clone() {
            let path = dir.join(format!("{name}.yaml"));
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    // .yml fallback?
                    let alt = dir.join(format!("{name}.yml"));
                    match std::fs::read_to_string(&alt) {
                        Ok(c) => c,
                        Err(_) => {
                            tracing::warn!(
                                target: "apollia.aip.datasources",
                                name = %name,
                                path = %path.display(),
                                error = %e,
                                "datasource declared but file missing"
                            );
                            continue;
                        }
                    }
                }
            };
            match serde_yaml::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    self.values.insert(name.clone(), val);
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        target: "apollia.aip.datasources",
                        name = %name,
                        error = %e,
                        "datasource parse error"
                    );
                }
            }
        }
        loaded
    }

    /// Injects a value directly into the cache (useful for unit tests without
    /// a filesystem).
    #[cfg(test)]
    pub(crate) fn inject(&mut self, name: &str, value: serde_json::Value) {
        if !self.declared.iter().any(|d| d == name) {
            self.declared.push(name.to_string());
        }
        self.values.insert(name.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undeclared_returns_file_not_found() {
        // GIVEN an interface with declared=[]
        let ds = DatasourcesInterface::new(vec![]);

        // WHEN we ask for an undeclared name from Python
        Python::with_gil(|py| {
            let res = ds.get(py, "competitors");
            // THEN FileNotFoundError is raised
            assert!(res.is_err(), "expected FileNotFoundError");
            let err = res.expect_err("error");
            let msg = format!("{err}");
            assert!(
                msg.contains("not declared"),
                "expected 'not declared' in: {msg}"
            );
        });
    }

    #[test]
    fn test_declared_missing_on_disk_returns_file_not_found() {
        // GIVEN a datasource declared but no value cached
        let ds = DatasourcesInterface::new(vec!["competitors".to_string()]);

        // WHEN we ask for it
        Python::with_gil(|py| {
            let res = ds.get(py, "competitors");
            assert!(res.is_err(), "expected FileNotFoundError");
            let msg = format!("{}", res.expect_err("err"));
            assert!(
                msg.contains("not found on disk"),
                "expected 'not found on disk' in: {msg}"
            );
        });
    }

    #[test]
    fn test_list_names_returns_declared_in_order() {
        // GIVEN three declared datasources
        let ds = DatasourcesInterface::new(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        // THEN list_names returns them in order
        assert_eq!(ds.list_names(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_has_reflects_load_state() {
        // GIVEN a datasource declared but not loaded
        let mut ds = DatasourcesInterface::new(vec!["competitors".to_string()]);
        assert!(!ds.has("competitors"));

        // WHEN we inject a value
        ds.inject("competitors", serde_json::json!([{"name": "OpenAI"}]));

        // THEN has() returns true
        assert!(ds.has("competitors"));
        assert!(!ds.has("unknown"));
    }

    /// Checks the production path: `load_from_dir` reads a real YAML file from
    /// `<agent_dir>/datasources/<name>.yaml` and exposes it to the Python
    /// runtime via `get()`.
    #[test]
    fn test_load_from_dir_parses_real_yaml() {
        // GIVEN a temp agent_dir containing datasources/competitors.yaml
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir datasources");
        std::fs::write(
            ds_dir.join("competitors.yaml"),
            "- name: OpenAI\n  rank: 1\n- name: Anthropic\n  rank: 2\n",
        )
        .expect("write yaml");

        // WHEN we load via the production path
        let mut iface = DatasourcesInterface::new(vec!["competitors".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN one datasource was loaded
        assert_eq!(loaded, 1);
        assert!(iface.has("competitors"));

        // AND Python sees a list of two dicts
        Python::with_gil(|py| {
            let obj = iface
                .get(py, "competitors")
                .expect("get should succeed after load_from_dir");
            let len: usize = obj
                .bind(py)
                .call_method0("__len__")
                .expect("len")
                .extract()
                .expect("usize");
            assert_eq!(len, 2);
        });
    }

    /// Checks the `.yml` fallback when `.yaml` is absent.
    #[test]
    fn test_load_from_dir_yml_fallback() {
        // GIVEN datasources/config.yml (not .yaml)
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("config.yml"), "enabled: true\n").expect("write");

        // WHEN we load
        let mut iface = DatasourcesInterface::new(vec!["config".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN the .yml file was picked up
        assert_eq!(loaded, 1);
        assert!(iface.has("config"));
    }

    /// Checks that a missing datasource does not prevent the others from
    /// loading (logs warn! but no global failure, matching the `load_from_dir`
    /// comment).
    #[test]
    fn test_load_from_dir_missing_file_is_non_fatal() {
        // GIVEN datasources/present.yaml exists, declared also includes "missing"
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("present.yaml"), "value: 42\n").expect("write");

        // WHEN we load both
        let mut iface =
            DatasourcesInterface::new(vec!["present".to_string(), "missing".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN only present was loaded, no panic on missing
        assert_eq!(loaded, 1);
        assert!(iface.has("present"));
        assert!(!iface.has("missing"));
    }

    /// A malformed YAML is skipped (warn!), not a boot crash.
    #[test]
    fn test_load_from_dir_invalid_yaml_is_non_fatal() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        // A tab at the start of a mapping = invalid YAML.
        std::fs::write(ds_dir.join("broken.yaml"), "\t- not\n  valid").expect("write");

        let mut iface = DatasourcesInterface::new(vec!["broken".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // load_from_dir doesn't propagate the parse error, just logs it.
        assert_eq!(loaded, 0);
        assert!(!iface.has("broken"));
    }

    #[test]
    fn test_get_returns_python_object_via_json_loads() {
        // GIVEN a declared datasource with a parsed YAML value
        let mut ds = DatasourcesInterface::new(vec!["entries".to_string()]);
        ds.inject(
            "entries",
            serde_json::json!([
                {"name": "Apollia", "rank": 1},
                {"name": "Other", "rank": 2}
            ]),
        );

        // WHEN we read it from Python
        Python::with_gil(|py| {
            let obj = ds.get(py, "entries").expect("get should succeed");
            // THEN it's a Python list of dicts
            let len: usize = obj
                .bind(py)
                .call_method0("__len__")
                .expect("len")
                .extract()
                .expect("usize");
            assert_eq!(len, 2);
        });
    }
}
