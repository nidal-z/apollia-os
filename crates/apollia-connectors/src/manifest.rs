//! Connector manifest: declarative description of a connector.

use serde::Serialize;

use crate::operation::OperationSpec;

/// Stable, human-readable description of a connector.
///
/// Surfaced in the desktop integrations panel and consumed by `apollia-tools`
/// to register the connector's operations as native tools.
///
/// `Serialize` only: manifests are produced by the Rust code (static `&str`
/// references) and never round-tripped from JSON. The UI consumes the
/// serialized form; introspection from outside the crate uses [`ConnectorSummary`].
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorManifest {
    /// Stable connector identifier (e.g. `"google"`, `"microsoft"`).
    pub id: &'static str,
    /// Display name (in English; i18n is handled at the UI layer).
    pub name: &'static str,
    /// Short one-line description.
    pub description: &'static str,
    /// Publisher (typically the SaaS company name).
    pub publisher: &'static str,
    /// Services this connector exposes (e.g. `["gmail", "gcal", "gdrive"]`).
    pub services: &'static [&'static str],
}

impl ConnectorManifest {
    /// True when the connector advertises an operation under `service`.
    ///
    /// Used by the UI to render service-level toggles.
    pub fn supports_service(&self, service: &str) -> bool {
        self.services.contains(&service)
    }
}

/// Lightweight summary of a connector exposed to non-connector callers.
///
/// Returned by [`ConnectorRegistry::manifests`](crate::registry::ConnectorRegistry::manifests)
/// so the runtime / UI can introspect what is available without pulling in the
/// full trait object.
#[derive(Debug, Clone)]
pub struct ConnectorSummary {
    /// Manifest of the connector.
    pub manifest: ConnectorManifest,
    /// Operations exposed.
    pub operations: Vec<OperationSpec>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_supports_service_returns_true_for_known() {
        // GIVEN a manifest declaring gmail + gcal
        let m = ConnectorManifest {
            id: "google",
            name: "Google",
            description: "Google Workspace",
            publisher: "Google",
            services: &["gmail", "gcal"],
        };
        // WHEN asked about gmail
        // THEN true
        assert!(m.supports_service("gmail"));
        assert!(m.supports_service("gcal"));
    }

    #[test]
    fn test_manifest_supports_service_returns_false_for_unknown() {
        let m = ConnectorManifest {
            id: "google",
            name: "Google",
            description: "Google Workspace",
            publisher: "Google",
            services: &["gmail"],
        };
        assert!(!m.supports_service("gcal"));
    }
}
