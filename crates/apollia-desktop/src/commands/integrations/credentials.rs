//! The OAuth credentials a connector needs before a browser window opens:
//! client id, client secret, API key, and the Drive folder a Google account
//! writes to.
//!
//! The client id, secret and API key live in the runtime
//! (`apollia_runtime::oauth_clients`): the commands below call it directly
//! when the runtime runs in this process, and reach the same functions over
//! `/api/v1/oauth/clients` when the window is attached to a daemon. The Drive
//! folders and the gate that refuses a half-resolved credential stay here.

use apollia_auth::ConnectorProvider;
use apollia_runtime::api::routes_oauth_clients::{ImportOauthClientBody, OauthClientValueBody};
pub use apollia_runtime::oauth_clients::OauthClientIdStatus;
use apollia_runtime::oauth_clients::{
    self, OauthClientField, OauthClientTestResult, OauthClientsError,
};
use serde::Serialize;
use tauri::State;

use super::{auth_manager, IntegrationsError};
use crate::attached::{segment, AttachedError, RuntimeLink};

/// The route of the OAuth clients on the runtime.
const CLIENTS: &str = "/api/v1/oauth/clients";

// ─── Credential gate ────────────────────────────────────────────────────────

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
    if oauth_clients::requires_client_secret(provider) && !has_value(client_secret) {
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

// ─── OAuth clients, in both modes ───────────────────────────────────────────

impl From<OauthClientsError> for IntegrationsError {
    fn from(error: OauthClientsError) -> Self {
        match error {
            OauthClientsError::UnknownProvider(p) => Self::UnknownProvider(p),
            OauthClientsError::InvalidClientFile(r) => Self::InvalidClientFile(r),
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<AttachedError> for IntegrationsError {
    fn from(error: AttachedError) -> Self {
        Self::Service(String::from(error))
    }
}

/// The client file of the runtime running in this process.
fn embedded_file() -> Result<std::path::PathBuf, IntegrationsError> {
    Ok(oauth_clients::default_file()?)
}

/// The path segment of a field's route.
const fn field_segment(field: OauthClientField) -> &'static str {
    match field {
        OauthClientField::ClientId => "client-id",
        OauthClientField::ClientSecret => "client-secret",
        OauthClientField::ApiKey => "api-key",
    }
}

async fn list_on(link: &RuntimeLink) -> Result<Vec<OauthClientIdStatus>, IntegrationsError> {
    match link {
        RuntimeLink::Embedded(_) => Ok(oauth_clients::list_client_ids(&embedded_file()?)),
        RuntimeLink::Attached(remote) => Ok(remote.get(CLIENTS).await?),
    }
}

async fn set_on(
    link: &RuntimeLink,
    provider: &str,
    field: OauthClientField,
    value: String,
) -> Result<(), IntegrationsError> {
    match link {
        RuntimeLink::Embedded(_) => Ok(oauth_clients::set_field(
            &embedded_file()?,
            provider,
            field,
            &value,
        )?),
        RuntimeLink::Attached(remote) => {
            let path = format!("{CLIENTS}/{}/{}", segment(provider), field_segment(field));
            let _: serde_json::Value = remote.put(&path, &OauthClientValueBody { value }).await?;
            Ok(())
        }
    }
}

async fn test_on(
    link: &RuntimeLink,
    provider: &str,
) -> Result<OauthClientTestResult, IntegrationsError> {
    match link {
        RuntimeLink::Embedded(_) => {
            Ok(oauth_clients::test_client(&embedded_file()?, provider).await?)
        }
        RuntimeLink::Attached(remote) => {
            let path = format!("{CLIENTS}/{}/test", segment(provider));
            Ok(remote.post(&path, &serde_json::json!({})).await?)
        }
    }
}

async fn import_on(
    link: &RuntimeLink,
    provider: &str,
    client_json: String,
) -> Result<(), IntegrationsError> {
    match link {
        RuntimeLink::Embedded(_) => Ok(oauth_clients::import_client_json(
            &embedded_file()?,
            provider,
            &client_json,
        )?),
        RuntimeLink::Attached(remote) => {
            let path = format!("{CLIENTS}/{}/import", segment(provider));
            let body = ImportOauthClientBody { client_json };
            let _: serde_json::Value = remote.post(&path, &body).await?;
            Ok(())
        }
    }
}

/// Snapshot the current OAuth client configuration per provider: which client
/// id is active, where it came from, and whether a secret and an API key
/// resolve. Carries no token material and no secret value.
#[tauri::command]
pub async fn oauth_list_client_ids(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<OauthClientIdStatus>, IntegrationsError> {
    list_on(link.inner()).await
}

/// Validate a provider's configured OAuth client non-interactively: a client
/// id resolves, it is well-formed, a secret resolves where the provider needs
/// one, and the provider's discovery endpoint answers. Returns `{ ok, detail }`.
#[tauri::command]
pub async fn oauth_test_client(
    link: State<'_, RuntimeLink>,
    provider: String,
) -> Result<OauthClientTestResult, IntegrationsError> {
    test_on(link.inner(), &provider).await
}

/// Write the client id override for `provider`. An empty string clears it, so
/// the resolution chain falls back to the env var or the compiled default.
#[tauri::command]
pub async fn oauth_set_client_id(
    link: State<'_, RuntimeLink>,
    provider: String,
    client_id: String,
) -> Result<(), IntegrationsError> {
    set_on(
        link.inner(),
        &provider,
        OauthClientField::ClientId,
        client_id,
    )
    .await
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

/// Persist the public Google API key used by Google Picker, beside the client
/// id and secret. An empty string clears the override.
#[tauri::command]
pub async fn oauth_set_api_key(
    link: State<'_, RuntimeLink>,
    provider: String,
    api_key: String,
) -> Result<(), IntegrationsError> {
    set_on(link.inner(), &provider, OauthClientField::ApiKey, api_key).await
}

/// Write the client secret override for `provider`. An empty string clears
/// it. Only meaningful for Google (Installed App requires a secret at the
/// token endpoint); Microsoft public clients use none.
#[tauri::command]
pub async fn oauth_set_client_secret(
    link: State<'_, RuntimeLink>,
    provider: String,
    client_secret: String,
) -> Result<(), IntegrationsError> {
    set_on(
        link.inner(),
        &provider,
        OauthClientField::ClientSecret,
        client_secret,
    )
    .await
}

// ─── Credentials file import ────────────────────────────────────────────────

/// Import an OAuth client from the JSON file the provider's console produces.
///
/// The file is the one the operator picked on this machine, so this window
/// reads it; the runtime, here or the daemon, parses the text and writes the
/// client id and secret where the connectors read them.
#[tauri::command]
pub async fn oauth_import_client_json(
    link: State<'_, RuntimeLink>,
    provider: String,
    path: String,
) -> Result<(), IntegrationsError> {
    let client_json = std::fs::read_to_string(path.trim())
        .map_err(|e| IntegrationsError::InvalidClientFile(format!("cannot read the file: {e}")))?;
    import_on(link.inner(), &provider, client_json).await
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── The attached arm ─────────────────────────────────────────────────

    use crate::attached::AttachedClient;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn bearer() -> String {
        format!("Bearer {TOKEN}")
    }

    #[tokio::test]
    async fn test_the_clients_are_read_from_the_daemon_with_the_token() {
        // GIVEN a daemon answering one Google row, only to the token
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/oauth/clients"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "provider": "google", "effective_client_id": "1-a.apps.googleusercontent.com",
                    "source": "file", "override_client_id": "1-a.apps.googleusercontent.com",
                    "has_client_secret": true, "client_secret_source": "file",
                    "requires_client_secret": true, "has_api_key": false,
                    "api_key_source": "none", "requires_api_key": true,
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached arm lists the clients
        let rows = list_on(&attached(&server)).await.unwrap();

        // THEN the daemon's row reads as the one the settings draw
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, "google");
        assert_eq!(rows[0].source, "file");
        assert!(rows[0].has_client_secret);
    }

    #[tokio::test]
    async fn test_each_field_is_put_on_its_own_route_with_the_token() {
        // GIVEN a daemon accepting the three writes of one provider
        let server = MockServer::start().await;
        for (route, value) in [
            ("client-id", "1-a.apps.googleusercontent.com"),
            ("client-secret", "GOCSPX-s"),
            ("api-key", "AIza-k"),
        ] {
            Mock::given(method("PUT"))
                .and(path(format!("/api/v1/oauth/clients/google/{route}")))
                .and(header("authorization", bearer().as_str()))
                .and(body_json(serde_json::json!({ "value": value })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "provider": "google" })),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        let link = attached(&server);

        // WHEN the attached arm writes the id, the secret and the key
        let id = set_on(
            &link,
            "google",
            OauthClientField::ClientId,
            "1-a.apps.googleusercontent.com".into(),
        )
        .await;
        let secret = set_on(
            &link,
            "google",
            OauthClientField::ClientSecret,
            "GOCSPX-s".into(),
        )
        .await;
        let key = set_on(&link, "google", OauthClientField::ApiKey, "AIza-k".into()).await;

        // THEN each reached its route, which the mock verifies on drop
        assert!(id.is_ok() && secret.is_ok() && key.is_ok());
    }

    #[tokio::test]
    async fn test_a_client_is_tested_by_the_daemon() {
        // GIVEN a daemon whose check of the Google client fails on the secret
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth/clients/google/test"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "detail": "client secret is required for this provider but not configured",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached arm asks for the test
        let verdict = test_on(&attached(&server), "google").await.unwrap();

        // THEN the daemon's verdict is the one shown
        assert!(!verdict.ok);
        assert!(verdict.detail.contains("client secret"));
    }

    #[tokio::test]
    async fn test_an_export_is_read_here_and_its_text_posted_to_the_daemon() {
        // GIVEN an export on this machine, and a daemon accepting its text
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("client_secret.json");
        let export = r#"{"installed": {"client_id": "9-z.apps.googleusercontent.com"}}"#;
        std::fs::write(&file, export).unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth/clients/google/import"))
            .and(header("authorization", bearer().as_str()))
            .and(body_json(serde_json::json!({ "client_json": export })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "provider": "google" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window reads the file and the attached arm imports it
        let text = std::fs::read_to_string(&file).unwrap();
        let imported = import_on(&attached(&server), "google", text).await;

        // THEN the daemon received the text, not a path it cannot open
        assert!(imported.is_ok(), "{imported:?}");
    }

    #[tokio::test]
    async fn test_a_refusal_of_the_daemon_is_shown_in_its_own_words() {
        // GIVEN a daemon refusing an export that is not a client
        let server = MockServer::start().await;
        let words =
            "invalid OAuth client file: expected an \"installed\" or \"web\" object at the root";
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth/clients/google/import"))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(serde_json::json!({ "error": words })),
            )
            .mount(&server)
            .await;

        // WHEN the attached arm imports it
        let err = import_on(&attached(&server), "google", "{}".into())
            .await
            .unwrap_err();

        // THEN the error carries the daemon's words unchanged
        match err {
            IntegrationsError::Service(message) => assert_eq!(message, words),
            other => panic!("expected Service, got: {other:?}"),
        }
    }

    #[test]
    fn test_the_six_client_commands_pass_the_gate_attached() {
        // GIVEN the commands of the OAuth client settings
        let commands = [
            "oauth_import_client_json",
            "oauth_list_client_ids",
            "oauth_set_client_id",
            "oauth_set_client_secret",
            "oauth_set_api_key",
            "oauth_test_client",
        ];

        // WHEN each is gated while attached
        let refused: Vec<&str> = commands
            .into_iter()
            .filter(|c| crate::attached::gate(crate::attached::LinkMode::Attached, c).is_err())
            .collect();

        // THEN none answers that the service makes it unavailable
        assert!(refused.is_empty(), "refused: {refused:?}");
    }
}
