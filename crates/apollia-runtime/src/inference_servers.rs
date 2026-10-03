//! The declared inference servers, from `system.db` to the live router.
//!
//! `system.db` holds a server's address and whether it has a key; the key
//! itself sits in the secret store under
//! [`INFERENCE_SERVER_SECRET_SERVICE`]. This module joins the two: it moves the
//! keys a schema migration could not reach into the store, and resolves every
//! enabled server into the [`ServerEndpoint`] the router routes
//! `<server>/<model>` through.

use apollia_auth::SecretStore;
use apollia_core::inference_server::INFERENCE_SERVER_SECRET_SERVICE;
use apollia_core::{InferenceServer, LlmBackendRepository};
use apollia_llm::ServerEndpoint;

/// Why a server's key could not be resolved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerKeyError {
    /// The row says a key is stored, and the store has none.
    #[error("the key of server '{0}' is missing from the secret store")]
    Missing(String),
    /// The environment variable the server names is not set.
    #[error("the environment variable {var} holding the key of server '{server}' is not set")]
    EnvUnset {
        /// Server name.
        server: String,
        /// Variable name.
        var: String,
    },
    /// The secret store could not be opened or read.
    #[error("secret store: {0}")]
    Store(String),
}

/// The key of `server`: from the secret store when the row says one is
/// stored, from its environment variable when it names one, `None` otherwise.
///
/// `store` is only called when the row says a key is stored, so a server
/// without one never touches the OS keychain.
///
/// # Errors
///
/// See [`ServerKeyError`].
pub fn resolve_server_key<F>(
    server: &InferenceServer,
    store: F,
) -> Result<Option<String>, ServerKeyError>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    if server.has_key {
        let store = store().map_err(ServerKeyError::Store)?;
        return store
            .get(INFERENCE_SERVER_SECRET_SERVICE, &server.name)
            .map_err(|e| ServerKeyError::Store(e.to_string()))?
            .map(Some)
            .ok_or_else(|| ServerKeyError::Missing(server.name.clone()));
    }
    match &server.api_key_env {
        Some(var) => std::env::var(var)
            .map(Some)
            .map_err(|_| ServerKeyError::EnvUnset {
                server: server.name.clone(),
                var: var.clone(),
            }),
        None => Ok(None),
    }
}

/// The secret store this process is configured with, as a resolver argument.
pub fn default_store() -> Result<Box<dyn SecretStore>, String> {
    apollia_auth::select_secret_store().map_err(|e| e.to_string())
}

/// Opens the secret store a route writes keys to. Held by the API state so a
/// test hands the routes a throwaway store instead of the operator's keychain.
pub type SecretStoreOpener =
    std::sync::Arc<dyn Fn() -> Result<Box<dyn SecretStore>, String> + Send + Sync>;

/// The opener over [`default_store`], the one the runtime serves with.
pub fn default_store_opener() -> SecretStoreOpener {
    std::sync::Arc::new(default_store)
}

/// Move into the secret store the keys a schema migration left in clear on a
/// backend row, then record each as stored. Returns how many were moved.
///
/// The store is opened only when there is something to move.
pub fn adopt_pending_keys<F>(repo: &LlmBackendRepository, store: F) -> usize
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let pending = match repo.pending_server_keys() {
        Ok(pending) if !pending.is_empty() => pending,
        Ok(_) => return 0,
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "llm.servers.keys.read_failed");
            return 0;
        }
    };
    let store = match store() {
        Ok(store) => store,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                pending = pending.len(),
                "llm.servers.keys.store_unavailable"
            );
            return 0;
        }
    };
    let mut moved = 0;
    for (server, key) in pending {
        let stored = store
            .set(INFERENCE_SERVER_SECRET_SERVICE, &server, &key)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                repo.mark_server_key_stored(&server)
                    .map_err(|e| e.to_string())
            });
        match stored {
            Ok(()) => moved += 1,
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                server = %server,
                error = %e,
                "llm.servers.key.adoption_failed"
            ),
        }
    }
    tracing::event!(
        tracing::Level::INFO,
        moved = moved,
        "llm.servers.keys.adopted"
    );
    moved
}

/// Every enabled server, with its key resolved. A server whose key cannot be
/// resolved is left out and logged: its models are then not routable, and the
/// other servers still are.
///
/// A server the organisation policy refuses (`governance`) stays in the list
/// with the reason of the refusal and no key, so a request to one of its
/// models fails with that reason before anything is sent to it.
///
/// The store is opened at most once, and only for a server that has a key in it.
pub fn server_endpoints<F>(
    repo: &LlmBackendRepository,
    store: F,
    governance: &crate::governance::InferenceGovernance,
) -> Vec<ServerEndpoint>
where
    F: Fn() -> Result<Box<dyn SecretStore>, String>,
{
    let servers = match repo.list_servers() {
        Ok(servers) => servers,
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "llm.servers.read_failed");
            return Vec::new();
        }
    };
    let opened: std::cell::OnceCell<Result<std::sync::Arc<dyn SecretStore>, String>> =
        std::cell::OnceCell::new();
    let shared = || -> Result<Box<dyn SecretStore>, String> {
        let store = opened
            .get_or_init(|| store().map(std::sync::Arc::from))
            .clone()?;
        Ok(Box::new(SharedStore(store)))
    };
    servers
        .into_iter()
        .filter(|s| s.enabled)
        .filter_map(|server| {
            if let Err(reason) = governance.server_decision(&server.base_url) {
                tracing::event!(
                    tracing::Level::WARN,
                    server = %server.name,
                    reason = %reason,
                    "llm.server.refused_by_policy"
                );
                return Some(ServerEndpoint {
                    name: server.name,
                    base_url: server.base_url,
                    api_key: None,
                    refusal: Some(reason),
                });
            }
            resolved_endpoint(server, shared)
        })
        .collect()
}

