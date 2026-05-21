//! Auth manager — high-level token lifecycle for SaaS connectors.
//!
//! Wraps [`MultiAccountStorage`] with:
//! - Automatic refresh when the access token has expired (or will within a leeway).
//! - **Singleflight** semantics so concurrent calls for the same
//!   `(provider, account_id)` share a single refresh request — without this,
//!   a burst of N parallel tool calls would emit N refresh requests against
//!   the OAuth Authorization Server, exhausting rate limits.
//! - A small in-memory cache for the active access token so subsequent calls
//!   skip the keyring round-trip until the token expires.
//!
//! The refresh-once-on-401 retry pattern lives in `apollia-connectors::http` —
//! this module only ensures the **token** is current; the HTTP layer decides
//! when to re-attempt against a 401.

use std::sync::Arc;

use chrono::{Duration, Utc};
use dashmap::DashMap;
use tokio::sync::Mutex;

use crate::{
    connector_providers::ConnectorProvider,
    error::AuthError,
    multi_account::{AccountId, MultiAccountStorage},
    providers::ProviderConfig,
    token::{refresh_token, StoredToken},
};

/// Refresh leeway — refresh proactively this duration before expiry to avoid
/// returning a token that would expire mid-request.
const REFRESH_LEEWAY: Duration = Duration::seconds(60);

// ─── Key ─────────────────────────────────────────────────────────────────────

/// Internal cache + singleflight key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    provider: ConnectorProvider,
    account_id: String,
}

// ─── Manager ─────────────────────────────────────────────────────────────────

/// High-level connector auth manager.
///
/// Construct once at runtime startup and share via `Arc`.
pub struct AuthManager {
    storage: MultiAccountStorage,
    /// In-memory cache of the currently valid token per (provider, account).
    cache: DashMap<CacheKey, StoredToken>,
    /// Singleflight: per-key mutex so only one refresh runs at a time.
    /// Concurrent callers contend on the lock, then re-check the cache after
    /// the lock is acquired.
    refresh_locks: DashMap<CacheKey, Arc<Mutex<()>>>,
}

impl AuthManager {
    /// Build an [`AuthManager`] backed by the default index location.
    pub fn new() -> Result<Self, AuthError> {
        Ok(Self {
            storage: MultiAccountStorage::new()?,
            cache: DashMap::new(),
            refresh_locks: DashMap::new(),
        })
    }

    /// Build an [`AuthManager`] backed by a custom storage (used in tests).
    pub fn with_storage(storage: MultiAccountStorage) -> Self {
        Self {
            storage,
            cache: DashMap::new(),
            refresh_locks: DashMap::new(),
        }
    }

    /// Store an initial token after a successful OAuth flow.
    ///
    /// Replaces any existing token for the same `(provider, account_id)`.
    pub async fn put_token(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
        token: StoredToken,
    ) -> Result<(), AuthError> {
        self.storage.store(provider, account_id, &token).await?;
        self.cache.insert(
            CacheKey {
                provider,
                account_id: account_id.as_str().to_owned(),
            },
            token,
        );
        Ok(())
    }

    /// Get a currently valid access token for `(provider, account_id)`.
    ///
    /// Refreshes automatically if the cached token is expired (or within
    /// [`REFRESH_LEEWAY`] of expiry). Multiple concurrent callers share a
    /// single refresh request via the per-key mutex (singleflight).
    pub async fn get_valid_token(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
        provider_config: &ProviderConfig,
    ) -> Result<StoredToken, AuthError> {
        let key = CacheKey {
            provider,
            account_id: account_id.as_str().to_owned(),
        };

        // Fast path: cached token still valid (with leeway).
        if let Some(token) = self.cache.get(&key) {
            if !is_near_expiry(&token) {
                return Ok(token.clone());
            }
        }

        // Slow path: acquire the per-key lock, double-check, refresh if needed.
        let lock = self
            .refresh_locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        // Re-check cache after acquiring the lock — another task may have
        // refreshed while we were waiting.
        if let Some(token) = self.cache.get(&key) {
            if !is_near_expiry(&token) {
                return Ok(token.clone());
            }
        }

        // Load from keyring (the cache miss path on first call after restart).
        let stored = match self.storage.load(provider, account_id).await? {
            Some(t) if !is_near_expiry(&t) => {
                self.cache.insert(key.clone(), t.clone());
                return Ok(t);
            }
            Some(t) => t,
            None => return Err(AuthError::UnknownProvider(format!(
                "no token for {}/{account_id}",
                provider.id()
            ))),
        };

        // Refresh.
        let refreshed = refresh_token(provider_config, &stored).await?;
        self.storage.store(provider, account_id, &refreshed).await?;
        self.cache.insert(key, refreshed.clone());
        Ok(refreshed)
    }

