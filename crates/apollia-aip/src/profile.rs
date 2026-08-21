//! ProfileInterface: Python-facing proxy for the global user profile.
//!
//! Exposes `#[pyclass]` accessors that agents use via:
//!
//! ```python
//! await ctx.profile.set("name", "Ada")
//! ctx.profile.get("role")
//! ctx.profile.all()
//! ctx.profile.has("agents.hitl")
//! ```
//!
//! Reads are always available: every agent can recall the profile through
//! the `__user__` namespace.  Writes are gated by the manifest's
//! `user_memory_write` field, matching the rule that already governs
//! [`crate::memory::MemoryInterface::remember_user`].
//!
//! Implementation forwards to
//! [`apollia_memory::user_memory::UserMemoryRepository`], opened against the
//! canonical `<data_dir>/user_memory.db` file (the same one the desktop and CLI
//! read), so a fact stored through `ctx.profile` is immediately visible there.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3::IntoPyObjectExt;

use apollia_memory::user_memory::{ProfileEntry, WrittenBy};

/// Python-facing handle on the global user profile.
#[pyclass]
pub struct ProfileInterface {
    /// Canonical user-profile database (`<data_dir>/user_memory.db`), the SAME
    /// file the desktop `get_profile` command and the CLI `apollia profile`
    /// read. Writing here (instead of a separate `memory/__user__.db`) makes the
    /// facts an agent stores through `ctx.profile` show up in Settings > Profile,
    /// a single source of truth with no duplicate/orphaned copy.
    db_path: std::path::PathBuf,
    user_memory_writable: bool,
    /// Agent name used as the [`WrittenBy::Agent`] tag on writes coming from
    /// non-onboarding agents.  Onboarding agent writes use
    /// [`WrittenBy::Onboarding`] regardless of the agent name.
    agent_name: String,
    /// `true` when this interface represents the onboarding agent; writes are
    /// tagged [`WrittenBy::Onboarding`].
    is_onboarding: bool,
}

#[pymethods]
impl ProfileInterface {
    /// Returns the value of a profile field by key, or `None` when absent.
    fn get<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let db_path = self.db_path.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = tokio::task::spawn_blocking(move || profile_get(&db_path, &key))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(PyRuntimeError::new_err)?;
            Python::with_gil(|py| match value {
                Some(v) => v.into_py_any(py),
                None => Ok(py.None()),
            })
        })
    }

    /// Returns `True` when the given key is present in the profile.
    fn has<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let db_path = self.db_path.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let present = tokio::task::spawn_blocking(move || profile_get(&db_path, &key))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(PyRuntimeError::new_err)?
                .is_some();
            Python::with_gil(|py| {
                let py_bool = pyo3::types::PyBool::new(py, present);
                Ok(py_bool.to_owned().into_any().unbind())
            })
        })
    }

    /// Returns a dict mapping every profile key to its value.
    fn all<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let db_path = self.db_path.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let entries = tokio::task::spawn_blocking(move || profile_list_all(&db_path))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(PyRuntimeError::new_err)?;
            Python::with_gil(|py| {
                let dict = PyDict::new(py);
                for entry in entries {
                    dict.set_item(entry.key, entry.value)?;
                }
                Ok(dict.into_any().unbind())
            })
        })
    }

    /// Returns the list of canonical profile keys defined by
    /// [`apollia_memory::profile_schema::PROFILE_SCHEMA`].
    fn schema_keys<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let keys = apollia_memory::profile_schema::canonical_keys();
        let list = PyList::new(py, &keys)?;
        Ok(list.into_any())
    }

    /// Sets or updates a profile field.
    ///
    /// Requires `user_memory_write = true` in the agent manifest, otherwise
    /// raises `RuntimeError`.  The `user.` prefix on `key` is stripped to
    /// preserve compatibility with the historical `remember_user("user.X")`
    /// call sites.
    fn set<'py>(&self, py: Python<'py>, key: String, value: String) -> PyResult<Bound<'py, PyAny>> {
        if !self.user_memory_writable {
            return Err(PyRuntimeError::new_err(
                "user profile write not permitted: manifest must declare user_memory_write = true",
            ));
        }
        let db_path = self.db_path.clone();
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || profile_set(&db_path, &key, &value, written_by))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(PyRuntimeError::new_err)?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Sets or updates several profile fields in one round-trip.
    ///
    /// `entries` is a Python dict mapping `key -> value`.  Permission rules
    /// match [`Self::set`].
    fn update<'py>(
        &self,
        py: Python<'py>,
        entries: Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.user_memory_writable {
            return Err(PyRuntimeError::new_err(
                "user profile write not permitted: manifest must declare user_memory_write = true",
            ));
        }
        let mut pairs: Vec<(String, String)> = Vec::with_capacity(entries.len());
        for (k, v) in entries.iter() {
            let key: String = k.extract()?;
            let value: String = v.extract()?;
            pairs.push((key, value));
        }
        let db_path = self.db_path.clone();
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                for (key, value) in &pairs {
                    profile_set(&db_path, key, value, written_by.clone())?;
                }
                Ok::<(), String>(())
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
            .map_err(PyRuntimeError::new_err)?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Returns `True` when this profile interface allows writes.
    #[getter]
    fn writable(&self) -> bool {
        self.user_memory_writable
    }

    fn __repr__(&self) -> String {
        format!(
            "ProfileInterface(agent={}, writable={})",
            self.agent_name, self.user_memory_writable
        )
    }
}

impl ProfileInterface {
    /// Constructs a profile interface for the runtime context.
    ///
    /// `user_memory_writable` mirrors the manifest's `user_memory_write`
    /// field.  `is_onboarding` tags writes with [`WrittenBy::Onboarding`];
    /// set this to `true` only for the onboarding agent.
    pub fn new(
        db_path: std::path::PathBuf,
        agent_name: String,
        user_memory_writable: bool,
        is_onboarding: bool,
    ) -> Self {
        Self {
            db_path,
            user_memory_writable,
            agent_name,
            is_onboarding,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure Rust internals, testable without PyO3
// ---------------------------------------------------------------------------

fn open_repo(
    db_path: &std::path::Path,
) -> Result<apollia_memory::user_memory::UserMemoryRepository, String> {
    apollia_memory::user_memory::UserMemoryRepository::new(db_path)
        .map_err(|e| format!("open user memory repo: {e}"))
}

fn profile_get(db_path: &std::path::Path, key: &str) -> Result<Option<String>, String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key);
    let repo = open_repo(db_path)?;
    let entry = repo.get(flat_key).map_err(|e| format!("get: {e}"))?;
    Ok(entry.map(|e| e.value))
}

fn profile_list_all(db_path: &std::path::Path) -> Result<Vec<ProfileEntry>, String> {
    let repo = open_repo(db_path)?;
    repo.list_all().map_err(|e| format!("list_all: {e}"))
}

fn profile_set(
    db_path: &std::path::Path,
    key: &str,
    value: &str,
    written_by: WrittenBy,
) -> Result<(), String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key);
    let repo = open_repo(db_path)?;
    repo.set(flat_key, value, written_by)
        .map_err(|e| format!("set: {e}"))
}
