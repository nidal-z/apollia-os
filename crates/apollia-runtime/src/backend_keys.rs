//! The key of a backend row, kept in the secret store rather than in `system.db`.
//!
//! A backend reached over HTTP may carry a key. Written as `${VAR}`, it names
//! an environment variable and stays in `config_json`. Written as the key
//! itself, it is moved into the secret store under [`BACKEND_SECRET_SERVICE`]
//! (account = backend name) before the row is saved, and `config_json` only
//! records [`KEY_IN_STORE_FIELD`]. The router reads `config_json["api_key"]`,
//! so every path that builds one puts the key back in memory with
//! [`unseal_backend_keys`] first; nothing writes it back.

use apollia_auth::SecretStore;
use apollia_core::{LlmBackendConfig, LlmBackendRepository};

use crate::inference_servers::SharedStore;

/// Secret store service under which a backend key is kept, account = backend name.
pub const BACKEND_SECRET_SERVICE: &str = "apollia-llm-backend";

/// `config_json` field set to `true` when the backend's key sits in the store.
pub const KEY_IN_STORE_FIELD: &str = "api_key_in_store";

/// How a `config_json` carries its key.
enum KeyForm {
    /// No `api_key`, or an empty one.
    Absent,
    /// `${VAR}`: an environment variable name, not a secret.
    Env,
    /// The key itself.
    Literal(String),
}

fn key_form(config: &serde_json::Value) -> KeyForm {
    match config
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::trim)
    {
        None | Some("") => KeyForm::Absent,
        Some(raw) if raw.starts_with("${") && raw.ends_with('}') && raw.len() > 3 => KeyForm::Env,
        Some(raw) => KeyForm::Literal(raw.to_string()),
    }
}

