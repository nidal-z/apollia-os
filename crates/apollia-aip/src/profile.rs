//! ProfileInterface: Python-facing proxy for the user profile an execution
//! runs with.
//!
//! Exposes `#[pyclass]` accessors that agents use via:
//!
//! ```python
//! await ctx.profile.set("name", "Ada")
//! ctx.profile.get("role")
//! ctx.profile.all()
//! ctx.profile.has("role")
//! ```
//!
//! The profile served is resolved at each call: the profile the user
//! designated for the agent (its `profile` override in `agents.db`), else the
//! default profile ([`apollia_runtime::profile::profile_for_agent`]). It
//! carries five fields (`name`, `role`, `function`, `tools`, `preferences`)
//! and a free text (`free_text`); a key outside these reads and writes a
//! free-form entry, shared by every profile.
//!
//! Reads are open only to an agent whose profile source is checked: the
//! package default of its `sources`, replaced by the user's override
//! ([`apollia_runtime::profile::profile_source_checked`]). For any other agent
//! the context carries no interface and `ctx.profile` is `None`. Each read
//! emits a [`RuntimeEvent::ProfileRead`] naming the agent, the profile served
//! and the time, persisted in the trace of the task it belongs to. Writes are
//! further gated by the manifest's `user_memory_write` field, matching the
//! rule that already governs
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

use apollia_core::events::{EventBusSender, RuntimeEvent};
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
    /// Where each read is reported.
    trace: ReadTrace,
    /// The conversation the execution answers, when there is one: it may
    /// open or close the reading for itself.
    conversation: Option<String>,
}

/// Where a read of the profile is reported: the bus of the context and the
/// task the execution runs, both bound by [`crate::context::RuntimeContext`].
#[derive(Clone, Default)]
struct ReadTrace {
    bus: Option<EventBusSender>,
    task_id: Option<String>,
}

impl ReadTrace {
    /// Reports that `agent` read `profile_id`, on the tracer and on the bus.
    fn report(&self, agent: &str, profile_id: &str) {
        let read_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        tracing::event!(
            tracing::Level::INFO,
            agent = %agent,
            profile_id = %profile_id,
            task_id = ?self.task_id,
            "profile.read"
        );
        if let Some(bus) = &self.bus {
            // fire-and-forget: a bus without subscribers never blocks a read.
            let _ = bus.send(RuntimeEvent::ProfileRead {
                task_id: self.task_id.clone().map(Into::into),
                agent_id: agent.to_owned().into(),
                profile_id: profile_id.to_owned(),
                read_at,
            });
        }
    }
}

#[pymethods]
impl ProfileInterface {
    /// Returns the value of a profile field by key, or `None` when absent.
    fn get<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let db_path = self.db_path.clone();
        let agent = self.agent_name.clone();
        let trace = self.trace.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = tokio::task::spawn_blocking(move || {
                let read = profile_get(&db_path, &agent, &key)?;
                trace.report(&agent, &read.profile_id);
                Ok::<_, String>(read.value)
            })
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
        let trace = self.trace.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let present = tokio::task::spawn_blocking(move || {
                let read = profile_get(&db_path, &agent, &key)?;
                trace.report(&agent, &read.profile_id);
                Ok::<_, String>(read.value.is_some())
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
            .map_err(PyRuntimeError::new_err)?;
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
        let trace = self.trace.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let entries = tokio::task::spawn_blocking(move || {
                let read = profile_list_all(&db_path, &agent)?;
                trace.report(&agent, &read.profile_id);
                Ok::<_, String>(read.value)
            })
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
            trace: ReadTrace::default(),
            conversation: None,
        }
    }

    // TEST-ONLY: the chat runner of the Python agent path named the
    // conversation; the context of a package tool is built without one.
    /// Names the conversation the execution answers. Its settings may open
    /// the reading of the profile for it, or close it, whatever the profile
    /// source of the agent says.
    #[must_use]
    pub fn in_conversation(mut self, session_id: impl Into<String>) -> Self {
        self.conversation = Some(session_id.into());
        self
    }

