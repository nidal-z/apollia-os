//! The OAuth credentials a connector needs before a browser window opens:
//! client id, client secret, API key, and the Drive folder a Google account
//! writes to. Reads and writes `~/.apollia/oauth-clients.toml` through
//! `apollia_auth::oauth_clients_file`, and reports which layer of the
//! resolution chain each value came from.

use apollia_auth::ConnectorProvider;
use serde::Serialize;

use super::{auth_manager, provider_from_id, IntegrationsError};

// ─── OAuth client_id overrides (~/.apollia/oauth-clients.toml) ──────────────

/// One row of `oauth_list_client_ids`.
///
/// Reports the **effective** client id used by the resolution chain
/// (`env var → override file → compiled default`) so the UI can show the
/// user what is currently active per provider, and whether the override
/// file contains a per-provider entry. Also reports whether a client_secret
/// is configured (Google Installed App needs one, Microsoft does not).
#[derive(Debug, Clone, Serialize)]
pub struct OauthClientIdStatus {
    /// Provider id (`"google"` / `"microsoft"`).
    pub provider: String,
    /// Effective client id used at runtime. Empty string when no source is configured.
    pub effective_client_id: String,
    /// Source that produced the effective value: `"env"`, `"file"`, `"builtin"`, or `"none"`.
    pub source: String,
    /// Override stored in `~/.apollia/oauth-clients.toml`, if any.
    pub override_client_id: Option<String>,
    /// True when a client_secret is resolved through any source. Never
    /// returns the secret value itself; the UI only renders a presence dot.
    pub has_client_secret: bool,
    /// Same source semantics as `source` but for the secret.
    pub client_secret_source: String,
    /// True when this provider requires a client_secret to function. Hint for
    /// the UI to surface a "secret missing" warning on Google but not on
    /// Microsoft where it's intentional.
    pub requires_client_secret: bool,
    /// True when an API key is resolved through any source. Google Picker
    /// needs one; Microsoft entries leave this false.
    pub has_api_key: bool,
    /// Source for the resolved API key. Same semantics as `source`.
    pub api_key_source: String,
    /// True when this provider needs an API key (Google for Picker only).
    pub requires_api_key: bool,
}

fn detect_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_id_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_id(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_id().is_empty() {
        return "builtin";
    }
    "none"
}

fn detect_secret_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_secret_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_secret(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_secret().is_empty() {
        return "builtin";
    }
    "none"
}

fn detect_api_key_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.api_key_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_api_key(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_api_key().is_empty() {
        return "builtin";
    }
    "none"
}

