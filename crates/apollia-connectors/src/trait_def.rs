//! The `Connector` trait: abstraction implemented by every native SaaS connector.

use async_trait::async_trait;

use apollia_auth::AccountId;

use crate::{error::ConnectorError, manifest::ConnectorManifest, operation::OperationSpec};

/// Health snapshot returned by [`Connector::check`].
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// True when the connector can reach the upstream API with the current token.
    pub reachable: bool,
    /// Scopes the token actually holds (as reported by the upstream).
    pub granted_scopes: Vec<String>,
    /// Free-form human-readable detail (failure reason, latency, etc.).
    pub detail: String,
}

/// A connector binds Apollia to an external service via OAuth + a typed API client.
///
/// Implementations expose a stable set of operations consumed by
/// `apollia-tools::registry`. Connectors are stateless wrappers: credentials
/// live in `apollia-auth` (keyring) and are fetched per-call.
///
/// The trait is intentionally minimal (4 methods). Anything beyond identity,
/// manifest, health check, and operation enumeration belongs to the connector's
/// own internal modules (per-service clients), not the trait surface.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    /// Stable connector identifier (e.g. `"google"`).
    fn id(&self) -> &'static str;

    /// Manifest describing the connector and its services.
    fn manifest(&self) -> &ConnectorManifest;

    /// Operations exposed by this connector. The list is stable for the
    /// lifetime of the runtime.
    fn operations(&self) -> &[OperationSpec];

    /// Verify that `account_id` has a valid token with the scopes required to
    /// reach the upstream API.
    ///
    /// Implementations should make a cheap upstream call (typically the
    /// userinfo / `/me` endpoint) and report the result. **Not** an exhaustive
    /// scope audit: surface `granted_scopes` from the token, not from a
    /// per-API probe.
    async fn check(&self, account_id: &AccountId) -> Result<HealthReport, ConnectorError>;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway connector used solely to exercise the trait surface in tests.
    struct DummyConnector;

    #[async_trait]
    impl Connector for DummyConnector {
        fn id(&self) -> &'static str {
            "dummy"
        }

        fn manifest(&self) -> &ConnectorManifest {
            const MANIFEST: ConnectorManifest = ConnectorManifest {
                id: "dummy",
                name: "Dummy",
                description: "Test fixture connector",
                publisher: "Apollia",
                services: &["noop"],
            };
            &MANIFEST
        }

        fn operations(&self) -> &[OperationSpec] {
            &[]
        }

        async fn check(&self, _account_id: &AccountId) -> Result<HealthReport, ConnectorError> {
            Ok(HealthReport {
                reachable: true,
                granted_scopes: vec![],
                detail: "always healthy".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_dummy_connector_check_returns_reachable() {
        // GIVEN a dummy connector
        let c = DummyConnector;
        let account = AccountId::new("test@example.com");
        // WHEN we call check
        let report = c.check(&account).await.expect("check");
        // THEN reachable=true with the expected detail
        assert!(report.reachable);
        assert_eq!(report.detail, "always healthy");
    }

    #[test]
    fn test_dummy_connector_id_and_manifest_match() {
        // GIVEN a dummy connector
        let c = DummyConnector;
        // WHEN we ask for id and manifest
        // THEN they line up
        assert_eq!(c.id(), "dummy");
        assert_eq!(c.manifest().id, "dummy");
        assert_eq!(c.operations().len(), 0);
    }
}
