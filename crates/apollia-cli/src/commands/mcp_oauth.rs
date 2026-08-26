//! `apollia-os mcp oauth` subcommands: interactive PKCE flow from the CLI.
//!
//! Mirrors the Desktop `mcp_oauth_login` Tauri command but drives the browser
//! via the `open` crate. The orchestrator (`apollia_auth::negotiate_token`)
//! accepts an injected `open_browser` closure precisely so the same flow can
//! run headless from a terminal: the URL is printed to stdout *and* opened in
//! the system browser, then the OAuth provider redirects to a loopback
//! listener that the orchestrator manages.
//!
//! Tokens are stored under the canonical keychain service
//! `apollia_auth::MCP_OAUTH_SERVICE` (`apollia-mcp-oauth`), so they are
//! transparently shared with the Desktop runtime (subject to OS keychain ACL
//! between the two binaries).

use std::path::PathBuf;

use clap::Subcommand;

use apollia_auth::{
    delete_mcp_token, load_mcp_token, negotiate_token, parse_www_authenticate, select_secret_store,
    AuthError, McpDiscoveryClient, McpOAuthError, NegotiateRequest,
};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Subcommands of `apollia-os mcp oauth`.
#[derive(Debug, Subcommand)]
pub enum McpOauthCommand {
    /// Run the interactive PKCE login flow for `<server>` and persist the token.
    ///
    /// Opens the OAuth authorisation URL in the system browser, prints it on
    /// stdout (so headless setups can copy / paste it), waits for the
    /// authorisation server to redirect back to the loopback listener, then
    /// stores the resulting access + refresh tokens in the OS keychain under
    /// `apollia-mcp-oauth/<server>`.
    Login {
        /// Server name as declared in `mcp.db` (matches the Desktop wizard).
        server: String,
        /// Optional comma-separated scope list. Omit to defer to the AS's
        /// `scopes_supported` (recommended).
        #[arg(long, value_delimiter = ',', value_name = "SCOPE")]
        scopes: Vec<String>,
        /// Override the OAuth client id resolution (for tenants running their
        /// own AS app, usually unnecessary).
        #[arg(long, value_name = "ID")]
        client_id: Option<String>,
        /// Override the path to `mcp.db` (default: `~/.apollia/mcp.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Report the persisted-token status for one or every configured server.
    ///
    /// Surfaces token expiry, granted scopes, and identity claims (`sub`,
    /// `email`) without revealing the access token itself.
    Status {
        /// Optional server name. When omitted, lists every server with a
        /// stored token plus those declared in `mcp.db` but unauthenticated.
        server: Option<String>,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Delete the persisted token for `<server>` from the OS keychain.
    ///
    /// The authorisation server is **not** notified: call the provider's
    /// revocation endpoint manually if a server-side revocation is required.
    Logout {
        /// Server name to forget.
        server: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },

    /// Manage per-env-var OAuth client-id overrides stored in the OS keychain.
    ///
    /// Mirrors the Desktop Settings → MCP → "OAuth client id" panel.
    /// Resolution chain: env var > keychain (this command) > build-time default.
    #[command(name = "client-id", subcommand)]
    ClientId(McpClientIdCommand),

    /// Run RFC 9728 + RFC 8414 OAuth discovery against `<server>` and print
    /// the resulting authorization server, scopes and endpoints.
    ///
    /// Read-only: no token is exchanged and no secret is written. Useful to
    /// confirm an HTTP MCP server's PRM + AS metadata before invoking `login`.
    Discover {
        /// Server name as declared in `mcp.db`.
        server: String,
        /// Override the path to `mcp.db` (default: `~/.apollia/mcp.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
}

/// Subcommands of `apollia-os mcp oauth client-id`.
#[derive(Debug, Subcommand)]
pub enum McpClientIdCommand {
    /// Persist `<value>` as the OAuth client id for the env var `<env_var>`.
    ///
    /// The env var is the same one the connector wizard surfaces (e.g.
    /// `APOLLIA_FIGMA_CLIENT_ID`). Pass an empty value to fail validation;
    /// use `clear` to remove.
    Set {
        /// Env var name (e.g. `APOLLIA_FIGMA_CLIENT_ID`).
        env_var: String,
        /// New client id value.
        value: String,
    },

    /// Remove the persisted client id stored under `<env_var>`.
    Clear {
        /// Env var name.
        env_var: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Same keychain service name the Desktop uses for OAuth client-id overrides.
/// Mirrors `MCP_CLIENT_ID_SERVICE` in `apollia-desktop/src/commands/mcp.rs`
/// so the two binaries share entries on the host keychain.
const MCP_CLIENT_ID_SERVICE: &str = "apollia-mcp-client-ids";

/// Entry point for `apollia-os mcp oauth <verb>`.
pub async fn run(cmd: &McpOauthCommand, json: bool) -> i32 {
    match cmd {
        McpOauthCommand::Login {
            server,
            scopes,
            client_id,
            db,
        } => run_login(server, scopes, client_id.as_deref(), db.as_deref(), json).await,
        McpOauthCommand::Status { server, db } => {
            run_status(server.as_deref(), db.as_deref(), json).await
        }
        McpOauthCommand::Logout { server, confirm } => run_logout(server, *confirm, json),
        McpOauthCommand::ClientId(sub) => run_client_id(sub, json),
        McpOauthCommand::Discover { server, db } => run_discover(server, db.as_deref(), json).await,
    }
}

fn emit_error(msg: impl Into<String>, json: bool) -> i32 {
    let s = msg.into();
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &s.to_string())
}

/// Outcome of the post-login runtime reconnect attempt. Recorded so the
/// human-readable output can give precise next-step guidance instead of a
/// generic "token stored" without telling the operator whether the server is
/// actually live.
#[derive(Debug)]
enum ReconnectOutcome {
    /// Runtime answered, server is now connected.
    Connected,
    /// Runtime not running: token is stored for the next boot.
    RuntimeOffline,
    /// Runtime answered but the reconnect attempt failed (parsed error in field).
    Failed(String),
    /// Runtime answered but said the server is unknown (404 fallback).
    Skipped,
}

/// Hit `POST /api/v1/mcp/servers/<name>/restart` so the runtime picks up the
/// freshly stored token. The route falls back to `add_server` when no session
/// was ever started (the normal post-OAuth case for a server that boot-failed),
/// so a single call covers both "session exists" and "first connection" paths.
async fn reconnect_runtime_session(server: &str) -> ReconnectOutcome {
    let client = RuntimeClient::new(default_socket_path());
    let uri = format!("/api/v1/mcp/servers/{server}/restart");
    match client.post(&uri, None).await {
        Ok(resp) if resp.status < 400 => ReconnectOutcome::Connected,
        Ok(resp) if resp.status == 404 => ReconnectOutcome::Skipped,
        Ok(resp) => ReconnectOutcome::Failed(format!("HTTP {}: {}", resp.status, resp.body)),
        Err(ClientError::ConnectionRefused) => ReconnectOutcome::RuntimeOffline,
        Err(e) => ReconnectOutcome::Failed(e.to_string()),
    }
}

impl serde::Serialize for ReconnectOutcome {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            ReconnectOutcome::Connected => ser.serialize_str("connected"),
            ReconnectOutcome::RuntimeOffline => ser.serialize_str("runtime_offline"),
            ReconnectOutcome::Skipped => ser.serialize_str("skipped"),
            ReconnectOutcome::Failed(reason) => {
                use serde::ser::SerializeStruct;
                let mut s = ser.serialize_struct("ReconnectOutcome", 2)?;
                s.serialize_field("status", "failed")?;
                s.serialize_field("reason", reason)?;
                s.end()
            }
        }
    }
}

/// Probe `server_url` to capture the `WWW-Authenticate` header advertised
/// when the server requires OAuth. Returns `Ok(Some(header))` when the server
/// answered 401 with the header, `Ok(None)` when it answered anything else,
/// and `Err(...)` only when the request itself never reached the server.
///
/// Required for hosts whose Protected Resource Metadata lives behind the
/// MCP endpoint (Notion, Linear) rather than at the well-known origin path.
async fn probe_www_authenticate(server_url: &str) -> Result<Option<String>, reqwest::Error> {
    // The MCP server URL is operator-declared and often loopback, so the
    // public-destination policy would refuse the normal configuration.
    let client = apollia_core::net::configured_endpoint_client_builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // Use a minimal POST so the server's auth middleware engages even when GET
    // is unauthenticated; many MCP HTTP transports answer 200 on GET (health
    // probe) but 401 on POST. The body is intentionally minimal, we only
    // need the response headers.
    let resp = client
        .post(server_url)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await?;
    if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }
    Ok(resp
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string()))
}

