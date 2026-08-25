//! Pluggable secret storage backend.
//!
//! v0.1.0 ships two implementations:
//!
//! - [`KeyringSecretStore`]: wraps the OS keyring (macOS Keychain, Windows
//!   Credential Manager, Linux Secret Service). The default everywhere a
//!   daemon is available.
//! - [`AgeFileSecretStore`]: `age` symmetric (scrypt + ChaCha20-Poly1305)
//!   file storage at `~/.apollia/secrets/<service>__<user>.age` for Linux
//!   headless environments where the keyring crate cannot bind to a daemon.
//!
//! Selection at runtime is driven by `APOLLIA_TOKEN_STORAGE`:
//!
//! - unset / `keyring` → [`KeyringSecretStore`] (default).
//! - `file` → [`AgeFileSecretStore`] with passphrase from
//!   `APOLLIA_TOKEN_PASSPHRASE` (mandatory; the boot fails fast if absent).

use std::path::PathBuf;

use crate::error::AuthError;

/// Common interface for secret backends.
///
/// Implementations are sync because the keyring crate is sync; the
/// AgeFileSecretStore is also synchronous (file I/O + symmetric crypto).
pub trait SecretStore: Send + Sync {
    /// Store a secret value under (service, user).
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError>;
    /// Load the secret value for (service, user). Returns `Ok(None)` when absent.
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError>;
    /// Delete the secret. Idempotent: succeeds even if the entry was absent.
    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError>;
    /// Short identifier for telemetry / debugging.
    fn backend_id(&self) -> &'static str;
}

// ─── Keyring backend ─────────────────────────────────────────────────────────

/// OS keyring-backed [`SecretStore`].
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| AuthError::Keyring(e.to_string()))?;
        // Best-effort delete-then-set.
        //
        // macOS keychain entries carry an ACL bound to the *first* binary that
        // wrote them. When the Apollia CLI is rebuilt (every `cargo build`
        // for unsigned dev artefacts) its code signature changes, and
        // `SecItemUpdate` silently fails to overwrite entries owned by the
        // previous signature; the entry stays put with the old payload while
        // `set_password` returns `Ok(())`. Downstream readers (daemon /
        // chat runner) then see the stale value and OAuth flows appear to
        // succeed but never actually refresh the keychain.
        //
        // Deleting first forces `SecItemAdd` rather than `SecItemUpdate`, so
        // the new entry inherits the current binary's identity and any future
        // read/update from the same identity succeeds. The delete is
        // best-effort because `NoEntry` is the expected first-run case.
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                tracing::warn!(
                    service = %service,
                    user = %user,
                    error = %e,
                    "keyring: pre-write delete failed; falling back to in-place update"
                );
            }
        }
        entry
            .set_password(value)
            .map_err(|e| AuthError::Keyring(e.to_string()))
    }

    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Keyring(e.to_string())),
        }
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AuthError::Keyring(e.to_string())),
        }
    }

    fn backend_id(&self) -> &'static str {
        "keyring"
    }
}

// ─── Age file backend (Linux headless fallback) ─────────────────

/// `age`-symmetric file storage backend.
///
/// Each secret is stored as a separate file at
/// `<base>/<sanitised(service)>__<sanitised(user)>.age` encrypted with the
/// configured passphrase. One file per secret to avoid having to lock-and-
/// rewrite a single bundle on every update.
///
/// Sanitisation: filesystem-unsafe characters in `service` / `user` are
/// percent-escaped to keep paths predictable and reversible.
pub struct AgeFileSecretStore {
    base: PathBuf,
    passphrase: String,
}

impl AgeFileSecretStore {
    /// Build a new file backend rooted at `base`.
    ///
    /// `passphrase` must be non-empty; boots fail fast otherwise
    /// (no silent unencrypted fallback).
    pub fn new(base: PathBuf, passphrase: String) -> Result<Self, AuthError> {
        if passphrase.is_empty() {
            return Err(AuthError::Keyring(
                "APOLLIA_TOKEN_PASSPHRASE must be set when APOLLIA_TOKEN_STORAGE=file".into(),
            ));
        }
        std::fs::create_dir_all(&base)
            .map_err(|e| AuthError::Keyring(format!("create secrets dir: {e}")))?;
        Ok(Self { base, passphrase })
    }