    /// Whether the agent may read the profile: its profile source is checked
    /// (package default, then the user's override), read from the `agents.db`
    /// beside `user_memory.db`. In a conversation, what the conversation
    /// chose for itself comes first.
    pub fn source_checked(&self) -> bool {
        let data_dir = data_dir_of(&self.db_path);
        match &self.conversation {
            Some(session_id) => apollia_runtime::profile::profile_source_checked_in_conversation(
                data_dir,
                &self.agent_name,
                session_id,
            ),
            None => apollia_runtime::profile::profile_source_checked(data_dir, &self.agent_name),
        }
    }

    /// Reports each read on `bus`.
    pub(crate) fn report_reads_on(&mut self, bus: Option<EventBusSender>) {
        self.trace.bus = bus;
    }

    /// Ties each read to `task_id`, so it shows in that task's trace.
    pub(crate) fn report_reads_for_task(&mut self, task_id: String) {
        self.trace.task_id = Some(task_id);
    }
}

/// What a read returned, and from which profile.
struct Read<T> {
    profile_id: String,
    value: T,
}

/// The data directory `user_memory.db` sits in, where `agents.db` sits too.
fn data_dir_of(db_path: &std::path::Path) -> &std::path::Path {
    db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
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
    apollia_runtime::profile::profile_for_agent(repo, data_dir_of(db_path), agent)
        .map_err(|e| format!("resolve profile: {e}"))
}

fn profile_get(
    db_path: &std::path::Path,
    agent: &str,
    key: &str,
) -> Result<Read<Option<String>>, String> {
    let flat_key = key.strip_prefix("user.").unwrap_or(key);
    let repo = open_repo(db_path)?;
    let profile = resolved_profile(&repo, db_path, agent)?;
    let entry = repo
        .get_in(&profile, flat_key)
        .map_err(|e| format!("get: {e}"))?;
    Ok(Read {
        profile_id: profile.id,
        value: entry.map(|e| e.value),
    })
}