/// Resolve `~/.apollia/mcp.db` or honour an explicit override.
fn resolve_mcp_db(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .map(apollia_core::paths::data_dir_under)
        .unwrap_or_else(|| PathBuf::from(apollia_core::paths::DATA_DIR_NAME))
        .join(apollia_core::paths::DataFile::Mcp.file_name())
}

/// Look up the URL stored for `server` in `mcp.db`. The orchestrator needs it
/// to drive resource-metadata discovery; failing fast here yields a far
/// clearer error than a cryptic `fetch_prm` failure deep in the auth crate.
fn load_server_url(db_override: Option<&std::path::Path>, server: &str) -> Result<String, String> {
    let path = resolve_mcp_db(db_override);
    if !path.exists() {
        return Err(format!(
            "mcp database not found at {} (Desktop or `apollia-os mcp add` must have run first)",
            path.display()
        ));
    }
    let repo = apollia_mcp::McpServerRepository::open(&path)
        .map_err(|e| format!("open {} failed: {e}", path.display()))?;
    let entry = repo
        .list()
        .map_err(|e| format!("read mcp servers failed: {e}"))?
        .into_iter()
        .find(|s| s.name == server)
        .ok_or_else(|| format!("server '{server}' not configured in {}", path.display()))?;
    entry.url.clone().filter(|u| !u.is_empty()).ok_or_else(|| {
        format!(
            "server '{server}' has no URL - only HTTP/streamable-http MCP servers \
                 require OAuth (stdio servers run a local subprocess)"
        )
    })
}

