//! Apollia OS — OAuth2 PKCE authentication crate.
//!
//! Provides an interactive OAuth2 PKCE login flow (RFC 7636) with:
//! - A local callback HTTP server for the authorization redirect
//! - OS-native keyring storage (macOS Keychain / Linux Secret Service / Windows Credential Store)
//! - Support for Anthropic, OpenAI, and Google Vertex AI providers
//!
//! # Typical login flow
//!
//! ```text
//! 1. bind_ephemeral_port()      → get (listener, port)
//! 2. OAuth2PkceFlow::new(port)  → generate verifier, challenge, state, redirect_uri
//! 3. build_auth_url(provider, flow) → full authorization URL
//! 4. open::that(&url)           → open browser
//! 5. wait_for_callback(listener, &flow.state) → authorization code
//! 6. exchange_code(provider, flow, code)      → StoredToken
//! 7. KeyringStorage::store(name, &token)      → persisted
//! ```

pub mod auth_manager;
pub mod callback;
pub mod connector_providers;
pub mod error;
pub mod mcp_oauth;
pub mod multi_account;
pub mod pkce;
pub mod providers;
pub mod secret_storage;
pub mod storage;
pub mod token;

pub use auth_manager::AuthManager;
pub use connector_providers::{
    build_google_provider, build_microsoft_provider, build_microsoft_provider_for_tenant,
    ConnectorProvider, GoogleScope, MicrosoftScope,
};
pub use error::AuthError;
pub use mcp_oauth::{
    as_metadata_candidates, canonical_resource_uri, parse_www_authenticate,
    AuthorizationServerMetadata, ClientIdMetadataDocument, DcrRequest, DcrResponse,
    McpDiscoveryClient, ProtectedResourceMetadata, WwwAuthenticate, APOLLIA_CIMD,
    APOLLIA_CIMD_URL,
};
pub use multi_account::{AccountId, MultiAccountStorage};
pub use secret_storage::{
    select_default as select_secret_store, AgeFileSecretStore, KeyringSecretStore, SecretStore,
};
pub use pkce::{build_auth_url, generate_code_challenge, generate_code_verifier, OAuth2PkceFlow};
pub use providers::{get_provider, ProviderConfig, SUPPORTED_PROVIDERS};
pub use storage::KeyringStorage;
pub use token::StoredToken;