fn profile_list_all(
    db_path: &std::path::Path,
    agent: &str,
) -> Result<Read<Vec<ProfileEntry>>, String> {
    let repo = open_repo(db_path)?;
    let profile = resolved_profile(&repo, db_path, agent)?;
    let entries = repo
        .list_all_in(&profile)
        .map_err(|e| format!("list_all: {e}"))?;
    Ok(Read {
        profile_id: profile.id,
        value: entries,
    })
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
        let for_cours = profile_get(&db, "cours", "role").expect("read").value;
        let for_other = profile_get(&db, "veille", "user.role").expect("read").value;
        profile_set(
            &db,
            "cours",
            "tools",
            "Moodle",
            WrittenBy::Agent("cours".into()),
        )
        .expect("write");
        let all_cours = profile_list_all(&db, "cours").expect("list").value;
        let all_other = profile_list_all(&db, "veille").expect("list").value;

        // THEN each reads its own profile, and the write landed in the designated one only
        assert_eq!(for_cours.as_deref(), Some("Formateur"));
        assert_eq!(for_other.as_deref(), Some("Responsable SI"));
        assert!(all_cours
            .iter()
            .any(|e| e.key == "tools" && e.value == "Moodle"));
        assert!(!all_other.iter().any(|e| e.key == "tools"));
    }

    #[test]
    fn ctx_profile_is_none_unless_the_profile_source_is_checked() {
        // GIVEN an agent whose package declares the profile source and one declaring none
        pyo3::prepare_freethreaded_python();
        let dir = tempfile::tempdir().expect("tempdir");
        let db = apollia_core::paths::DataFile::UserMemory.path(dir.path());
        designate(dir.path(), "veille", "");
        let agents = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
            .expect("agents.db opens");
        agents
            .set_override("veille", OverrideField::Sources, &serde_json::json!([]))
            .expect("sources unchecked");
        agents
            .save_definition(&StoredAgent {
                name: "cours".to_owned(),
                origin: AgentOrigin::Package,
                package: Some("formation".to_owned()),
                version: "1".to_owned(),
                definition: serde_json::json!({
                    "id": "cours",
                    "defaults": { "sources": ["profile"] },
                }),
                enabled: true,
                installed_at: "2026-10-03T00:00:00Z".to_owned(),
                updated_at: "2026-10-03T00:00:00Z".to_owned(),
            })
            .expect("agent stored");

        // WHEN each gets a context with a profile interface
        let seen = |agent: &str| {
            let profile = ProfileInterface::new(db.clone(), agent.to_owned(), false, false);
            let ctx = crate::context::RuntimeContext::for_package_tool(agent).with_profile(profile);
            Python::with_gil(|py| {
                let ctx = Py::new(py, ctx).expect("ctx");
                ctx.bind(py)
                    .getattr("profile")
                    .expect("ctx.profile")
                    .is_none()
            })
        };

        // THEN only the agent whose source is checked sees a profile
        assert!(!seen("cours"), "checked: ctx.profile is served");
        assert!(seen("veille"), "unchecked: ctx.profile is None");
        assert!(seen("ghost"), "unknown agent: ctx.profile is None");
    }

    #[test]
    fn a_conversation_opens_or_closes_the_profile_for_itself_whatever_the_source_says() {
        // GIVEN an agent whose profile source is checked and one whose source
        // is cleared, and three conversations: one that says nothing, one
        // that opens the profile, one that closes it
        pyo3::prepare_freethreaded_python();
        let dir = tempfile::tempdir().expect("tempdir");
        let db = apollia_core::paths::DataFile::UserMemory.path(dir.path());
        designate(dir.path(), "veille", "");
        let agents = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
            .expect("agents.db opens");
        agents
            .set_override("veille", OverrideField::Sources, &serde_json::json!([]))
            .expect("sources unchecked");
        designate(dir.path(), "cours", "");
        agents
            .set_override(
                "cours",
                OverrideField::Sources,
                &serde_json::json!(["profile"]),
            )
            .expect("sources checked");
        let chat = apollia_runtime::chat::ChatSessionRepository::open(
            &apollia_core::paths::DataFile::Chat.path(dir.path()),
        )
        .expect("chat.db opens");
        for (session, choice) in [
            ("silent", None),
            ("opens", Some(true)),
            ("closes", Some(false)),
        ] {
            chat.create_session(
                session,
                &apollia_runtime::chat::ChatMode::Agent,
                Some("cours"),
                "",
                &[],
                "2026-10-06T00:00:00Z",
                None,
            )
            .expect("session");
            chat.set_session_settings(
                session,
                &apollia_runtime::chat::SessionSettings {
                    profile_access: choice,
                    ..Default::default()
                },
            )
            .expect("settings");
        }

        // WHEN each agent gets a context in each conversation
        let served = |agent: &str, session: &str| {
            let profile = ProfileInterface::new(db.clone(), agent.to_owned(), false, false)
                .in_conversation(session);
            let ctx = crate::context::RuntimeContext::for_package_tool(agent).with_profile(profile);
            Python::with_gil(|py| {
                let ctx = Py::new(py, ctx).expect("ctx");
                !ctx.bind(py)
                    .getattr("profile")
                    .expect("ctx.profile")
                    .is_none()
            })
        };

        // THEN a silent conversation follows the source of the agent, and a
        // conversation that chose decides for both agents
        assert!(served("cours", "silent"));
        assert!(!served("veille", "silent"));
        assert!(served("veille", "opens"));
        assert!(!served("cours", "closes"));
        assert!(!served("veille", "unknown-conversation"));
    }

    #[tokio::test]
    async fn each_read_reports_the_agent_the_profile_and_the_time_on_the_bus() {
        // GIVEN a read trace bound to a bus and a task
        let (bus, mut rx) = tokio::sync::broadcast::channel(8);
        let trace = ReadTrace {
            bus: Some(bus),
            task_id: Some("task-cours".to_owned()),
        };

        // WHEN the agent reads a profile
        trace.report("cours", "profile-formateur");

        // THEN one ProfileRead names the task, the agent, the profile and an RFC 3339 time
        match rx.recv().await.expect("event") {
            RuntimeEvent::ProfileRead {
                task_id,
                agent_id,
                profile_id,
                read_at,
            } => {
                assert_eq!(
                    task_id.map(|t| t.to_string()).as_deref(),
                    Some("task-cours")
                );
                assert_eq!(agent_id.to_string(), "cours");
                assert_eq!(profile_id, "profile-formateur");
                assert!(
                    chrono::DateTime::parse_from_rfc3339(&read_at).is_ok(),
                    "{read_at}"
                );
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
}