/// Whether `config` says its key sits in the secret store.
pub fn key_in_store(config: &serde_json::Value) -> bool {
    config
        .get(KEY_IN_STORE_FIELD)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Make `cfg` fit to be written to `system.db`: a key given in clear goes to
/// the store `store` opens and leaves `config_json`. `had_stored_key` says
/// whether the row being replaced already held a key in the store; that key is
/// removed when the new configuration no longer refers to it.
///
/// The store is opened only when a key has to be written or removed.
///
/// # Errors
///
/// The store could not be opened or written. Nothing is then to be saved: a
/// key the store refused must not fall back to `system.db`.
pub fn seal_backend_key<F>(
    cfg: &mut LlmBackendConfig,
    had_stored_key: bool,
    store: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let Some(config) = cfg.config_json.as_object_mut() else {
        return Ok(());
    };
    let value = serde_json::Value::Object(config.clone());
    match key_form(&value) {
        KeyForm::Literal(key) => {
            store()?
                .set(BACKEND_SECRET_SERVICE, &cfg.name, &key)
                .map_err(|e| e.to_string())?;
            config.remove("api_key");
            config.insert(KEY_IN_STORE_FIELD.into(), serde_json::Value::Bool(true));
        }
        form => {
            if matches!(form, KeyForm::Env) {
                config.remove(KEY_IN_STORE_FIELD);
            }
            let keeps_stored = key_in_store(&serde_json::Value::Object(config.clone()));
            if had_stored_key && !keeps_stored {
                store()?
                    .delete(BACKEND_SECRET_SERVICE, &cfg.name)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Remove the stored key of a backend that is being deleted.
///
/// # Errors
///
/// The store could not be opened or written.
pub fn forget_backend_key<F>(name: &str, store: F) -> Result<(), String>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    store()?
        .delete(BACKEND_SECRET_SERVICE, name)
        .map_err(|e| e.to_string())
}

/// Put back, in memory, the key of every row that keeps it in the store, so
/// the router finds it under `config_json["api_key"]`. A key the store cannot
/// give is logged and left out: the backend is then called without one.
///
/// The store is opened at most once, and only when a row needs it.
pub fn unseal_backend_keys<F>(configs: &mut [LlmBackendConfig], store: F)
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let mut store = Some(store);
    let mut opened: Option<Result<Box<dyn SecretStore>, String>> = None;
    for cfg in configs.iter_mut() {
        if !key_in_store(&cfg.config_json) || !matches!(key_form(&cfg.config_json), KeyForm::Absent)
        {
            continue;
        }
        if opened.is_none() {
            opened = store.take().map(|open| open());
        }
        let key = match opened.as_ref() {
            Some(Ok(store)) => store
                .get(BACKEND_SECRET_SERVICE, &cfg.name)
                .map_err(|e| e.to_string())
                .and_then(|key| key.ok_or_else(|| "missing from the secret store".to_string())),
            Some(Err(e)) => Err(e.clone()),
            None => Err("secret store unavailable".to_string()),
        };
        match (key, cfg.config_json.as_object_mut()) {
            (Ok(key), Some(config)) => {
                config.insert("api_key".into(), serde_json::Value::String(key));
            }
            (Ok(_), None) => {}
            (Err(e), _) => tracing::event!(
                tracing::Level::WARN,
                backend = %cfg.name,
                error = %e,
                "llm.backend.key.unresolved"
            ),
        }
    }
}

/// Every backend row with its stored key put back in memory, and the name of
/// the default one, ready for the router.
///
/// # Errors
///
/// The repository could not be read.
pub fn routable_configs<F>(
    repo: &LlmBackendRepository,
    store: F,
) -> Result<(Vec<LlmBackendConfig>, Option<String>), apollia_core::LlmBackendError>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let mut configs = repo.list()?;
    let default = repo.find_default()?.map(|cfg| cfg.name);
    unseal_backend_keys(&mut configs, store);
    Ok((configs, default))
}

/// Move into the secret store every key a backend row still holds in clear,
/// and rewrite the row without it. Returns how many rows were sealed.
///
/// Runs at startup, after the declared servers took their copy of the keys
/// they inherited (see `inference_servers::adopt_pending_keys`). The store is
/// opened only when a row holds a key in clear.
pub fn seal_stored_backend_keys<F>(repo: &LlmBackendRepository, store: F) -> usize
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let rows = match repo.list() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "llm.backend.keys.read_failed");
            return 0;
        }
    };
    let clear: Vec<LlmBackendConfig> = rows
        .into_iter()
        .filter(|cfg| matches!(key_form(&cfg.config_json), KeyForm::Literal(_)))
        .collect();
    if clear.is_empty() {
        return 0;
    }
    let opened = match store() {
        Ok(store) => std::sync::Arc::<dyn SecretStore>::from(store),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                pending = clear.len(),
                "llm.backend.keys.store_unavailable"
            );
            return 0;
        }
    };
    let mut sealed = 0;
    for mut cfg in clear {
        let shared = opened.clone();
        let outcome = seal_backend_key(&mut cfg, false, move || {
            Ok(Box::new(SharedStore(shared)) as Box<dyn SecretStore>)
        })
        .and_then(|()| repo.save(&cfg).map_err(|e| e.to_string()));
        match outcome {
            Ok(()) => sealed += 1,
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                backend = %cfg.name,
                error = %e,
                "llm.backend.key.seal_failed"
            ),
        }
    }
    tracing::event!(
        tracing::Level::INFO,
        sealed = sealed,
        "llm.backend.keys.sealed"
    );
    sealed
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_auth::AgeFileSecretStore;
    use apollia_core::inference_server::INFERENCE_SERVER_SECRET_SERVICE;
    use apollia_core::paths::DataFile;
    use apollia_core::LlmProvider;

    fn file_store(dir: &tempfile::TempDir) -> Box<dyn SecretStore> {
        Box::new(
            AgeFileSecretStore::new(dir.path().join("secrets"), "test-passphrase".into()).unwrap(),
        )
    }

    fn no_store() -> Result<Box<dyn SecretStore>, String> {
        Err("the store must not be opened".into())
    }

    fn gateway(config_json: serde_json::Value) -> LlmBackendConfig {
        LlmBackendConfig {
            name: "gateway".into(),
            provider: LlmProvider::OpenAi,
            model: "qwen3-32b".into(),
            config_json,
            enabled: true,
            is_default: true,
        }
    }

    // GIVEN a backend sent with its key in clear, the way
    //       `llm backends create --api-key` sends it
    // WHEN it is sealed and saved
    // THEN the key sits in the store, `config_json` only says so, the bytes of
    //      `system.db` do not hold it, and the router gets it back in memory
    #[test]
    fn test_a_backend_key_given_in_clear_goes_to_the_store_never_to_the_database() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let db = dir.path().join(DataFile::System.file_name());
        let repo = LlmBackendRepository::open(&db).unwrap();
        let mut cfg = gateway(serde_json::json!({
            "base_url": "http://10.0.0.5:8000/v1",
            "api_key": "sk-from-the-cli",
        }));

        // WHEN
        seal_backend_key(&mut cfg, false, || Ok(file_store(&dir))).unwrap();
        repo.save(&cfg).unwrap();

        // THEN
        let saved = repo.find_by_name("gateway").unwrap().unwrap();
        assert_eq!(
            saved.config_json,
            serde_json::json!({
                "base_url": "http://10.0.0.5:8000/v1",
                KEY_IN_STORE_FIELD: true,
            })
        );
        assert_eq!(
            file_store(&dir)
                .get(BACKEND_SECRET_SERVICE, "gateway")
                .unwrap()
                .as_deref(),
            Some("sk-from-the-cli")
        );
        let (configs, default) = routable_configs(&repo, || Ok(file_store(&dir))).unwrap();
        assert_eq!(default.as_deref(), Some("gateway"));
        assert_eq!(configs[0].config_json["api_key"], "sk-from-the-cli");
        drop(repo);
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = std::fs::read(&path).unwrap();
                assert!(
                    !String::from_utf8_lossy(&bytes).contains("sk-from-the-cli"),
                    "the key must not be in {}",
                    path.display()
                );
            }
        }
    }

    // GIVEN a v2 `system.db` whose self-hosted `openai` row holds its key in clear
    // WHEN it is opened, the server adopts its key, and the backend keys are sealed
    // THEN the server and the backend both find the key in the store, and the
    //      row's `config_json` no longer holds it
    #[test]
    fn test_a_clear_key_of_an_old_row_leaves_config_json_after_adoption() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(DataFile::System.file_name());
        {
            let repo = LlmBackendRepository::open(&path).unwrap();
            repo.save(&gateway(serde_json::json!({}))).unwrap();
        }
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
        let adopted =
            crate::inference_servers::adopt_pending_keys(&repo, || Ok(file_store(&store_dir)));
        let sealed = seal_stored_backend_keys(&repo, || Ok(file_store(&store_dir)));

        // THEN
        assert_eq!((adopted, sealed), (1, 1));
        let store = file_store(&store_dir);
        assert_eq!(
            store
                .get(INFERENCE_SERVER_SECRET_SERVICE, "gateway")
                .unwrap()
                .as_deref(),
            Some("sk-live")
        );
        assert_eq!(
            store
                .get(BACKEND_SECRET_SERVICE, "gateway")
                .unwrap()
                .as_deref(),
            Some("sk-live")
        );
        let row = repo.find_by_name("gateway").unwrap().unwrap();
        assert!(
            row.config_json.get("api_key").is_none(),
            "{}",
            row.config_json
        );
        assert!(key_in_store(&row.config_json));
        assert_eq!(
            seal_stored_backend_keys(&repo, no_store),
            0,
            "nothing left in clear"
        );
    }

    // GIVEN a backend whose key sits in the store
    // WHEN it is saved again with `${VAR}`, then with no key at all
    // THEN the variable name stays in `config_json` and the stored key is removed
    #[test]
    fn test_a_key_replaced_by_a_variable_leaves_the_store() {
        // GIVEN
        let dir = tempfile::TempDir::new().unwrap();
        let mut cfg = gateway(serde_json::json!({ "api_key": "sk-old" }));
        seal_backend_key(&mut cfg, false, || Ok(file_store(&dir))).unwrap();
        let mut by_env = gateway(serde_json::json!({
            "api_key": "${GATEWAY_KEY}",
            KEY_IN_STORE_FIELD: true,
        }));

        // WHEN
        seal_backend_key(&mut by_env, true, || Ok(file_store(&dir))).unwrap();

        // THEN
        assert_eq!(
            by_env.config_json,
            serde_json::json!({ "api_key": "${GATEWAY_KEY}" })
        );
        assert!(file_store(&dir)
            .get(BACKEND_SECRET_SERVICE, "gateway")
            .unwrap()
            .is_none());
    }

    // GIVEN backends with no key, with a `${VAR}` key, and a store that must
    //       not be opened
    // WHEN they are sealed, then unsealed
    // THEN nothing changes and the store is never opened
    #[test]
    fn test_a_backend_without_a_stored_key_never_opens_the_store() {
        // GIVEN
        let mut configs = vec![
            gateway(serde_json::json!({})),
            gateway(serde_json::json!({ "api_key": "${GATEWAY_KEY}" })),
        ];
        let before: Vec<serde_json::Value> =
            configs.iter().map(|c| c.config_json.clone()).collect();

        // WHEN
        for cfg in configs.iter_mut() {
            seal_backend_key(cfg, false, no_store).unwrap();
        }
        unseal_backend_keys(&mut configs, no_store);

        // THEN
        let after: Vec<serde_json::Value> = configs.iter().map(|c| c.config_json.clone()).collect();
        assert_eq!(after, before);
    }

    // GIVEN a key given in clear, and a store that cannot be opened
    // WHEN the backend is sealed
    // THEN it fails, and the key is still in memory only, to be refused
    #[test]
    fn test_a_store_that_cannot_be_opened_refuses_the_key() {
        // GIVEN
        let mut cfg = gateway(serde_json::json!({ "api_key": "sk-x" }));

        // WHEN
        let result = seal_backend_key(&mut cfg, false, no_store);

        // THEN
        assert!(result.is_err());
    }
}
