//! ProfileInterface: Python-facing proxy for the user profile an execution
//! runs with.
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
//! The profile served is resolved at each call: the profile the user
//! designated for the agent (its `profile` override in `agents.db`), else the
//! default profile ([`apollia_runtime::profile::profile_for_agent`]). It
//! carries five fields (`name`, `role`, `function`, `tools`, `preferences`)
//! and a free text (`free_text`); a key outside these reads and writes a
//! free-form entry, shared by every profile.
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

use apollia_memory::user_memory::{Profile, ProfileEntry, WrittenBy};

/// Python-facing handle on the user profile resolved for the agent.
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
        let agent = self.agent_name.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = tokio::task::spawn_blocking(move || profile_get(&db_path, &agent, &key))
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
        let agent = self.agent_name.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let present = tokio::task::spawn_blocking(move || profile_get(&db_path, &agent, &key))
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
        let agent = self.agent_name.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let entries = tokio::task::spawn_blocking(move || profile_list_all(&db_path, &agent))
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
        let agent = self.agent_name.clone();
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                profile_set(&db_path, &agent, &key, &value, written_by)
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
        let db_path = self.db_path.clone();
        let agent = self.agent_name.clone();
        let written_by = if self.is_onboarding {
            WrittenBy::Onboarding
        } else {
            WrittenBy::Agent(self.agent_name.clone())
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                for (key, value) in &pairs {
                    profile_set(&db_path, &agent, key, value, written_by.clone())?;
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

/// The profile `agent` runs with: its designated one, else the default. The
/// designation is read from the `agents.db` beside `user_memory.db`.
fn resolved_profile(
    repo: &apollia_memory::user_memory::UserMemoryRepository,
    db_path: &std::path::Path,
    agent: &str,
) -> Result<Profile, String> {
    let data_dir = db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    apollia_runtime::profile::profile_for_agent(repo, data_dir, agent)
        .map_err(|e| format!("resolve profile: {e}"))
}

fn profile_get(
    db_path: &std::path::Path,
    agent: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key);
    let repo = open_repo(db_path)?;
    let profile = resolved_profile(&repo, db_path, agent)?;
    let entry = repo
        .get_in(&profile, flat_key)
        .map_err(|e| format!("get: {e}"))?;
    Ok(entry.map(|e| e.value))
}

fn profile_list_all(db_path: &std::path::Path, agent: &str) -> Result<Vec<ProfileEntry>, String> {
    let repo = open_repo(db_path)?;
    let profile = resolved_profile(&repo, db_path, agent)?;
    repo.list_all_in(&profile)
        .map_err(|e| format!("list_all: {e}"))
}

fn profile_set(
    db_path: &std::path::Path,
    agent: &str,
    key: &str,
    value: &str,
    written_by: WrittenBy,
) -> Result<(), String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key);
    let repo = open_repo(db_path)?;
    let profile = resolved_profile(&repo, db_path, agent)?;
    repo.set_in(&profile, flat_key, value, written_by)
        .map_err(|e| format!("set: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::user_memory::UserMemoryRepository;
    use apollia_tools::{AgentOrigin, AgentRepository, OverrideField, StoredAgent};

    fn designate(data_dir: &std::path::Path, agent: &str, profile_id: &str) {
        let repo = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(data_dir))
            .expect("agents.db opens");
        repo.save_definition(&StoredAgent {
            name: agent.to_owned(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1".to_owned(),
            definition: serde_json::json!({ "id": agent, "name": agent, "mission": "" }),
            enabled: true,
            installed_at: "2026-10-03T00:00:00Z".to_owned(),
            updated_at: "2026-10-03T00:00:00Z".to_owned(),
        })
        .expect("agent stored");
        repo.set_override(
            agent,
            OverrideField::Profile,
            &serde_json::json!(profile_id),
        )
        .expect("override set");
    }

    #[test]
    fn ctx_profile_serves_the_designated_profile_then_the_default() {
        // GIVEN a default profile with a role, a "Formateur" profile with another, and an agent designating it
        let dir = tempfile::tempdir().expect("tempdir");
        let db = apollia_core::paths::DataFile::UserMemory.path(dir.path());
        let repo = UserMemoryRepository::new(&db).expect("user_memory.db opens");
        repo.set("role", "Responsable SI", WrittenBy::User)
            .expect("default role");
        let formateur = repo.create_profile("Formateur").expect("created");
        repo.set_profile_field(&formateur.id, "role", Some("Formateur"))
            .expect("formateur role");
        drop(repo);
        designate(dir.path(), "cours", &formateur.id);

        // WHEN the designated agent and another one read the role, and the designated one writes a field
        let for_cours = profile_get(&db, "cours", "role").expect("read");
        let for_other = profile_get(&db, "veille", "user.role").expect("read");
        profile_set(
            &db,
            "cours",
            "tools",
            "Moodle",
            WrittenBy::Agent("cours".into()),
        )
        .expect("write");
        let all_cours = profile_list_all(&db, "cours").expect("list");
        let all_other = profile_list_all(&db, "veille").expect("list");

        // THEN each reads its own profile, and the write landed in the designated one only
        assert_eq!(for_cours.as_deref(), Some("Formateur"));
        assert_eq!(for_other.as_deref(), Some("Responsable SI"));
        assert!(all_cours
            .iter()
            .any(|e| e.key == "tools" && e.value == "Moodle"));
        assert!(!all_other.iter().any(|e| e.key == "tools"));
    }
}
