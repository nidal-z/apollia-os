//! An authentication manager over an in-memory token store, so a test reads
//! the token of an account without the OS keychain or the network.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apollia_auth::{
    AccountId, AuthError, AuthManager, ConnectorProvider, MultiAccountStorage, SecretStore,
    StoredToken,
};

#[derive(Default)]
struct MemoryStore(Mutex<HashMap<(String, String), String>>);

impl SecretStore for MemoryStore {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
        self.0
            .lock()
            .unwrap()
            .insert((service.to_owned(), user.to_owned()), value.to_owned());
        Ok(())
    }

    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(service.to_owned(), user.to_owned()))
            .cloned())
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service.to_owned(), user.to_owned()));
        Ok(())
    }

    fn backend_id(&self) -> &'static str {
        "memory"
    }
}

/// An authentication manager holding, for each `(account, token)` of
/// `tokens`, a token of `provider` that never expires. The index lives in
/// `dir`.
pub(crate) async fn auth_with_tokens(
    dir: &std::path::Path,
    provider: ConnectorProvider,
    tokens: &[(&str, &str)],
) -> Arc<AuthManager> {
    let storage = MultiAccountStorage::with_store(
        dir.join("connectors-index.json"),
        Box::new(MemoryStore::default()),
    );
    let auth = Arc::new(AuthManager::with_storage(storage));
    for (account, token) in tokens {
        auth.put_token(
            provider,
            &AccountId::new(*account),
            StoredToken {
                access_token: (*token).to_owned(),
                refresh_token: None,
                expires_at: None,
                scopes: Vec::new(),
            },
        )
        .await
        .unwrap();
    }
    auth
}
