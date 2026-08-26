//! The HTTP OAuth path of an MCP server: discovering the authorisation
//! server, resolving and storing the client id the operator supplies, and
//! running the login that yields an account.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─── MCP HTTP OAuth IPC ──────────────────────────────────────────────────────

/// Discovery result emitted by [`mcp_oauth_discover`].
///
/// Surfaces what the wizard needs to render the OAuth Auth step:
/// - `as_url` for telemetry / "you'll authenticate at <X>".
/// - `scopes_supported` populates the scope selector (defaults all checked).
/// - `scope_descriptions` lets the AS provide human labels - sparse in
///   practice, but rendered when available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthDiscoveryResult {
    pub as_url: String,
    pub scopes_supported: Vec<String>,
    /// Map of `scope → human-readable description`, when the AS exposes
    /// one. Sparse (most AS don't ship this).
    pub scope_descriptions: HashMap<String, String>,
    pub registration_supported: bool,
}

/// Sign-in outcome returned by [`mcp_oauth_login`]. Carries identity claims
/// for UI display ("Signed in as ...").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthAccount {
    /// `sub` claim from the access token JWT, when present.
    pub sub: Option<String>,
    /// `email` claim, when present.
    pub email: Option<String>,
    /// Effective scopes granted by the AS (may be a subset of requested).
    pub scopes: Vec<String>,
}

/// Discover the OAuth requirements of a remote MCP server.
///
/// Pre-condition: the caller already ran [`test_mcp_connection`] and observed
/// `OauthRequired { www_authenticate }`. This IPC runs the RFC 9728 + RFC 8414
/// discovery chain (with origin fallback) and returns enough metadata for the
/// wizard to render the scope selector before the OAuth dance starts.
///
/// Why a separate IPC from `mcp_oauth_login`? The user must see the scope list
/// (their explicit decision) before consenting. Folding both into
/// `negotiate_token` would force the browser to open before scope selection.
#[tauri::command]
pub async fn mcp_oauth_discover(
    url: String,
    www_authenticate: Option<String>,
) -> Result<McpOAuthDiscoveryResult, String> {
    let discovery = apollia_auth::McpDiscoveryClient::new()
        .map_err(|e| format!("init discovery client: {e}"))?;

    // 1. PRM: prefer the URL advertised by WWW-Authenticate, then fall back
    //    to the well-known at the server's origin.
    let prm = match www_authenticate
        .as_deref()
        .and_then(apollia_auth::parse_www_authenticate)
        .and_then(|wa| wa.resource_metadata)
    {
        Some(prm_url) => discovery.fetch_prm_at(&prm_url).await,
        None => discovery.fetch_prm(&url).await,
    }
    .map_err(|e| format!("fetch PRM: {e}"))?;

    let as_url = prm
        .authorization_servers
        .first()
        .cloned()
        .ok_or_else(|| "PRM declared no authorization servers".to_string())?;

    // 2. AS metadata.
    let as_metadata = discovery
        .fetch_as_metadata(&as_url)
        .await
        .map_err(|e| format!("fetch AS metadata: {e}"))?;

    // Refuse to surface OAuth as available when PKCE S256 isn't advertised -
    // matches the orchestrator's hard refusal at negotiation time so the UI
    // doesn't lure the user into a downgraded flow.
    if !as_metadata.supports_pkce_s256() {
        return Err(format!(
            "authorization server at {as_url} does not advertise PKCE S256"
        ));
    }

    // 3. Effective scope catalogue: PRM scopes_supported preferred (resource-
    //    defined), else nothing (we don't surface AS-wide scopes that may
    //    include unrelated services).
    let scopes_supported = if !prm.scopes_supported.is_empty() {
        prm.scopes_supported.clone()
    } else {
        // Fallback: no resource-level scopes published → leave empty so the
        // wizard defers to AS defaults (sends no `scope=`).
        Vec::new()
    };

    Ok(McpOAuthDiscoveryResult {
        as_url,
        scopes_supported,
        // Scope descriptions aren't part of RFC 8414; some AS extend the
        // metadata with `scopes_supported_description` or similar, but we
        // don't parse those today - leave empty.
        scope_descriptions: HashMap::new(),
        registration_supported: as_metadata.registration_endpoint.is_some(),
    })
}

/// Keychain service slot for user-entered OAuth client ids.
///
/// Holds the values typed into the wizard's OAuth client id input when the
/// catalog enrichment declares `oauth_pre_registered_client_id_env` and
/// neither the runtime env var nor the build-time constant resolves. Keeps
/// non-technical users out of the terminal while still supporting power-user
/// overrides (enterprise tenants that want their own Figma app).
const MCP_CLIENT_ID_SERVICE: &str = "apollia-mcp-client-ids";