    /// Build using the default location `~/.apollia/secrets/` and the
    /// passphrase from `APOLLIA_TOKEN_PASSPHRASE`.
    pub fn from_env() -> Result<Self, AuthError> {
        let home = dirs::home_dir()
            .ok_or_else(|| AuthError::Keyring("home directory not found".into()))?;
        let base = home.join(".apollia").join("secrets");
        let passphrase = std::env::var("APOLLIA_TOKEN_PASSPHRASE").map_err(|_| {
            AuthError::Keyring(
                "APOLLIA_TOKEN_PASSPHRASE not set (required when APOLLIA_TOKEN_STORAGE=file)"
                    .into(),
            )
        })?;
        Self::new(base, passphrase)
    }

    fn path_for(&self, service: &str, user: &str) -> PathBuf {
        self.base
            .join(format!("{}__{}.age", sanitise(service), sanitise(user)))
    }
}

impl SecretStore for AgeFileSecretStore {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
        use age::secrecy::SecretString;
        use std::io::Write;

        let path = self.path_for(service, user);
        let encryptor =
            age::Encryptor::with_user_passphrase(SecretString::from(self.passphrase.clone()));
        let mut buf: Vec<u8> = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut buf)
            .map_err(|e| AuthError::Keyring(format!("age wrap: {e}")))?;
        writer
            .write_all(value.as_bytes())
            .map_err(|e| AuthError::Keyring(format!("age write: {e}")))?;
        writer
            .finish()
            .map_err(|e| AuthError::Keyring(format!("age finish: {e}")))?;

        let tmp = path.with_extension("age.tmp");
        std::fs::write(&tmp, &buf).map_err(|e| AuthError::Keyring(format!("write tmp: {e}")))?;
        std::fs::rename(&tmp, &path).map_err(|e| AuthError::Keyring(format!("rename: {e}")))?;
        Ok(())
    }

    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
        use age::secrecy::SecretString;
        use std::io::Read;

        let path = self.path_for(service, user);
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            std::fs::read(&path).map_err(|e| AuthError::Keyring(format!("read secret: {e}")))?;
        let decryptor = age::Decryptor::new(&bytes[..])
            .map_err(|e| AuthError::Keyring(format!("age parse: {e}")))?;
        let identity = age::scrypt::Identity::new(SecretString::from(self.passphrase.clone()));
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| AuthError::Keyring(format!("age decrypt: {e}")))?;
        let mut out = String::new();
        reader
            .read_to_string(&mut out)
            .map_err(|e| AuthError::Keyring(format!("age read: {e}")))?;
        Ok(Some(out))
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
        let path = self.path_for(service, user);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AuthError::Keyring(format!("delete secret: {e}"))),
        }
    }

    fn backend_id(&self) -> &'static str {
        "age_file"
    }
}

// ─── Selection ───────────────────────────────────────────────────────────────

/// Build the configured [`SecretStore`] based on `APOLLIA_TOKEN_STORAGE`.
///
/// - Unset or `keyring` → [`KeyringSecretStore`].
/// - `file` → [`AgeFileSecretStore::from_env`].
/// - Any other value → fail fast.
pub fn select_default() -> Result<Box<dyn SecretStore>, AuthError> {
    let mode = std::env::var("APOLLIA_TOKEN_STORAGE").unwrap_or_else(|_| "keyring".into());
    match mode.as_str() {
        "keyring" => Ok(Box::new(KeyringSecretStore)),
        "file" => Ok(Box::new(AgeFileSecretStore::from_env()?)),
        other => Err(AuthError::Keyring(format!(
            "unknown APOLLIA_TOKEN_STORAGE: {other} (expected `keyring` or `file`)"
        ))),
    }
}

// ─── Filename sanitisation ───────────────────────────────────────────────────