    /// Revoke (forget) a stored token for `(provider, account_id)`.
    ///
    /// The token is removed from both the cache and the keyring. The OAuth
    /// Authorization Server is **not** notified — call the provider revocation
    /// endpoint separately if a server-side revocation is required.
    pub async fn revoke(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
    ) -> Result<(), AuthError> {
        let key = CacheKey {
            provider,
            account_id: account_id.as_str().to_owned(),
        };
        self.cache.remove(&key);
        self.refresh_locks.remove(&key);
        self.storage.delete(provider, account_id).await
    }

    /// List the connected accounts for `provider`.
    pub async fn list_accounts(
        &self,
        provider: ConnectorProvider,
    ) -> Result<Vec<AccountId>, AuthError> {
        self.storage.list_accounts(provider).await
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn is_near_expiry(token: &StoredToken) -> bool {
    match token.expires_at {
        None => false,
        Some(exp) => exp <= Utc::now() + REFRESH_LEEWAY,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn token_expiring_in(secs: i64) -> StoredToken {
        StoredToken {
            access_token: format!("at-{secs}"),
            refresh_token: Some("refresh".into()),
            expires_at: Some(Utc::now() + Duration::seconds(secs)),
            scopes: vec![],
        }
    }

    fn temp_manager() -> (AuthManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = MultiAccountStorage::with_index_path(dir.path().join("idx.json"));
        (AuthManager::with_storage(storage), dir)
    }

    #[test]
    fn test_is_near_expiry_when_no_expiry_returns_false() {
        // GIVEN a token without expiry
        let token = StoredToken {
            access_token: "x".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        // THEN never near expiry
        assert!(!is_near_expiry(&token));
    }

    #[test]
    fn test_is_near_expiry_when_in_far_future_returns_false() {
        let token = token_expiring_in(3600);
        assert!(!is_near_expiry(&token));
    }

    #[test]
    fn test_is_near_expiry_when_within_leeway_returns_true() {
        // GIVEN a token expiring in 30 seconds (within 60s leeway)
        let token = token_expiring_in(30);
        // THEN considered near expiry
        assert!(is_near_expiry(&token));
    }

    #[test]
    fn test_is_near_expiry_when_already_expired_returns_true() {
        let token = token_expiring_in(-10);
        assert!(is_near_expiry(&token));
    }

    /// Generate a unique account id per test so concurrent / repeated runs
    /// don't collide on the same OS keychain entry (the keyring backend now
    /// writes through to the real macOS keychain — ADR-todo).
    fn unique_account(label: &str) -> AccountId {
        AccountId::new(format!(
            "apollia-test-{label}-{}@example.invalid",
            uuid::Uuid::new_v4()
        ))
    }

    #[tokio::test]
    async fn test_get_valid_token_returns_cached_when_valid() {
        // GIVEN a manager with a freshly cached non-expired token
        let (manager, _dir) = temp_manager();
        let token = token_expiring_in(3600);
        let account = unique_account("cached-valid");
        manager
            .put_token(ConnectorProvider::Google, &account, token.clone())
            .await
            .expect("put");

        // WHEN we request a valid token
        // (build a minimal ProviderConfig — never actually used since we hit cache)
        let cfg = crate::connector_providers::build_google_provider(&[]);
        let got = manager
            .get_valid_token(ConnectorProvider::Google, &account, &cfg)
            .await
            .expect("get");

        // THEN we get the cached one
        assert_eq!(got.access_token, token.access_token);
    }

    #[tokio::test]
    async fn test_revoke_clears_cache_and_storage() {
        // GIVEN a stored + cached token
        let (manager, _dir) = temp_manager();
        let token = token_expiring_in(3600);
        let account = unique_account("revoke");
        manager
            .put_token(ConnectorProvider::Google, &account, token)
            .await
            .expect("put");

        // WHEN we revoke
        manager
            .revoke(ConnectorProvider::Google, &account)
            .await
            .expect("revoke");

        // THEN cache is empty
        let key = CacheKey {
            provider: ConnectorProvider::Google,
            account_id: account.as_str().to_owned(),
        };
        assert!(manager.cache.get(&key).is_none());
        // AND storage is empty
        assert!(
            manager
                .storage
                .load(ConnectorProvider::Google, &account)
                .await
                .expect("load")
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_list_accounts_returns_registered_ones() {
        let (manager, _dir) = temp_manager();
        let a = unique_account("list-a");
        let b = unique_account("list-b");
        manager
            .put_token(ConnectorProvider::Google, &a, token_expiring_in(3600))
            .await
            .expect("put a");
        manager
            .put_token(ConnectorProvider::Google, &b, token_expiring_in(3600))
            .await
            .expect("put b");

        let accounts = manager
            .list_accounts(ConnectorProvider::Google)
            .await
            .expect("list");
        assert_eq!(accounts.len(), 2);
        assert!(accounts.contains(&a));
        assert!(accounts.contains(&b));
    }
}