// ─── login ────────────────────────────────────────────────────────────────────

async fn run_login(
    server: &str,
    scopes: &[String],
    client_id: Option<&str>,
    db_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let server_url = match load_server_url(db_override, server) {
        Ok(u) => u,
        Err(e) => return emit_error(e, json),
    };

    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };

    let scopes_opt: Option<Vec<String>> = if scopes.is_empty() {
        None
    } else {
        Some(scopes.to_vec())
    };
    let client_id_override = client_id.map(str::trim).filter(|s| !s.is_empty());

    // Probe the server first to capture the WWW-Authenticate header. Many MCP
    // hosts (Notion, Linear, …) hide their Protected Resource Metadata behind
    // an authenticated path and only advertise it via the 401 challenge, so
    // letting `negotiate_token` discover PRM blindly fails with "PRM fetch
    // returned 401". The probe is a cheap unauthenticated request: when the
    // server returns 401 we read the header verbatim; on 200 / other we leave
    // it as None and let the orchestrator's origin fallback do the work.
    let www_authenticate = match probe_www_authenticate(&server_url).await {
        Ok(Some(header)) => {
            tracing::debug!(server = %server, "mcp.oauth.probe.challenged");
            Some(header)
        }
        Ok(None) => {
            tracing::debug!(server = %server, "mcp.oauth.probe.unchallenged");
            None
        }
        Err(e) => {
            // Surface this clearly on stderr because the orchestrator's
            // origin fallback often points at an auth-gated 401, leaving
            // the operator wondering why a perfectly reachable server
            // refuses to authorise. The probe error is the real cause.
            eprintln!(
                "  ! probe of {server_url} failed: {e}\n    OAuth dance will use origin-based PRM discovery; may fail for hosts whose PRM lives behind /mcp."
            );
            None
        }
    };

    let req = NegotiateRequest {
        server_name: server,
        server_url: &server_url,
        www_authenticate: www_authenticate.as_deref(),
        scopes: scopes_opt,
        client_id_override,
    };

    if !json {
        eprintln!(
            "Starting MCP OAuth flow for '{server}' against {server_url}.\n  \
             A browser will open with the authorisation URL - log in there to complete the flow."
        );
    }

    let result = negotiate_token(req, &*store, |url| {
        // Print + open. Printing first guarantees the URL is reachable in
        // truly headless contexts (SSH, CI) where the `open` crate is a
        // no-op or fails silently.
        if !json {
            println!("\n  -> open in browser:\n     {url}\n");
        }
        match open::that(url) {
            Ok(()) => Ok(()),
            Err(e) => Err(AuthError::CallbackServer(format!(
                "failed to open browser ({e}); copy the URL above manually"
            ))),
        }
    })
    .await;

    match result {
        Ok(token) => emit_login_success(server, &token, json).await,
        Err(McpOAuthError::ReauthRequired { .. }) => emit_error(
            "the authorisation server rejected the refresh - stored token deleted, re-run login",
            json,
        ),
        Err(e) => emit_error(format!("OAuth flow failed: {e}"), json),
    }
}