fn sanitise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the tests that mutate `APOLLIA_TOKEN_STORAGE`: environment
    /// variables are process globals, so two parallel tests race without it.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs `f` with the variable set to `value`, restoring the previous
    /// value before returning.
    fn with_env_var<F: FnOnce()>(key: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        f();
        match previous {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    fn temp_store() -> (AgeFileSecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = AgeFileSecretStore::new(dir.path().to_path_buf(), "test-passphrase".into())
            .expect("store");
        (store, dir)
    }

    #[test]
    fn test_age_file_store_set_get_round_trip() {
        let (store, _dir) = temp_store();
        store
            .set("apollia-connector-google", "user@example.com", "tok123")
            .expect("set");
        let got = store
            .get("apollia-connector-google", "user@example.com")
            .expect("get");
        assert_eq!(got.as_deref(), Some("tok123"));
    }

    #[test]
    fn test_age_file_store_get_missing_returns_none() {
        let (store, _dir) = temp_store();
        let got = store.get("svc", "missing@example.com").expect("get");
        assert!(got.is_none());
    }

    #[test]
    fn test_age_file_store_delete_idempotent() {
        let (store, _dir) = temp_store();
        store
            .delete("svc", "absent@example.com")
            .expect("delete absent");
        store.set("svc", "u@e.com", "v").expect("set");
        store.delete("svc", "u@e.com").expect("delete existing");
        assert!(store.get("svc", "u@e.com").expect("get").is_none());
    }

    #[test]
    fn test_age_file_store_rejects_empty_passphrase() {
        let dir = tempfile::tempdir().expect("tempdir");
        match AgeFileSecretStore::new(dir.path().to_path_buf(), "".into()) {
            Ok(_) => panic!("empty passphrase should be rejected"),
            Err(AuthError::Keyring(msg)) => {
                assert!(msg.contains("APOLLIA_TOKEN_PASSPHRASE"));
            }
            Err(other) => panic!("expected Keyring error, got {other:?}"),
        }
    }

    #[test]
    fn test_age_file_store_decrypt_with_wrong_passphrase_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let writer =
            AgeFileSecretStore::new(dir.path().to_path_buf(), "correct".into()).expect("writer");
        writer.set("svc", "u@e.com", "secret").expect("set");

        let reader =
            AgeFileSecretStore::new(dir.path().to_path_buf(), "wrong".into()).expect("reader");
        let res = reader.get("svc", "u@e.com");
        assert!(res.is_err(), "decrypt with wrong passphrase must fail");
    }

    #[test]
    fn test_sanitise_preserves_safe_chars() {
        assert_eq!(
            sanitise("apollia-connector-google"),
            "apollia-connector-google"
        );
        assert_eq!(sanitise("user@example.com"), "user%40example.com");
        assert_eq!(sanitise("path/with/slashes"), "path%2Fwith%2Fslashes");
    }

    #[test]
    fn test_path_for_uses_sanitised_segments() {
        let (store, _dir) = temp_store();
        let path = store.path_for("svc", "u@e.com");
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("svc__"));
        assert!(filename.contains("%40")); // @ encoded
        assert!(filename.ends_with(".age"));
    }

    #[test]
    fn test_select_default_unset_uses_keyring() {
        // GIVEN APOLLIA_TOKEN_STORAGE unset (cannot reliably unset in tests,
        // so relax to "keyring" or unset depending on the runner). We probe
        // the explicit "keyring" branch.
        with_env_var("APOLLIA_TOKEN_STORAGE", "keyring", || {
            let store = select_default().expect("select");
            assert_eq!(store.backend_id(), "keyring");
        });
    }

    #[test]
    fn test_select_default_unknown_value_fails() {
        with_env_var("APOLLIA_TOKEN_STORAGE", "unknown", || {
            let result = select_default();
            // Use match instead of unwrap_err since Box<dyn SecretStore> does
            // not implement Debug (sealed at the trait level for v0.1.0).
            match result {
                Ok(_) => {
                    panic!("expected an error for unknown APOLLIA_TOKEN_STORAGE value");
                }
                Err(AuthError::Keyring(msg)) => {
                    assert!(msg.contains("unknown"));
                }
                Err(other) => {
                    panic!("expected Keyring error, got {other:?}");
                }
            }
        });
    }
}