const fn provider_requires_secret(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

/// Refuse an OAuth handshake whose credentials are incomplete, before any
/// browser window opens.
///
/// Checking the client id alone is not enough: a Google client id without its
/// paired secret passes every local check, opens the consent screen, and fails
/// only at the token endpoint, after the user has already granted access. The
/// resulting `invalid_client` names nothing the operator can act on.
///
/// Pure on purpose. The callers resolve the three-source chain
/// (`ConnectorProvider::resolve_*`) and hand the outcome in, which keeps this
/// testable without touching process environment.
fn credential_gate(
    provider: ConnectorProvider,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Result<(), IntegrationsError> {
    let has_value = |v: Option<&str>| v.is_some_and(|s| !s.trim().is_empty());

    if !has_value(client_id) {
        return Err(IntegrationsError::OauthClientNotConfigured(
            provider.id().to_owned(),
        ));
    }
    if provider_requires_secret(provider) && !has_value(client_secret) {
        return Err(IntegrationsError::OauthClientSecretMissing(
            provider.id().to_owned(),
        ));
    }
    Ok(())
}

/// Resolve a provider's credentials and run them through [`credential_gate`].
pub(super) fn ensure_credentials(provider: ConnectorProvider) -> Result<(), IntegrationsError> {
    let client_id = provider.resolve_client_id();
    let client_secret = provider.resolve_client_secret();
    credential_gate(provider, client_id.as_deref(), client_secret.as_deref())
}

const fn provider_requires_api_key(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

/// Snapshot the current OAuth client_id configuration per provider.
///
/// Used by the Settings → Integrations OAuth panel to render which client id
/// is active and where it came from. Safe to call without a sovereignty check
/// since the response contains no token material, only the public client id.
#[tauri::command]
pub async fn oauth_list_client_ids() -> Result<Vec<OauthClientIdStatus>, IntegrationsError> {
    let mut out = Vec::new();
    for provider in [ConnectorProvider::Google, ConnectorProvider::Microsoft] {
        let effective = provider.resolve_client_id().unwrap_or_default();
        let source = detect_source(provider).to_string();
        let override_value = apollia_auth::oauth_clients_file::lookup_client_id(provider.id());
        let secret_source = detect_secret_source(provider).to_string();
        let has_secret = provider.resolve_client_secret().is_some();
        let api_key_source = detect_api_key_source(provider).to_string();
        let has_api_key = provider.resolve_api_key().is_some();
        out.push(OauthClientIdStatus {
            provider: provider.id().to_owned(),
            effective_client_id: effective,
            source,
            override_client_id: override_value,
            has_client_secret: has_secret,
            client_secret_source: secret_source,
            requires_client_secret: provider_requires_secret(provider),
            has_api_key,
            api_key_source,
            requires_api_key: provider_requires_api_key(provider),
        });
    }
    Ok(out)
}

/// Result of `oauth_test_client`: a lightweight, non-interactive check of a
/// provider's configured OAuth client credentials.
///
/// A full validation (that the authorization server actually accepts the
/// client) requires an interactive user consent + token exchange, which cannot
/// run headless. This is the strongest safe check: credentials present +
/// well-formed + the provider's authorization server reachable.
#[derive(Debug, Clone, Serialize)]
pub struct OauthClientTestResult {
    /// True when every check passed.
    pub ok: bool,
    /// Human-readable summary: what passed, or the first failing check.
    pub detail: String,
}

/// The provider's OIDC discovery document, reachable without credentials. A
/// successful GET proves the authorization server is up and DNS/TLS resolve.
const fn provider_discovery_url(provider: ConnectorProvider) -> &'static str {
    match provider {
        ConnectorProvider::Google => "https://accounts.google.com/.well-known/openid-configuration",
        ConnectorProvider::Microsoft => {
            "https://login.microsoftonline.com/common/v2.0/.well-known/openid-configuration"
        }
    }
}

/// Returns `Some(reason)` when `client_id` is malformed for `provider`.
fn client_id_shape_error(provider: ConnectorProvider, client_id: &str) -> Option<String> {
    match provider {
        ConnectorProvider::Google => {
            if client_id.ends_with(".apps.googleusercontent.com") {
                None
            } else {
                Some("Google client id should end with .apps.googleusercontent.com".to_string())
            }
        }
        ConnectorProvider::Microsoft => {
            if is_guid_like(client_id) {
                None
            } else {
                Some("Microsoft client id should be a GUID (Azure application id)".to_string())
            }
        }
    }
}

/// Whether `s` looks like a canonical GUID (`8-4-4-4-12` hex groups).
fn is_guid_like(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups.iter().map(|g| g.len()).eq([8usize, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Validate a provider's configured OAuth client credentials non-interactively.
///
/// Runs three cheap checks in order and stops at the first failure:
/// 1. an effective client id is configured (env / file / build-time chain);
/// 2. the client id is well-formed for the provider;
/// 3. for providers that need a client secret (Google), one is configured;
/// 4. the provider's OIDC discovery endpoint is reachable over the network.
///
/// Returns a typed `{ ok, detail }`. Safe to call without a sovereignty check:
/// it inspects only local credential *presence* (never their values) and hits a
/// public, credential-less discovery URL.
#[tauri::command]
pub async fn oauth_test_client(
    provider: String,
) -> Result<OauthClientTestResult, IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;

    let client_id = match provider_id.resolve_client_id() {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            return Ok(OauthClientTestResult {
                ok: false,
                detail: "OAuth client id is not configured".to_string(),
            })
        }
    };

    if let Some(reason) = client_id_shape_error(provider_id, &client_id) {
        return Ok(OauthClientTestResult {
            ok: false,
            detail: reason,
        });
    }

    if provider_requires_secret(provider_id) && provider_id.resolve_client_secret().is_none() {
        return Ok(OauthClientTestResult {
            ok: false,
            detail: "client secret is required for this provider but not configured".to_string(),
        });
    }

    let client = match apollia_core::net::safe_client() {
        Ok(client) => client,
        Err(e) => {
            return Ok(OauthClientTestResult {
                ok: false,
                detail: format!("failed to build the HTTP client: {e}"),
            })
        }
    };
    match client.get(provider_discovery_url(provider_id)).send().await {
        Ok(resp) if resp.status().is_success() => Ok(OauthClientTestResult {
            ok: true,
            detail: format!(
                "client id present and well-formed; {} authorization server reachable",
                provider_id.id()
            ),
        }),
        Ok(resp) => Ok(OauthClientTestResult {
            ok: false,
            detail: format!("authorization server returned HTTP {}", resp.status()),
        }),
        Err(e) => Ok(OauthClientTestResult {
            ok: false,
            detail: format!("authorization server unreachable: {e}"),
        }),
    }
}

/// Write the client_id override for `provider` into `~/.apollia/oauth-clients.toml`.
///
/// Passing an empty string clears the override (the resolution chain then falls
/// back to env var or compiled default). The file is created on demand with
/// 0o600 permissions on Unix.
#[tauri::command]
pub async fn oauth_set_client_id(
    provider: String,
    client_id: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_client_id(provider_id.id(), client_id.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

// ─── Drive folder preferences (~/.apollia/drive-prefs.toml) ─────────────────

/// One row of `oauth_list_drive_folders`. Lists the user-configured Drive
/// root path per Google account, plus the effective fallback so the UI can
/// distinguish "explicit override" from "default Apollia folder".
#[derive(Debug, Clone, Serialize)]
pub struct DriveFolderStatus {
    /// Account id (typically the user's Google email).
    pub account_id: String,
    /// Explicit user-set path, when present (`None` means falling back to default).
    pub folder_path: Option<String>,
    /// Effective folder path used at runtime (override or default).
    pub effective_folder_path: String,
}

/// List the per-account Drive folder configuration for every connected
/// Google account. Used by the Settings → Integrations Google card so the
/// user can review/edit each account's folder.
#[tauri::command]
pub async fn oauth_list_drive_folders() -> Result<Vec<DriveFolderStatus>, IntegrationsError> {
    let manager = auth_manager().await?;
    let accounts = manager
        .list_accounts(ConnectorProvider::Google)
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    let mut out = Vec::with_capacity(accounts.len());
    for account in accounts {
        let override_path =
            apollia_auth::drive_prefs::lookup_folder_path("google", account.as_str());
        let effective =
            apollia_auth::drive_prefs::effective_folder_path("google", account.as_str());
        out.push(DriveFolderStatus {
            account_id: account.0,
            folder_path: override_path,
            effective_folder_path: effective,
        });
    }
    Ok(out)
}

/// Persist the Drive folder path for one Google account.
///
/// Tri-state semantics (mirrors `drive_prefs::AccountDrivePref::folder_path`):
/// - non-empty path → walk `Documents/.../X`, nest `<agent_slug>` inside.
/// - empty string → user *explicitly* chose My Drive root; agents create
///   `<agent_slug>` directly under root, no `Apollia` intermediate.
/// - to fall back to the default behaviour (legacy `Apollia/<slug>`), call
///   [`oauth_reset_drive_folder`] instead.
///
/// The path is sanitised (leading/trailing slashes stripped, repeated `/`
/// collapsed) before being written.
#[tauri::command]
pub async fn oauth_set_drive_folder(
    account_id: String,
    folder_path: String,
) -> Result<(), IntegrationsError> {
    apollia_auth::drive_prefs::set_folder_path("google", account_id.trim(), folder_path.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Clear the Drive folder override for one Google account, restoring the
/// default `Apollia/<agent_slug>` placement. Different from
/// [`oauth_set_drive_folder`] called with an empty string; that means
/// "use Drive root". Picked folders (Google Picker) are preserved.
#[tauri::command]
pub async fn oauth_reset_drive_folder(account_id: String) -> Result<(), IntegrationsError> {
    apollia_auth::drive_prefs::reset_folder_path("google", account_id.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Persist the public Google API key used by Google Picker. Stored
/// alongside the client_id / client_secret in `~/.apollia/oauth-clients.toml`.
/// Empty string clears the override.
#[tauri::command]
pub async fn oauth_set_api_key(provider: String, api_key: String) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_api_key(provider_id.id(), api_key.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Write the client_secret override for `provider`. Passing an empty string
/// clears the override. Same on-disk file as `oauth_set_client_id`.
///
/// Only meaningful for Google (Installed App requires a secret at the token
/// endpoint per their non-standard implementation). Microsoft public clients
/// don't use a secret; calling this for `"microsoft"` is allowed but pointless.
#[tauri::command]
pub async fn oauth_set_client_secret(
    provider: String,
    client_secret: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_client_secret(provider_id.id(), client_secret.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

// ─── Credentials file import ────────────────────────────────────────────────

/// Extract the client id and optional secret from a Google Cloud OAuth client
/// export.
///
/// The console hands the operator a JSON file rather than two strings on a
/// page, so reading the file directly removes the step where they hunt for
/// `client_id` inside it and paste the wrong half. Desktop clients nest under
/// `installed`; `web` is accepted too because the console produces that shape
/// for the other client type and the fields are identical.
fn parse_google_client_json(raw: &str) -> Result<(String, Option<String>), IntegrationsError> {
    let root: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| IntegrationsError::InvalidClientFile(format!("not valid JSON: {e}")))?;

    let section = root
        .get("installed")
        .or_else(|| root.get("web"))
        .ok_or_else(|| {
            IntegrationsError::InvalidClientFile(
                "expected an \"installed\" or \"web\" object at the root".to_owned(),
            )
        })?;

    let client_id = section
        .get("client_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IntegrationsError::InvalidClientFile("no client_id field".to_owned()))?
        .to_owned();

    let client_secret = section
        .get("client_secret")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    Ok((client_id, client_secret))
}

/// Import an OAuth client from the JSON file the provider's console produces.
///
/// Google only for now: Microsoft's portal shows the application id on screen
/// and issues no secret for a public client, so there is no file to import.
/// Both fields land in `~/.apollia/oauth-clients.toml` through the same
/// writers the manual fields use, which already create the file `0o600`.
#[tauri::command]
pub async fn oauth_import_client_json(
    provider: String,
    path: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    if provider_id != ConnectorProvider::Google {
        return Err(IntegrationsError::InvalidClientFile(format!(
            "{} does not publish a credentials file; paste the application id instead",
            provider_id.id()
        )));
    }

    let raw = std::fs::read_to_string(path.trim())
        .map_err(|e| IntegrationsError::InvalidClientFile(format!("cannot read the file: {e}")))?;
    let (client_id, client_secret) = parse_google_client_json(&raw)?;

    if let Some(reason) = client_id_shape_error(provider_id, &client_id) {
        return Err(IntegrationsError::InvalidClientFile(reason));
    }

    apollia_auth::oauth_clients_file::set_client_id(provider_id.id(), &client_id)
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    if let Some(secret) = client_secret {
        apollia_auth::oauth_clients_file::set_client_secret(provider_id.id(), &secret)
            .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_guid_like_accepts_canonical_guid() {
        // GIVEN two canonical GUIDs, one lowercase and one uppercase
        // WHEN each is checked for shape
        // THEN both pass, so case is not what a rejection means
        assert!(is_guid_like("12345678-1234-1234-1234-123456789abc"));
        assert!(is_guid_like("DEADBEEF-0000-1111-2222-333344445555"));
    }

    #[test]
    fn test_is_guid_like_rejects_malformed() {
        // GIVEN a word, a GUID without its dashes, one with a non-hexadecimal digit and an empty string
        // WHEN each is checked for shape
        // THEN all four are rejected
        assert!(!is_guid_like("not-a-guid"));
        assert!(!is_guid_like("12345678123412341234123456789abc"));
        assert!(!is_guid_like("12345678-1234-1234-1234-123456789abz"));
        assert!(!is_guid_like(""));
    }

    #[test]
    fn test_client_id_shape_error_google() {
        // GIVEN a well-formed Google client id
        // WHEN its shape is checked
        // THEN nothing is reported
        assert!(client_id_shape_error(
            ConnectorProvider::Google,
            "123-abc.apps.googleusercontent.com"
        )
        .is_none());
        // AND a malformed one
        // WHEN its shape is checked
        // THEN the operator is told before any OAuth round trip starts
        assert!(client_id_shape_error(ConnectorProvider::Google, "bogus").is_some());
    }

    #[test]
    fn test_client_id_shape_error_microsoft() {
        // GIVEN a well-formed Microsoft client id, then a malformed one
        // WHEN the shape of each is checked
        // THEN the first passes and the second is reported before any OAuth round trip
        assert!(client_id_shape_error(
            ConnectorProvider::Microsoft,
            "12345678-1234-1234-1234-123456789abc"
        )
        .is_none());
        assert!(client_id_shape_error(ConnectorProvider::Microsoft, "not-a-guid").is_some());
    }

    #[test]
    fn test_oauth_client_test_result_serializes() {
        // GIVEN the result of a connectivity test
        let result = OauthClientTestResult {
            ok: true,
            detail: "reachable".to_string(),
        };
        // WHEN it crosses the bridge as JSON
        let json = serde_json::to_value(&result).expect("serialize");
        // THEN the front end reads both the verdict and its detail
        assert_eq!(json["ok"], true);
        assert_eq!(json["detail"], "reachable");
    }

    #[test]
    fn test_credential_gate_google_without_secret_is_refused() {
        // GIVEN a Google client id configured but no paired secret
        let client_id = Some("123-abc.apps.googleusercontent.com");
        let client_secret = None;

        // WHEN the connect attempt is gated
        let err = credential_gate(ConnectorProvider::Google, client_id, client_secret)
            .expect_err("Google without a secret must not reach the browser");

        // THEN the refusal names the secret, before any consent screen opens
        match err {
            IntegrationsError::OauthClientSecretMissing(p) => assert_eq!(p, "google"),
            other => panic!("expected OauthClientSecretMissing, got: {other:?}"),
        }
    }

    #[test]
    fn test_credential_gate_google_blank_secret_counts_as_missing() {
        // GIVEN a secret entry that holds only whitespace
        let client_secret = Some("   ");

        // WHEN the connect attempt is gated
        let err = credential_gate(
            ConnectorProvider::Google,
            Some("123-abc.apps.googleusercontent.com"),
            client_secret,
        )
        .expect_err("a blank secret is not a secret");

        // THEN it is treated exactly like an absent one
        assert!(matches!(
            err,
            IntegrationsError::OauthClientSecretMissing(_)
        ));
    }

    #[test]
    fn test_credential_gate_microsoft_needs_no_secret() {
        // GIVEN a Microsoft public client, which the spec says carries no secret
        let client_id = Some("00000000-1111-2222-3333-444444444444");

        // WHEN the connect attempt is gated without one
        let result = credential_gate(ConnectorProvider::Microsoft, client_id, None);

        // THEN it is allowed through
        assert!(result.is_ok(), "Microsoft must connect without a secret");
    }

    #[test]
    fn test_credential_gate_without_client_id_is_refused_first() {
        // GIVEN no client id at all, for a provider that also wants a secret
        // WHEN the connect attempt is gated
        let err = credential_gate(ConnectorProvider::Google, None, None)
            .expect_err("no client id means nothing to connect with");

        // THEN the missing client id is reported, not the missing secret
        match err {
            IntegrationsError::OauthClientNotConfigured(p) => assert_eq!(p, "google"),
            other => panic!("expected OauthClientNotConfigured, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_google_client_json_reads_installed_section() {
        // GIVEN the file a Google Cloud Desktop client download produces
        let raw = r#"{
            "installed": {
                "client_id": "123-abc.apps.googleusercontent.com",
                "project_id": "apollia-test",
                "client_secret": "GOCSPX-example",
                "redirect_uris": ["http://localhost"]
            }
        }"#;

        // WHEN it is parsed
        let (id, secret) = parse_google_client_json(raw).expect("parse");

        // THEN both halves come out, so the operator never reads the file
        assert_eq!(id, "123-abc.apps.googleusercontent.com");
        assert_eq!(secret.as_deref(), Some("GOCSPX-example"));
    }

    #[test]
    fn test_parse_google_client_json_accepts_web_section() {
        // GIVEN the other shape the console produces
        let raw = r#"{"web": {"client_id": "456-def.apps.googleusercontent.com"}}"#;

        // WHEN it is parsed
        let (id, secret) = parse_google_client_json(raw).expect("parse");

        // THEN the client id is read and the absent secret stays absent
        assert_eq!(id, "456-def.apps.googleusercontent.com");
        assert!(secret.is_none());
    }

    #[test]
    fn test_parse_google_client_json_rejects_unknown_shape() {
        // GIVEN a JSON file that is not an OAuth client export
        let raw = r#"{"type": "service_account", "private_key": "..."}"#;

        // WHEN it is parsed
        let err =
            parse_google_client_json(raw).expect_err("service accounts are not OAuth clients");

        // THEN the operator is told what was expected
        match err {
            IntegrationsError::InvalidClientFile(reason) => assert!(reason.contains("installed")),
            other => panic!("expected InvalidClientFile, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_google_client_json_rejects_malformed_input() {
        // GIVEN a file that is not JSON at all
        // WHEN it is parsed
        let err = parse_google_client_json("not json").expect_err("must not accept garbage");

        // THEN the failure is attributed to the file, not to Apollia
        assert!(matches!(err, IntegrationsError::InvalidClientFile(_)));
    }
}