/// Report a successful login and trigger an immediate runtime reconnect.
async fn emit_login_success(server: &str, token: &apollia_auth::StoredMcpToken, json: bool) -> i32 {
    if json {
        let reconnect = reconnect_runtime_session(server).await;
        let body = serde_json::json!({
            "server": server,
            "stored": true,
            "scopes": token.scope,
            "identity": {
                "sub": token.identity_sub,
                "email": token.identity_email,
            },
            "expires_at": token.expires_at,
            "runtime_reconnect": reconnect,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }

    println!("  * OAuth token stored for '{server}'");
    if let Some(email) = &token.identity_email {
        println!("    identity : {email}");
    } else if let Some(sub) = &token.identity_sub {
        println!("    identity : {sub}");
    }
    if !token.scope.is_empty() {
        println!("    scopes   : {}", token.scope.join(", "));
    }
    if let Some(exp) = token.expires_at {
        println!("    expires  : unix {exp}");
    }
    // Triggers an immediate runtime reconnect: at boot the server
    // failed because no token was stored yet, so without this the
    // operator has to also stop+start the daemon manually.
    match reconnect_runtime_session(server).await {
        ReconnectOutcome::Connected => {
            println!("  * runtime reconnected '{server}' (new token applied)");
        }
        ReconnectOutcome::RuntimeOffline => {
            println!(
                "  ! daemon offline - start it with `apollia-os start` and the server will pick up the token."
            );
        }
        ReconnectOutcome::Failed(reason) => {
            println!(
                "  ! token stored but runtime reconnect failed: {reason}\n    Retry with `apollia-os mcp restart {server}`."
            );
        }
        ReconnectOutcome::Skipped => {
            // Daemon present but the server was unknown to the
            // repo as well, extremely unlikely once we reached
            // this point because load_server_url succeeded.
            println!("  ! token stored; could not reconnect (server not in runtime repo).");
        }
    }
    exit_codes::SUCCESS
}

// ─── status ───────────────────────────────────────────────────────────────────

async fn run_status(
    server: Option<&str>,
    db_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };

    let servers = match server {
        Some(name) => vec![name.to_string()],
        None => match enumerate_status_servers(db_override, json) {
            Ok(names) => names,
            Err(code) => return code,
        },
    };

    let mut report: Vec<serde_json::Value> = Vec::with_capacity(servers.len());
    for name in &servers {
        report.push(status_entry(&*store, name));
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else if report.is_empty() {
        println!("  (no servers found in mcp.db)");
    } else {
        print_status_table(&report);
    }
    exit_codes::SUCCESS
}

/// Enumerate every server declared in `mcp.db`.
///
/// Returns `Err(exit_code)` when the caller should return early (mcp.db
/// absent prints an empty report and exits SUCCESS; an open failure emits an
/// error and exits with `GENERAL_ERROR`).
fn enumerate_status_servers(
    db_override: Option<&std::path::Path>,
    json: bool,
) -> Result<Vec<String>, i32> {
    // Enumerate every server declared in mcp.db so unauthenticated
    // entries appear too, not just those with a stored token.
    let path = resolve_mcp_db(db_override);
    if !path.exists() {
        if json {
            println!("[]");
        } else {
            println!("  (mcp.db absent - no configured servers)");
        }
        return Err(exit_codes::SUCCESS);
    }
    match apollia_mcp::McpServerRepository::open(&path) {
        Ok(repo) => Ok(repo
            .list()
            .map(|rows| rows.into_iter().map(|r| r.name).collect::<Vec<_>>())
            .unwrap_or_default()),
        Err(e) => Err(emit_error(
            format!("open {} failed: {e}", path.display()),
            json,
        )),
    }
}

/// Build the JSON status entry for a single server.
fn status_entry(store: &dyn apollia_auth::SecretStore, name: &str) -> serde_json::Value {
    match load_mcp_token(store, name) {
        Ok(Some(token)) => serde_json::json!({
            "server": name,
            "stored": true,
            "scopes": token.scope,
            "expires_at": token.expires_at,
            "identity": {
                "sub": token.identity_sub,
                "email": token.identity_email,
            },
        }),
        Ok(None) => serde_json::json!({
            "server": name,
            "stored": false,
        }),
        Err(e) => serde_json::json!({
            "server": name,
            "stored": false,
            "error": e.to_string(),
        }),
    }
}

/// Print the status report as a human-readable table.
fn print_status_table(report: &[serde_json::Value]) {
    println!(
        "  {:<24} {:<10} {:<30} IDENTITY",
        "SERVER", "STORED", "SCOPES"
    );
    for entry in report {
        let s = entry.get("server").and_then(|v| v.as_str()).unwrap_or("?");
        let stored = entry
            .get("stored")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let stored_glyph = if stored { "*" } else { "-" };
        let scopes = entry
            .get("scopes")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let identity = entry
            .get("identity")
            .and_then(|v| v.get("email").or_else(|| v.get("sub")))
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let scopes_truncated = crate::commands::truncate_for_display(&scopes, 28, 27, "…");
        println!("  {s:<24} {stored_glyph:<10} {scopes_truncated:<30} {identity}");
    }
}

// ─── logout ───────────────────────────────────────────────────────────────────

fn run_logout(server: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        return emit_error(
            format!(
                "use --confirm to delete the stored token for '{server}'. \
                 The authorisation server is not notified."
            ),
            json,
        );
    }
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };
    match delete_mcp_token(&*store, server) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({"server": server, "deleted": true});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * token removed for '{server}'");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_error(format!("delete failed: {e}"), json),
    }
}

