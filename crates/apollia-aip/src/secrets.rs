//! ctx.secrets: read-only credentials access for agents.
//!
//! Wraps an existing [`ToolCredentialStore`](apollia_tools::ToolCredentialStore)
//! (AES-256-GCM) with **manifest gating**: an agent only sees the keys
//! declared in `@agent(secrets=...)`. Any undeclared key is invisible
//! (returns `None`), even if it exists in the database.
//!
//! The store is not exposed for writing to agents; ops manage secrets via the
//! CLI / desktop. This interface is read-only by design.
//!
//! The interface also works in degraded mode (`store = None`) with functional
//! gating and all values at `None`. The tool namespace used for agents is
//! `"agent"` by convention.

use std::sync::{Arc, Mutex};

use apollia_tools::ToolCredentialStore;
use pyo3::prelude::*;

/// `tool_name` namespace used in `ToolCredentialStore` for credentials that
/// belong to an agent (as opposed to secrets owned by a native tool such as
/// `web_search`).
const AGENT_SECRETS_NAMESPACE: &str = "agent";

/// Read-only interface exposed to the agent via `ctx.secrets`.
///
/// Built with:
/// - `store`: optional; `None` for tests / runtimes without a store.
/// - `declared`: copy of `manifest.secrets`, acts as a strict allowlist.
#[pyclass(name = "SecretsInterface", module = "apollia._native")]
pub struct SecretsInterface {
    /// Shared store (read-only from the agent). `None` disables effective
    /// resolution but keeps the gating semantics.
    ///
    /// Wrapped in a `Mutex` because [`ToolCredentialStore`] holds a
    /// [`rusqlite::Connection`] that is not `Sync` (due to the
    /// `RefCell<StatementCache>`). The `Mutex` introduces no contention in
    /// practice: agents are single-threaded on the Python side and the store
    /// is read rarely (at most a few times per task).
    store: Option<Arc<Mutex<ToolCredentialStore>>>,
    /// List of keys allowed by the manifest. Linear lookup since this list
    /// stays small (<= 10 typically).
    declared: Vec<String>,
    /// `tool_name` namespace to use when looking up this agent's credentials
    /// in the store. Defaults to [`AGENT_SECRETS_NAMESPACE`]; overridable via
    /// [`Self::with_namespace`] for tests / future per-agent scenarios.
    namespace: String,
}