/// Resolve a pre-registered OAuth client id, with three fallback layers so
/// end users get a turnkey experience.
///
/// Lookup order (mirrors the Google/Microsoft connector pattern):
/// 1. **Runtime env var** matching `env_var`, for dev / power-user override
///    (e.g. `APOLLIA_FIGMA_CLIENT_ID=xxx` exported before launch).
/// 2. **Keychain stored value** - set via the wizard input or settings panel
///    by users who don't want to touch env vars but registered their own app.
/// 3. **Build-time constant** baked into the binary via `option_env!` - set
///    when the release pipeline runs with `APOLLIA_BUILD_FIGMA_CLIENT_ID`
///    in the environment (cf. `OAUTH-SETUP-TUTO.md §4`). End users of the
///    release binary inherit this without setting anything.
///
/// Returns `None` only when all three layers are absent - the wizard then
/// shows an input field with the provider's registration help text.
#[tauri::command]
pub fn mcp_oauth_resolve_client_id(env_var: String) -> Option<String> {
    fn non_empty(v: &str) -> bool {
        !v.trim().is_empty()
    }
    if let Ok(v) = std::env::var(&env_var) {
        if non_empty(&v) {
            return Some(v);
        }
    }
    if let Some(v) = load_stored_client_id(&env_var) {
        if non_empty(&v) {
            return Some(v);
        }
    }
    compile_time_known_client_id(&env_var)
        .filter(|v| non_empty(v))
        .map(str::to_string)
}

/// Persist a user-entered OAuth client id to the OS keychain.
///
/// Called by the wizard when the operator pastes a `client_id` into the
/// input field shown for connectors that require manual app registration
/// (Figma today). Subsequent `mcp_oauth_resolve_client_id` calls will
/// return this value (priority 2) until the user clears it.
///
/// Returns `Ok(())` on success. The store is keyed by `env_var` so each
/// provider lives in its own keychain entry, never collides.
#[tauri::command]
pub fn mcp_oauth_store_client_id(env_var: String, value: String) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("client id is empty".into());
    }
    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;
    store
        .set(MCP_CLIENT_ID_SERVICE, &env_var, value.trim())
        .map_err(|e| format!("keychain write failed: {e}"))
}

fn load_stored_client_id(env_var: &str) -> Option<String> {
    let store = apollia_auth::select_secret_store().ok()?;
    store.get(MCP_CLIENT_ID_SERVICE, env_var).ok().flatten()
}

/// Build-time client id registry. `option_env!` requires a literal so each
/// known env var must be enumerated here - adding a new provider is a
/// 2-line change. End users never see this layer; it's how releases ship
/// "turnkey" credentials for AS that don't support CIMD/DCR (Figma today).
fn compile_time_known_client_id(env_var: &str) -> Option<&'static str> {
    match env_var {
        "APOLLIA_FIGMA_CLIENT_ID" => option_env!("APOLLIA_BUILD_FIGMA_CLIENT_ID"),
        _ => None,
    }
}

/// Drive the MCP HTTP OAuth flow end-to-end for `server_name`.
///
/// Opens the default browser at the authorize URL, blocks until the loopback
/// callback fires, exchanges the code for tokens (with `resource=` per
/// RFC 8707), persists the token under `(MCP_OAUTH_SERVICE, server_name)` in
/// the OS keychain, and returns the identity claims for UI display.
///
/// `scopes` controls what's requested at the AS:
/// - `None` or empty → the orchestrator uses the PRM's `scopes_supported`.
/// - non-empty → the user-selected subset from the wizard scope selector.
///
/// Idempotent: calling this for the same `server_name` overwrites any
/// previously-stored token (useful for the "Reconnect" button in settings).
#[tauri::command]
// Tauri command: the server name/url plus the OAuth discovery inputs exceed 5
// by design; they mirror the front-end reconnect payload one-to-one.
// REASON: Tauri command: each parameter is one invoke key or injected State; a struct would change the IPC contract.
#[allow(clippy::too_many_arguments)]
pub async fn mcp_oauth_login(
    app: tauri::AppHandle,
    server_name: String,
    server_url: String,
    www_authenticate: Option<String>,
    scopes: Vec<String>,
    client_id: Option<String>,
) -> Result<McpOAuthAccount, String> {
    use tauri_plugin_opener::OpenerExt;

    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;

    let scopes_opt: Option<Vec<String>> = if scopes.is_empty() {
        None
    } else {
        Some(scopes)
    };

    let client_id_override = client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let req = apollia_auth::NegotiateRequest {
        server_name: &server_name,
        server_url: &server_url,
        www_authenticate: www_authenticate.as_deref(),
        scopes: scopes_opt,
        client_id_override,
    };

    // Use Tauri's opener plugin instead of the `open` crate - the latter
    // spawns a subprocess that gets blocked by Tauri 2's webview sandbox on
    // some platforms (silent no-op). The opener plugin goes through Tauri's
    // own native integration, with explicit capability gating.
    let token = apollia_auth::negotiate_token(req, &*store, |url| {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| apollia_auth::AuthError::CallbackServer(e.to_string()))
    })
    .await
    .map_err(|e| format!("{e}"))?;

    Ok(McpOAuthAccount {
        sub: token.identity_sub,
        email: token.identity_email,
        scopes: token.scope,
    })
}