// ─── client-id ───────────────────────────────────────────────────────────────

fn run_client_id(cmd: &McpClientIdCommand, json: bool) -> i32 {
    match cmd {
        McpClientIdCommand::Set { env_var, value } => run_client_id_set(env_var, value, json),
        McpClientIdCommand::Clear { env_var, confirm } => {
            run_client_id_clear(env_var, *confirm, json)
        }
    }
}

fn run_client_id_set(env_var: &str, value: &str, json: bool) -> i32 {
    let trimmed_env = env_var.trim();
    if trimmed_env.is_empty() {
        return emit_error("env_var must not be empty", json);
    }
    let trimmed_value = value.trim();
    if trimmed_value.is_empty() {
        return emit_error(
            "value must not be empty (use `mcp oauth client-id clear <env_var>` to remove)",
            json,
        );
    }
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };
    match store.set(MCP_CLIENT_ID_SERVICE, trimmed_env, trimmed_value) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "env_var": trimmed_env,
                    "stored": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * client id stored under {trimmed_env}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_error(format!("keychain write failed: {e}"), json),
    }
}

fn run_client_id_clear(env_var: &str, confirm: bool, json: bool) -> i32 {
    let trimmed_env = env_var.trim();
    if trimmed_env.is_empty() {
        return emit_error("env_var must not be empty", json);
    }
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("clear the stored OAuth client id '{trimmed_env}'"),
    ) {
        return code;
    }
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };
    match store.delete(MCP_CLIENT_ID_SERVICE, trimmed_env) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "env_var": trimmed_env,
                    "cleared": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * client id cleared for {trimmed_env}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_error(format!("keychain delete failed: {e}"), json),
    }
}

