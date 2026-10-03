//! The Hugging Face access token, kept in the secret store of this machine.
//!
//! - [`get_hf_token`]: the stored token, for the field of the General settings
//! - [`set_hf_token`]: stores a token, replacing the previous one
//! - [`clear_hf_token`]: removes the stored token
//!
//! The token used to live in the memory of the Hub screen only and was lost
//! with it. It now sits in the configured [`SecretStore`] (OS keychain by
//! default, encrypted files when `APOLLIA_TOKEN_STORAGE=file`), and the
//! downloader reads it when a request carries none, so a gated repository
//! downloads without asking again. The value is never logged.

use apollia_auth::SecretStore;

/// Service under which the token is stored.
pub const HF_TOKEN_SERVICE: &str = "apollia-huggingface";

/// Account under which the token is stored: there is one token per machine.
const HF_TOKEN_ACCOUNT: &str = "read-token";

/// Stores `token` in `store`, trimmed. An empty token is refused: clearing is
/// its own gesture.
pub(crate) fn store_in(store: &dyn SecretStore, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("the Hugging Face token is empty".to_owned());
    }
    store
        .set(HF_TOKEN_SERVICE, HF_TOKEN_ACCOUNT, token)
        .map_err(|e| format!("cannot store the Hugging Face token: {e}"))?;
    tracing::event!(
        tracing::Level::INFO,
        backend = store.backend_id(),
        "model_hub.hf_token.stored"
    );
    Ok(())
}

/// The token stored in `store`, if any.
pub(crate) fn read_in(store: &dyn SecretStore) -> Option<String> {
    match store.get(HF_TOKEN_SERVICE, HF_TOKEN_ACCOUNT) {
        Ok(token) => token.filter(|t| !t.trim().is_empty()),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                backend = store.backend_id(),
                error = %e,
                "model_hub.hf_token.unreadable"
            );
            None
        }
    }
}

/// Removes the token from `store`. Succeeds when none was stored.
pub(crate) fn clear_in(store: &dyn SecretStore) -> Result<(), String> {
    store
        .delete(HF_TOKEN_SERVICE, HF_TOKEN_ACCOUNT)
        .map_err(|e| format!("cannot remove the Hugging Face token: {e}"))?;
    tracing::event!(
        tracing::Level::INFO,
        backend = store.backend_id(),
        "model_hub.hf_token.cleared"
    );
    Ok(())
}

/// The token a request to the Hub uses: the one it carries, else the stored
/// one.
pub(crate) fn passed_or_stored_in(
    store: Option<&dyn SecretStore>,
    passed: Option<String>,
) -> Option<String> {
    passed
        .filter(|t| !t.trim().is_empty())
        .or_else(|| store.and_then(read_in))
}

/// [`passed_or_stored_in`] against the configured secret store.
pub(crate) fn passed_or_stored(passed: Option<String>) -> Option<String> {
    if passed.as_deref().is_some_and(|t| !t.trim().is_empty()) {
        return passed;
    }
    let store = apollia_auth::select_secret_store().ok()?;
    passed_or_stored_in(Some(&*store), passed)
}

fn configured_store() -> Result<Box<dyn SecretStore>, String> {
    apollia_auth::select_secret_store().map_err(|e| format!("no secret store: {e}"))
}

/// The stored Hugging Face token, or `None` when there is none.
#[tauri::command]
pub async fn get_hf_token() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| configured_store().map(|store| read_in(&*store)))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Stores the Hugging Face token, replacing the previous one.
#[tauri::command]
pub async fn set_hf_token(token: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || store_in(&*configured_store()?, &token))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Removes the stored Hugging Face token.
#[tauri::command]
pub async fn clear_hf_token() -> Result<(), String> {
    tokio::task::spawn_blocking(|| clear_in(&*configured_store()?))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age_store(dir: &tempfile::TempDir) -> apollia_auth::AgeFileSecretStore {
        apollia_auth::AgeFileSecretStore::new(dir.path().to_path_buf(), "test-pass".into()).unwrap()
    }

    #[test]
    fn test_a_stored_token_is_read_back_trimmed() {
        // GIVEN an empty encrypted-file secret store
        let dir = tempfile::tempdir().unwrap();
        let store = age_store(&dir);
        assert_eq!(read_in(&store), None);

        // WHEN a token is stored with surrounding blanks
        store_in(&store, "  hf_secret123 \n").unwrap();

        // THEN it is read back without them, and not written in clear
        assert_eq!(read_in(&store).as_deref(), Some("hf_secret123"));
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("hf_secret123"), "token stored in clear");
        }
    }

    #[test]
    fn test_clearing_removes_the_token_and_an_empty_one_is_refused() {
        // GIVEN a store holding a token
        let dir = tempfile::tempdir().unwrap();
        let store = age_store(&dir);
        store_in(&store, "hf_first").unwrap();

        // WHEN an empty token is stored, then the token is cleared twice
        let empty = store_in(&store, "   ");
        let kept = read_in(&store);
        clear_in(&store).unwrap();
        let again = clear_in(&store);

        // THEN the empty one is refused and left the token, and clearing
        // removes it and succeeds when nothing is left
        assert!(empty.is_err());
        assert_eq!(kept.as_deref(), Some("hf_first"));
        assert_eq!(read_in(&store), None);
        assert!(again.is_ok());
    }

    #[test]
    fn test_the_downloader_reads_the_stored_token_when_the_request_has_none() {
        // GIVEN a store holding a token
        let dir = tempfile::tempdir().unwrap();
        let store = age_store(&dir);
        store_in(&store, "hf_stored").unwrap();

        // WHEN a request carries no token, an empty one, or its own
        let none = passed_or_stored_in(Some(&store), None);
        let blank = passed_or_stored_in(Some(&store), Some(String::new()));
        let own = passed_or_stored_in(Some(&store), Some("hf_passed".to_owned()));

        // THEN the stored token fills the first two, and the request's wins
        assert_eq!(none.as_deref(), Some("hf_stored"));
        assert_eq!(blank.as_deref(), Some("hf_stored"));
        assert_eq!(own.as_deref(), Some("hf_passed"));
    }
}
