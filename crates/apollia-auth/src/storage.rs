//! OS-native keyring storage for OAuth2 tokens.
//!
//! Delegates to the platform keyring backend:
//! - macOS: Keychain Services
//! - Linux: Secret Service (GNOME Keyring / KWallet via D-Bus)
//! - Windows: Credential Manager

use keyring::Entry;

use crate::{error::AuthError, token::StoredToken};

/// Service name used as the keyring application identifier.
const SERVICE_NAME: &str = "apollia-auth";

/// Store, load, and delete OAuth2 tokens from the OS-native keyring.
pub struct KeyringStorage;

impl KeyringStorage {
    /// Serialize `token` as JSON and persist it for `provider_name` in the OS keyring.
    pub fn store(provider_name: &str, token: &StoredToken) -> Result<(), AuthError> {
        let json =
            serde_json::to_string(token).map_err(|e| AuthError::Serialization(e.to_string()))?;
        let entry = Entry::new(SERVICE_NAME, provider_name)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        entry
            .set_password(&json)
            .map_err(|e| AuthError::Keyring(e.to_string()))
    }

    /// Load and deserialize the stored token for `provider_name`.
    ///
    /// Returns `Ok(None)` if no token is stored.
    pub fn load(provider_name: &str) -> Result<Option<StoredToken>, AuthError> {
        let entry = Entry::new(SERVICE_NAME, provider_name)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(json) => {
                let token = serde_json::from_str(&json)
                    .map_err(|e| AuthError::Serialization(e.to_string()))?;
                Ok(Some(token))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Keyring(e.to_string())),
        }
    }

    /// Remove the stored token for `provider_name`.
    ///
    /// Returns `Ok(())` when the token was deleted or was never stored.
    pub fn delete(provider_name: &str) -> Result<(), AuthError> {
        let entry = Entry::new(SERVICE_NAME, provider_name)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AuthError::Keyring(e.to_string())),
        }
    }
}
