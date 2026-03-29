//! MCP integration layer for the desktop application.
//!
//! Provides the registry client for discovering MCP servers from the official
//! registry, and will host the secret store and Tauri IPC commands in
//! subsequent stories.

pub mod registry_client;

pub use registry_client::{
    McpRegistryClient, RegistryClientError, RegistryEnvVar, RegistryIcon, RegistryListResponse,
    RegistryMetadata, RegistryPackage, RegistryPackageArg, RegistryRepository, RegistryServer,
    RegistryServerDetail, RegistryTransport,
};