/// `server` with its key resolved, or `None`, logged, when the key cannot be.
fn resolved_endpoint<F>(server: InferenceServer, store: F) -> Option<ServerEndpoint>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    match resolve_server_key(&server, store) {
        Ok(api_key) => Some(ServerEndpoint {
            name: server.name,
            base_url: server.base_url,
            api_key,
            refusal: None,
        }),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                server = %server.name,
                error = %e,
                "llm.server.skipped"
            );
            None
        }
    }
}

/// A shared store handed out as a `Box<dyn SecretStore>`.
pub(crate) struct SharedStore(pub(crate) std::sync::Arc<dyn SecretStore>);

impl SecretStore for SharedStore {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), apollia_auth::AuthError> {
        self.0.set(service, user, value)
    }
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, apollia_auth::AuthError> {
        self.0.get(service, user)
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), apollia_auth::AuthError> {
        self.0.delete(service, user)
    }
    fn backend_id(&self) -> &'static str {
        self.0.backend_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_auth::AgeFileSecretStore;
    use apollia_core::paths::DataFile;

    fn file_store(dir: &tempfile::TempDir) -> Box<dyn SecretStore> {
        Box::new(
            AgeFileSecretStore::new(dir.path().join("secrets"), "test-passphrase".into()).unwrap(),
        )
    }

    fn no_store() -> Result<Box<dyn SecretStore>, String> {
        Err("the store must not be opened".into())
    }

    // GIVEN a v2 `system.db` whose self-hosted `openai` row holds its key in clear
    // WHEN it is opened at the current version and the pending keys are adopted
    // THEN the key sits in the secret store under the server's name, the server
    //      says it has a key, and its endpoint resolves to that key
    #[test]
    fn test_a_clear_key_moves_into_the_secret_store() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(DataFile::System.file_name());
        {
            let repo = LlmBackendRepository::open(&path).unwrap();
            repo.save(&apollia_core::LlmBackendConfig {
                name: "gateway".into(),
                provider: apollia_core::LlmProvider::OpenAi,
                model: "m".into(),
                config_json: serde_json::json!({}),
                enabled: true,
                is_default: true,
            })
            .unwrap();
        }
        // Rewind to v2 with a clear key, the shape the migration meets.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "DROP TABLE inference_servers;
                 UPDATE llm_backends SET config_json = '{\"base_url\":\"http://10.1.1.1:8000/v1\",\"api_key\":\"sk-live\"}';
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        }
        let repo = LlmBackendRepository::open(&path).unwrap();
        let store_dir = tempfile::TempDir::new().unwrap();

        // WHEN
        let moved = adopt_pending_keys(&repo, || Ok(file_store(&store_dir)));

        // THEN
        assert_eq!(moved, 1);
        assert_eq!(
            file_store(&store_dir)
                .get(INFERENCE_SERVER_SECRET_SERVICE, "gateway")
                .unwrap()
                .as_deref(),
            Some("sk-live")
        );
        assert!(repo.find_server("gateway").unwrap().unwrap().has_key);
        let endpoints = server_endpoints(
            &repo,
            || Ok(file_store(&store_dir)),
            &crate::governance::InferenceGovernance::default(),
        );
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].api_key.as_deref(), Some("sk-live"));
        assert_eq!(
            adopt_pending_keys(&repo, no_store),
            0,
            "nothing left to move"
        );
    }

    // GIVEN servers with no key, and a disabled one
    // WHEN their endpoints are resolved
    // THEN the enabled one comes back without a key, and the store is never opened
    #[test]
    fn test_a_server_without_a_key_never_opens_the_store() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        for (name, enabled) in [("open", true), ("off", false)] {
            repo.save_server(&InferenceServer {
                name: name.into(),
                base_url: "http://127.0.0.1:11434".into(),
                has_key: false,
                api_key_env: None,
                enabled,
            })
            .unwrap();
        }

        // WHEN
        let endpoints = server_endpoints(
            &repo,
            no_store,
            &crate::governance::InferenceGovernance::default(),
        );

        // THEN
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].name, "open");
        assert!(endpoints[0].api_key.is_none());
    }

    // GIVEN a server that says its key is stored, and a store without it
    // WHEN its key is resolved
    // THEN the error names the missing key
    #[test]
    fn test_a_stored_key_absent_from_the_store_is_reported() {
        // GIVEN
        let store_dir = tempfile::TempDir::new().unwrap();
        let server = InferenceServer {
            name: "corp".into(),
            base_url: "http://h".into(),
            has_key: true,
            api_key_env: None,
            enabled: true,
        };

        // WHEN
        let err = resolve_server_key(&server, || Ok(file_store(&store_dir))).unwrap_err();

        // THEN
        assert!(matches!(err, ServerKeyError::Missing(name) if name == "corp"));
    }
}