#[pymethods]
impl SecretsInterface {
    /// Returns the value of secret `key`, or `None` if unknown / not
    /// configured.
    ///
    /// **No exception**: a missing secret is legitimate (an agent that
    /// degrades gracefully). To fail fast, the agent checks presence itself
    /// and raises the appropriate business error.
    ///
    /// **Gating**: if `key` is not in the manifest's declared list, the return
    /// is `None` even if the value exists in the database. This behavior is
    /// silent: the manifest declaration is the **only** source of authority.
    fn get(&self, key: &str) -> PyResult<Option<String>> {
        if !self.declared.iter().any(|d| d == key) {
            return Ok(None);
        }
        match &self.store {
            Some(store) => {
                let guard = match store.lock() {
                    Ok(g) => g,
                    Err(p) => {
                        tracing::error!(
                            target: "apollia.aip.secrets",
                            key = %key,
                            error = %p,
                            "aip.secret.store.poisoned"
                        );
                        return Ok(None);
                    }
                };
                match guard.get(&self.namespace, key) {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        tracing::warn!(
                            target: "apollia.aip.secrets",
                            key = %key,
                            error = %e,
                            "aip.secret.read.failed"
                        );
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// `True` if the key is declared AND configured in the database.
    ///
    /// Strictly equivalent to `ctx.secrets.get(key) is not None` on the agent
    /// side. Idiomatic form for early branching.
    fn has(&self, key: &str) -> PyResult<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Lists the keys declared in the manifest (never the values).
    ///
    /// Lets an agent introspect its own configuration at startup without
    /// hardcoding names. No leak: only the names the agent declared itself
    /// are revealed.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }
}

impl SecretsInterface {
    /// Builds the secrets interface with the shared store and the declared
    /// list. Uses the default namespace ([`AGENT_SECRETS_NAMESPACE`]).
    pub fn new(store: Option<Arc<Mutex<ToolCredentialStore>>>, declared: Vec<String>) -> Self {
        Self {
            store,
            declared,
            namespace: AGENT_SECRETS_NAMESPACE.to_string(),
        }
    }

    /// Overrides the `tool_name` namespace used for resolution. Allows
    /// isolating secrets per agent (e.g. `"agent::veille-ia"`).
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_without_store_returns_none() {
        // GIVEN a secrets interface declaring a key but backed by no store
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        // WHEN the declared key is read
        let v = s.get("API_KEY").expect("get should not raise");
        // THEN the read succeeds and yields nothing
        assert!(v.is_none(), "no store -> None");
    }

    #[test]
    fn test_undeclared_key_returns_none() {
        // GIVEN a secrets interface declaring API_KEY and nothing else
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        // WHEN a key the agent never declared is read
        let v = s.get("OTHER_KEY").expect("get");
        // THEN it yields nothing, whatever the database holds
        assert!(v.is_none(), "undeclared keys are invisible");
    }

    #[test]
    fn test_has_mirrors_get() {
        // GIVEN a secrets interface declaring API_KEY and backed by no store
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        // WHEN presence is asked for the declared key and for an undeclared one
        // THEN both answer false, exactly as get() does
        assert!(!s.has("API_KEY").expect("has"));
        assert!(!s.has("UNKNOWN").expect("has"));
    }

    #[test]
    fn test_list_names_returns_declared() {
        // GIVEN a secrets interface declaring two keys
        let s = SecretsInterface::new(None, vec!["A".to_string(), "B".to_string()]);
        // WHEN the declared names are listed
        // THEN both come back, in declaration order
        assert_eq!(s.list_names(), vec!["A", "B"]);
    }

    // Tests with a real ToolCredentialStore

    /// Builds an ephemeral `ToolCredentialStore` and inserts a secret for the
    /// `"agent"` namespace.
    fn store_with_agent_secret(
        dir: &tempfile::TempDir,
        key: &str,
        value: &str,
    ) -> Arc<Mutex<ToolCredentialStore>> {
        let _ = apollia_tools::GovernanceDb::open(dir.path()).expect("init governance.db");
        let db = dir.path().join(apollia_tools::GOVERNANCE_DB_FILENAME);
        let kf = dir.path().join(".keyfile");
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open store");
        store
            .set(AGENT_SECRETS_NAMESPACE, key, value)
            .expect("set secret");
        Arc::new(Mutex::new(store))
    }

    /// With a real store and a declared key, `get()` returns the cleartext
    /// value.
    #[test]
    fn test_get_with_store_returns_value_when_declared() {
        // GIVEN
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store_with_agent_secret(&tmp, "brave_api_key", "BSA-secret-001");
        let iface = SecretsInterface::new(Some(store), vec!["brave_api_key".to_string()]);
        // WHEN / THEN
        assert_eq!(
            iface.get("brave_api_key").expect("get"),
            Some("BSA-secret-001".to_string())
        );
        assert!(iface.has("brave_api_key").expect("has"));
    }

    /// Manifest gating: a key that exists in the database but is undeclared
    /// returns `None`.
    #[test]
    fn test_get_with_store_gated_by_manifest() {
        // GIVEN the store contains `brave_api_key` but the agent only declared
        // `openai_api_key`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store_with_agent_secret(&tmp, "brave_api_key", "BSA-secret-001");
        let iface = SecretsInterface::new(Some(store), vec!["openai_api_key".to_string()]);
        // WHEN / THEN: the stored secret is invisible because it is undeclared.
        assert_eq!(iface.get("brave_api_key").expect("get"), None);
        assert!(!iface.has("brave_api_key").expect("has"));
        // A declared key absent from the store also returns None.
        assert_eq!(iface.get("openai_api_key").expect("get"), None);
    }

    /// `has()` returns false for a declared key that is absent from the store.
    #[test]
    fn test_has_false_when_declared_but_missing_in_store() {
        // GIVEN an empty credential store and an interface declaring api_key
        let tmp = tempfile::tempdir().expect("tempdir");
        let _ = apollia_tools::GovernanceDb::open(tmp.path()).expect("init");
        let db = tmp.path().join(apollia_tools::GOVERNANCE_DB_FILENAME);
        let kf = tmp.path().join(".keyfile");
        let store = ToolCredentialStore::new(&db, &kf).expect("open");
        let iface = SecretsInterface::new(
            Some(Arc::new(Mutex::new(store))),
            vec!["api_key".to_string()],
        );
        // WHEN presence is asked for the declared key
        // THEN it answers false: declaring a key does not create it
        assert!(!iface.has("api_key").expect("has"));
    }

    /// `with_namespace` allows isolating secrets per agent.
    /// A store that holds a secret under `agent::veille-ia` is visible only to
    /// an interface configured with that namespace, not to the default
    /// `"agent"` namespace.
    #[test]
    fn test_with_namespace_isolates_secrets() {
        // GIVEN a store holding brave_api_key under the `agent::veille-ia` namespace
        let tmp = tempfile::tempdir().expect("tempdir");
        let _ = apollia_tools::GovernanceDb::open(tmp.path()).expect("init");
        let db = tmp.path().join(apollia_tools::GOVERNANCE_DB_FILENAME);
        let kf = tmp.path().join(".keyfile");
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open");
        store
            .set("agent::veille-ia", "brave_api_key", "scoped-value")
            .expect("set scoped");
        let shared = Arc::new(Mutex::new(store));

        // WHEN the key is read from the default namespace, then from the scoped one
        // Default "agent" namespace: sees nothing.
        let default_iface =
            SecretsInterface::new(Some(shared.clone()), vec!["brave_api_key".to_string()]);
        assert_eq!(default_iface.get("brave_api_key").expect("get"), None);

        // THEN only the scoped interface sees the value
        // Scoped namespace: sees the value.
        let scoped_iface = SecretsInterface::new(Some(shared), vec!["brave_api_key".to_string()])
            .with_namespace("agent::veille-ia");
        assert_eq!(
            scoped_iface.get("brave_api_key").expect("get"),
            Some("scoped-value".to_string())
        );
    }
}
