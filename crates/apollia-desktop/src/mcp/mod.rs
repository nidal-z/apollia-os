//! MCP integration layer for the desktop application.
//!
//! Provides the registry client for discovering MCP servers from the official
//! registry, builtin connector enrichments, and Tauri IPC commands. MCP
//! secrets go through the shared `apollia_auth::SecretStore`.

pub mod enrichments;
pub mod npm_server;
pub mod origin;
pub mod overrides;
pub mod python_server;

/// The registry client lives in `apollia-mcp`, shared with the runtime's
/// `/api/v1/mcp/registry` routes; the desktop reads it under its old path.
pub use apollia_mcp::registry as registry_client;

pub use enrichments::{
    find_enrichment, load_builtin_enrichments, ConnectorEnrichment, CostKind, CostModel, TrustLevel,
};
pub use overrides::{apply_overrides, default_overrides_path, load_overrides_from, McpOverrides};
pub use registry_client::{
    McpRegistryClient, RegistryClientError, RegistryEnvVar, RegistryIcon, RegistryListResponse,
    RegistryMetadata, RegistryPackage, RegistryPackageArg, RegistryRepository, RegistryServer,
    RegistryServerDetail, RegistryTransport,
};
