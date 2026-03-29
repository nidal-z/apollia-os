//! MCP integration layer for the desktop application.
//!
//! Provides the registry client for discovering MCP servers from the official
//! registry, the OS keychain secret store, builtin connector enrichments, and
//! Tauri IPC commands.

pub mod enrichments;
pub mod registry_client;
pub mod secret_store;

pub use enrichments::{find_enrichment, load_builtin_enrichments, ConnectorEnrichment, TrustLevel};
pub use registry_client::{
    McpRegistryClient, RegistryClientError, RegistryEnvVar, RegistryIcon, RegistryListResponse,
    RegistryMetadata, RegistryPackage, RegistryPackageArg, RegistryRepository, RegistryServer,
    RegistryServerDetail, RegistryTransport,
};
pub use secret_store::{SecretStore, SecretStoreError};