// ─── discover ─────────────────────────────────────────────────────────────────

async fn run_discover(server: &str, db_override: Option<&std::path::Path>, json: bool) -> i32 {
    let server_url = match load_server_url(db_override, server) {
        Ok(u) => u,
        Err(e) => return emit_error(e, json),
    };

    let www_authenticate = probe_www_authenticate(&server_url).await.unwrap_or(None);

    let discovery = match McpDiscoveryClient::new() {
        Ok(c) => c,
        Err(e) => return emit_error(format!("init discovery client: {e}"), json),
    };

    let prm_result = match www_authenticate
        .as_deref()
        .and_then(parse_www_authenticate)
        .and_then(|wa| wa.resource_metadata)
    {
        Some(prm_url) => discovery.fetch_prm_at(&prm_url).await,
        None => discovery.fetch_prm(&server_url).await,
    };
    let prm = match prm_result {
        Ok(p) => p,
        Err(e) => return emit_error(format!("fetch PRM: {e}"), json),
    };

    let as_url = match prm.authorization_servers.first().cloned() {
        Some(u) => u,
        None => {
            return emit_error("PRM declared no authorization servers", json);
        }
    };

    let as_metadata = match discovery.fetch_as_metadata(&as_url).await {
        Ok(m) => m,
        Err(e) => return emit_error(format!("fetch AS metadata: {e}"), json),
    };

    let supports_pkce = as_metadata.supports_pkce_s256();

    let ctx = DiscoverReport {
        server,
        server_url: &server_url,
        www_authenticate_present: www_authenticate.is_some(),
        as_url: &as_url,
        supports_pkce,
        prm: &prm,
        as_metadata: &as_metadata,
    };
    if json {
        print_discover_json(&ctx);
    } else {
        print_discover_human(&ctx);
    }

    if !supports_pkce {
        // Distinct exit code communicates that the *server* fails the policy
        // even though discovery itself succeeded, useful in scripts that
        // gate `mcp oauth login` on this report.
        return exit_codes::TASK_FAILED;
    }
    exit_codes::SUCCESS
}

/// Field set rendered by the `discover` subcommand reporters.
struct DiscoverReport<'a> {
    server: &'a str,
    server_url: &'a str,
    www_authenticate_present: bool,
    as_url: &'a str,
    supports_pkce: bool,
    prm: &'a apollia_auth::ProtectedResourceMetadata,
    as_metadata: &'a apollia_auth::AuthorizationServerMetadata,
}

fn print_discover_json(ctx: &DiscoverReport<'_>) {
    let body = serde_json::json!({
        "server": ctx.server,
        "server_url": ctx.server_url,
        "www_authenticate_present": ctx.www_authenticate_present,
        "authorization_server": ctx.as_url,
        "supports_pkce_s256": ctx.supports_pkce,
        "scopes_supported": ctx.prm.scopes_supported,
        "as_authorization_endpoint": ctx.as_metadata.authorization_endpoint,
        "as_token_endpoint": ctx.as_metadata.token_endpoint,
        "as_registration_endpoint": ctx.as_metadata.registration_endpoint,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&body).unwrap_or_default()
    );
}

