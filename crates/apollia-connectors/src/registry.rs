//! In-process registry of active connectors.
//!
//! Populated once at runtime startup from the build-time list of connectors
//! (see ADR-090 — no dynamic plugin loading in v0.1.0). The registry exposes:
//! - [`ConnectorRegistry::register`] — add a connector instance.
//! - [`ConnectorRegistry::get`] — look up by id.
//! - [`ConnectorRegistry::list`] — enumerate all registered connectors.
//! - [`ConnectorRegistry::manifests`] — lightweight summary for the UI.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{manifest::ConnectorSummary, trait_def::Connector};

/// Thread-safe registry of [`Connector`] instances.
#[derive(Default, Clone)]
pub struct ConnectorRegistry {
    inner: Arc<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    by_id: tokio::sync::RwLock<HashMap<&'static str, Arc<dyn Connector>>>,
}

impl ConnectorRegistry {
    /// Build an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a connector instance.
    ///
    /// If a connector with the same id is already registered, the new one
    /// replaces it and the previous instance is dropped — useful in tests, a
    /// no-op consequence in production where each id is wired once.
    pub async fn register<C: Connector>(&self, connector: C) {
        let id = connector.id();
        let arc: Arc<dyn Connector> = Arc::new(connector);
        let mut guard = self.inner.by_id.write().await;
        guard.insert(id, arc);
        tracing::info!(connector_id = %id, "connector.registered");
    }

    /// Look up a connector by id.
    pub async fn get(&self, id: &str) -> Option<Arc<dyn Connector>> {
        let guard = self.inner.by_id.read().await;
        guard.get(id).cloned()
    }

    /// List the ids of all registered connectors, in arbitrary order.
    pub async fn list(&self) -> Vec<&'static str> {
        let guard = self.inner.by_id.read().await;
        guard.keys().copied().collect()
    }

    /// Return a [`ConnectorSummary`] (manifest + operations) for every
    /// registered connector — UI-friendly snapshot.
    pub async fn manifests(&self) -> Vec<ConnectorSummary> {
        let guard = self.inner.by_id.read().await;
        guard
            .values()
            .map(|c| ConnectorSummary {
                manifest: c.manifest().clone(),
                operations: c.operations().to_vec(),
            })
            .collect()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::ConnectorError, manifest::ConnectorManifest, operation::OperationSpec,
        trait_def::HealthReport,
    };
    use apollia_auth::AccountId;
    use async_trait::async_trait;

    struct Stub {
        id: &'static str,
    }

    #[async_trait]
    impl Connector for Stub {
        fn id(&self) -> &'static str {
            self.id
        }

        fn manifest(&self) -> &ConnectorManifest {
            const MANIFEST: ConnectorManifest = ConnectorManifest {
                id: "stub",
                name: "Stub",
                description: "test",
                publisher: "apollia",
                services: &[],
            };
            &MANIFEST
        }

        fn operations(&self) -> &[OperationSpec] {
            &[]
        }

        async fn check(&self, _: &AccountId) -> Result<HealthReport, ConnectorError> {
            Ok(HealthReport {
                reachable: true,
                granted_scopes: vec![],
                detail: "ok".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_empty_registry_returns_none() {
        let r = ConnectorRegistry::new();
        assert!(r.get("anything").await.is_none());
        assert!(r.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let r = ConnectorRegistry::new();
        r.register(Stub { id: "google" }).await;
        let got = r.get("google").await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().id(), "google");
    }

    #[tokio::test]
    async fn test_register_twice_replaces() {
        // GIVEN a registry with two connectors registered under the same id
        let r = ConnectorRegistry::new();
        r.register(Stub { id: "google" }).await;
        r.register(Stub { id: "google" }).await;
        // THEN there is exactly one entry
        assert_eq!(r.list().await.len(), 1);
    }

    #[tokio::test]
    async fn test_list_returns_all_registered() {
        let r = ConnectorRegistry::new();
        r.register(Stub { id: "google" }).await;
        r.register(Stub { id: "microsoft" }).await;
        let mut ids = r.list().await;
        ids.sort();
        assert_eq!(ids, vec!["google", "microsoft"]);
    }

    #[tokio::test]
    async fn test_manifests_returns_summary_for_each() {
        let r = ConnectorRegistry::new();
        r.register(Stub { id: "google" }).await;
        let summaries = r.manifests().await;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].manifest.id, "stub");
    }
}
