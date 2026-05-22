//! ProfileInterface — Python-facing proxy for the global user profile.
//!
//! Exposes `#[pyclass]` accessors that agents use via:
//!
//! ```python
//! await ctx.profile.set("name", "Nidal")
//! ctx.profile.get("role")
//! ctx.profile.all()
//! ctx.profile.has("agents.hitl")
//! ```
//!
//! Reads are always available — every agent can recall the profile through
//! the `__user__` namespace.  Writes are gated by the manifest's
//! `user_memory_write` field, matching the rule that already governs
//! [`crate::memory::MemoryInterface::remember_user`].
//!
//! Implementation forwards to
//! [`apollia_memory::user_memory::UserMemoryRepository`] via the shared
//! [`MemoryManager`] held by the runtime.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use apollia_memory::manager::MemoryManager;
use apollia_memory::user_memory::{ProfileEntry, WrittenBy, USER_NAMESPACE};

/// Python-facing handle on the global user profile.
#[pyclass]
pub struct ProfileInterface {
    user_manager: Arc<Mutex<MemoryManager>>,
    user_memory_writable: bool,
    /// Agent name used as the [`WrittenBy::Agent`] tag on writes coming from
    /// non-onboarding agents.  Onboarding agent writes use
    /// [`WrittenBy::Onboarding`] regardless of the agent name.
    agent_name: String,
    /// `true` when this interface represents the onboarding agent — writes are
    /// tagged [`WrittenBy::Onboarding`].
    is_onboarding: bool,
}

#[pymethods]
impl ProfileInterface {
    /// Returns the value of a profile field by key, or `None` when absent.
    fn get<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let user_manager = Arc::clone(&self.user_manager);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = tokio::task::spawn_blocking(move || profile_get(&user_manager, &key))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(PyRuntimeError::new_err)?;
            Python::with_gil(|py| match value {
                Some(v) => Ok(v.into_pyobject(py).unwrap().into_any().unbind()),
                None => Ok(py.None()),
            })
        })
    }

    /// Returns `True` when the given key is present in the profile.
    fn has<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let user_manager = Arc::clone(&self.user_manager);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let present = tokio::task::spawn_blocking(move || profile_get(&user_manager, &key))
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
        let user_manager = Arc::clone(&self.user_manager);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let entries = tokio::task::spawn_blocking(move || profile_list_all(&user_manager))
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
        let user_manager = Arc::clone(&self.user_manager);
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                profile_set(&user_manager, &key, &value, written_by)
            })
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
        let user_manager = Arc::clone(&self.user_manager);
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                for (key, value) in &pairs {
                    profile_set(&user_manager, key, value, written_by.clone())?;
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
    /// field.  `is_onboarding` tags writes with [`WrittenBy::Onboarding`]
    /// — set this to `true` only for the onboarding agent.
    pub fn new(
        user_manager: MemoryManager,
        agent_name: String,
        user_memory_writable: bool,
        is_onboarding: bool,
    ) -> Self {
        Self {
            user_manager: Arc::new(Mutex::new(user_manager)),
            user_memory_writable,
            agent_name,
            is_onboarding,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure Rust internals — testable without PyO3
// ---------------------------------------------------------------------------

fn lock_manager(
    mgr: &Arc<Mutex<MemoryManager>>,
) -> Result<std::sync::MutexGuard<'_, MemoryManager>, String> {
    mgr.lock()
        .map_err(|e| format!("user memory mutex poisoned: {e}"))
}

fn profile_get(
    user_manager: &Arc<Mutex<MemoryManager>>,
    key: &str,
) -> Result<Option<String>, String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key).to_owned();
    let mut mgr = lock_manager(user_manager)?;
    let store = match mgr.store(USER_NAMESPACE) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let sem = apollia_memory::semantic::SemanticMemory::new(store);
    let entry = sem
        .recall(USER_NAMESPACE, &flat_key)
        .map_err(|e| format!("recall failed: {e}"))?;
    Ok(entry.and_then(|e| match e.value {
        serde_json::Value::String(s) => Some(s),
        other => Some(other.to_string()),
    }))
}

fn profile_list_all(user_manager: &Arc<Mutex<MemoryManager>>) -> Result<Vec<ProfileEntry>, String> {
    let db_path = {
        let mgr = lock_manager(user_manager)?;
        mgr.db_path_for(USER_NAMESPACE)
            .ok_or_else(|| "__user__ db path not available".to_owned())?
    };
    let repo = apollia_memory::user_memory::UserMemoryRepository::new(&db_path)
        .map_err(|e| format!("open user memory repo: {e}"))?;
    repo.list_all().map_err(|e| format!("list_all: {e}"))
}

fn profile_set(
    user_manager: &Arc<Mutex<MemoryManager>>,
    key: &str,
    value: &str,
    written_by: WrittenBy,
) -> Result<(), String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key).to_owned();
    let db_path = {
        let mgr = lock_manager(user_manager)?;
        mgr.db_path_for(USER_NAMESPACE)
            .ok_or_else(|| "__user__ db path not available".to_owned())?
    };
    let repo = apollia_memory::user_memory::UserMemoryRepository::new(&db_path)
        .map_err(|e| format!("open user memory repo: {e}"))?;
    repo.set(&flat_key, value, written_by)
        .map_err(|e| format!("set: {e}"))
}