fn print_discover_human(ctx: &DiscoverReport<'_>) {
    note!("  Discovery report for '{}':", ctx.server);
    println!("    server URL              : {}", ctx.server_url);
    println!(
        "    WWW-Authenticate probe  : {}",
        if ctx.www_authenticate_present {
            "captured"
        } else {
            "not present"
        }
    );
    println!("    authorization server    : {}", ctx.as_url);
    println!(
        "    supports PKCE S256      : {}",
        if ctx.supports_pkce { "yes" } else { "NO" }
    );
    if !ctx.prm.scopes_supported.is_empty() {
        note!("    PRM scopes_supported    :");
        for s in &ctx.prm.scopes_supported {
            println!("      - {s}");
        }
    }
    println!(
        "    authorization endpoint  : {}",
        ctx.as_metadata.authorization_endpoint
    );
    println!(
        "    token endpoint          : {}",
        ctx.as_metadata.token_endpoint
    );
    if let Some(reg) = &ctx.as_metadata.registration_endpoint {
        println!("    registration endpoint   : {reg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: McpOauthCommand,
    }

    #[test]
    fn parses_login_minimal() {
        let cli = TestCli::parse_from(["x", "login", "notion"]);
        match cli.cmd {
            McpOauthCommand::Login {
                server,
                scopes,
                client_id,
                ..
            } => {
                assert_eq!(server, "notion");
                assert!(scopes.is_empty());
                assert!(client_id.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_login_with_scopes() {
        let cli = TestCli::parse_from(["x", "login", "notion", "--scopes", "read,write,admin"]);
        match cli.cmd {
            McpOauthCommand::Login { scopes, .. } => {
                assert_eq!(scopes, vec!["read", "write", "admin"]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_status_optional_server() {
        let cli_all = TestCli::parse_from(["x", "status"]);
        let cli_one = TestCli::parse_from(["x", "status", "notion"]);
        assert!(matches!(
            cli_all.cmd,
            McpOauthCommand::Status { server: None, .. }
        ));
        match cli_one.cmd {
            McpOauthCommand::Status { server, .. } => {
                assert_eq!(server.as_deref(), Some("notion"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_logout_requires_confirm_flag() {
        let cli = TestCli::parse_from(["x", "logout", "notion"]);
        match cli.cmd {
            McpOauthCommand::Logout { server, confirm } => {
                assert_eq!(server, "notion");
                assert!(!confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_logout_with_confirm() {
        let cli = TestCli::parse_from(["x", "logout", "notion", "--confirm"]);
        match cli.cmd {
            McpOauthCommand::Logout { confirm, .. } => assert!(confirm),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn logout_without_confirm_errors() {
        let code = run_logout("notion", false, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn resolve_mcp_db_honours_override() {
        let p = std::path::Path::new("/tmp/custom-mcp.db");
        assert_eq!(resolve_mcp_db(Some(p)), PathBuf::from("/tmp/custom-mcp.db"));
    }

    #[test]
    fn resolve_mcp_db_default_ends_with_canonical_filename() {
        let p = resolve_mcp_db(None);
        assert!(
            p.to_string_lossy().ends_with(".apollia/mcp.db"),
            "unexpected default path: {p:?}"
        );
    }

    #[test]
    fn parses_client_id_set() {
        let cli = TestCli::parse_from([
            "x",
            "client-id",
            "set",
            "APOLLIA_FIGMA_CLIENT_ID",
            "figma-abc123",
        ]);
        match cli.cmd {
            McpOauthCommand::ClientId(McpClientIdCommand::Set { env_var, value }) => {
                assert_eq!(env_var, "APOLLIA_FIGMA_CLIENT_ID");
                assert_eq!(value, "figma-abc123");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_client_id_clear() {
        let cli = TestCli::parse_from(["x", "client-id", "clear", "APOLLIA_FIGMA_CLIENT_ID"]);
        match cli.cmd {
            McpOauthCommand::ClientId(McpClientIdCommand::Clear { env_var, confirm }) => {
                assert_eq!(env_var, "APOLLIA_FIGMA_CLIENT_ID");
                assert!(!confirm, "the confirmation is opt-in, never the default");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_discover() {
        let cli = TestCli::parse_from(["x", "discover", "notion"]);
        match cli.cmd {
            McpOauthCommand::Discover { server, db } => {
                assert_eq!(server, "notion");
                assert!(db.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_discover_with_db_override() {
        let cli = TestCli::parse_from(["x", "discover", "notion", "--db", "/tmp/custom.db"]);
        match cli.cmd {
            McpOauthCommand::Discover { server, db } => {
                assert_eq!(server, "notion");
                assert_eq!(db, Some(std::path::PathBuf::from("/tmp/custom.db")));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn client_id_set_rejects_empty_env_var() {
        let code = run_client_id_set("", "v", true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn client_id_set_rejects_empty_value() {
        let code = run_client_id_set("APOLLIA_FIGMA_CLIENT_ID", "  ", true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }
}
