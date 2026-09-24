//! REST routes that set and remove the static secret an MCP server's
//! configuration names, under `/api/v1/mcp/servers/:name/secrets/:key`.
//!
//! A configuration refers to a secret by placeholder, `${APOLLIA_SECRET:KEY}`
//! in its `env` or its headers, and the runtime resolves it at connection time
//! from the secret store under `"{name}:{KEY}"`
//! ([`apollia_mcp::config::DefaultMcpSecretResolver`]). Until these routes only
//! the desktop could put a value there, so a server that needs a key could not
//! be configured end to end over HTTP.
//!
//! The value goes in and never comes out: no route answers it, and
//! `raw_config` keeps rendering the placeholder alone. The secret is written
//! through [`apollia_auth::select_secret_store`], the store the resolver reads,
//! so a headless host on the encrypted-file backend is served as well as a
//! desktop on the OS keychain.
//!
//! A secret may be set before its server exists: `POST /api/v1/mcp/servers`
//! connects at once, so the value has to be in place first.

use axum::{extract::Path, http::StatusCode, routing::put, Json, Router};
use serde::Deserialize;

use apollia_auth::SecretStore;
use apollia_mcp::config::{is_valid_server_name, MCP_STATIC_SECRET_SERVICE};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Longest secret key accepted.
const MAX_KEY_LEN: usize = 128;

/// Longest server name accepted.
const MAX_NAME_LEN: usize = 128;

/// Largest secret value accepted. API keys and tokens run to a few kilobytes
/// at most; a service-account document fits well under this.
const MAX_VALUE_BYTES: usize = 64 * 1024;

/// Build the MCP secret router.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
pub fn mcp_secrets_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new().route(
        "/api/v1/mcp/servers/:name/secrets/:key",
        put(put_mcp_secret).delete(delete_mcp_secret),
    )
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

/// Body of `PUT /api/v1/mcp/servers/:name/secrets/:key`.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PutMcpSecretBody {
    /// The secret. Stored, never answered back.
    pub value: String,
}

/// Hand-written so a logged request can never print the value.
impl std::fmt::Debug for PutMcpSecretBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutMcpSecretBody")
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Whether `key` can stand in a `${APOLLIA_SECRET:KEY}` placeholder: an
/// environment variable name, `[A-Za-z_][A-Za-z0-9_]*`. The colon that
/// separates server from key in the store is therefore never part of a key.
fn is_valid_secret_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validate the path pair and build the store key, `"{name}:{key}"`.
fn store_key(name: &str, key: &str) -> Result<String, ApiError> {
    if name.len() > MAX_NAME_LEN || !is_valid_server_name(name) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "server name must be lowercase letters, digits, '_' or '-'",
        ));
    }
    if key.len() > MAX_KEY_LEN || !is_valid_secret_key(key) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "secret key must be an environment variable name, [A-Za-z_][A-Za-z0-9_]*",
        ));
    }
    Ok(format!("{name}:{key}"))
}

fn open_store() -> Result<Box<dyn SecretStore>, ApiError> {
    apollia_auth::select_secret_store().map_err(|e| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("secret store unavailable: {e}"),
        )
    })
}

/// Write `value` under `store_key` in the MCP static-secret service.
fn write_secret(store: &dyn SecretStore, store_key: &str, value: &str) -> Result<(), ApiError> {
    store
        .set(MCP_STATIC_SECRET_SERVICE, store_key, value)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Remove `store_key` from the MCP static-secret service. Idempotent.
fn remove_secret(store: &dyn SecretStore, store_key: &str) -> Result<(), ApiError> {
    store
        .delete(MCP_STATIC_SECRET_SERVICE, store_key)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// `PUT /api/v1/mcp/servers/:name/secrets/:key`: set the secret a
/// `${APOLLIA_SECRET:KEY}` placeholder of server `name` resolves to.
///
/// The server need not exist yet. A running server reads the new value at its
/// next connection (`POST /api/v1/mcp/servers/:name/restart`).
#[utoipa::path(
    put,
    path = "/api/v1/mcp/servers/{name}/secrets/{key}",
    tag = "mcp",
    params(
        ("name" = String, Path, description = "Server name"),
        ("key" = String, Path, description = "Secret key, as the placeholder names it"),
    ),
    request_body = PutMcpSecretBody,
    responses(
        (status = 204, description = "Secret stored"),
        (status = 400, description = "Invalid server name, key, or value", body = ApiErrorBody),
        (status = 500, description = "The secret store refused the write", body = ApiErrorBody),
        (status = 503, description = "No secret store is available", body = ApiErrorBody),
    )
)]
pub(crate) async fn put_mcp_secret(
    Path((name, key)): Path<(String, String)>,
    Json(body): Json<PutMcpSecretBody>,
) -> Result<StatusCode, ApiError> {
    let store_key = store_key(&name, &key)?;
    if body.value.is_empty() || body.value.len() > MAX_VALUE_BYTES {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "secret value must be non-empty and at most 64 KiB",
        ));
    }
    let store = open_store()?;
    write_secret(&*store, &store_key, &body.value)?;
    tracing::info!(server = %name, key = %key, backend = %store.backend_id(), "mcp.secret.stored");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/mcp/servers/:name/secrets/:key`: remove the secret.
/// Removing one that is absent succeeds.
#[utoipa::path(
    delete,
    path = "/api/v1/mcp/servers/{name}/secrets/{key}",
    tag = "mcp",
    params(
        ("name" = String, Path, description = "Server name"),
        ("key" = String, Path, description = "Secret key, as the placeholder names it"),
    ),
    responses(
        (status = 204, description = "Secret removed, or was absent"),
        (status = 400, description = "Invalid server name or key", body = ApiErrorBody),
        (status = 500, description = "The secret store refused the removal", body = ApiErrorBody),
        (status = 503, description = "No secret store is available", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_mcp_secret(
    Path((name, key)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let store_key = store_key(&name, &key)?;
    let store = open_store()?;
    remove_secret(&*store, &store_key)?;
    tracing::info!(server = %name, key = %key, backend = %store.backend_id(), "mcp.secret.removed");
    Ok(StatusCode::NO_CONTENT)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use apollia_auth::AuthError;
    use apollia_mcp::config::SecretResolver;

    use super::*;

    /// In-memory stand-in for the keychain, keyed by `(service, user)`.
    #[derive(Default)]
    struct MemoryStore(Mutex<HashMap<(String, String), String>>);

    impl SecretStore for MemoryStore {
        fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
            self.0
                .lock()
                .unwrap()
                .insert((service.to_owned(), user.to_owned()), value.to_owned());
            Ok(())
        }
        fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(service.to_owned(), user.to_owned()))
                .cloned())
        }
        fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
            self.0
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), user.to_owned()));
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "memory"
        }
    }

    /// Reads the store the way the connection-time resolver does.
    struct ResolverOver<'a>(&'a MemoryStore);

    #[async_trait::async_trait]
    impl SecretResolver for ResolverOver<'_> {
        fn get_secret(&self, key: &str) -> Result<String, String> {
            self.0
                .get(MCP_STATIC_SECRET_SERVICE, key)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("secret not found: {key}"))
        }
    }

    #[tokio::test]
    async fn test_a_secret_written_here_is_what_the_placeholder_resolves_to() {
        // GIVEN a server whose env names a secret by placeholder, and the
        // secret written through this module
        let store = MemoryStore::default();
        let key = store_key("notion", "NOTION_API_KEY").expect("valid pair");
        write_secret(&store, &key, "secret-value").expect("write");
        let config: apollia_mcp::config::McpServerConfig =
            serde_json::from_value(serde_json::json!({
                "name": "notion",
                "command": "npx",
                "env": { "NOTION_TOKEN": "${APOLLIA_SECRET:NOTION_API_KEY}" },
            }))
            .expect("config");

        // WHEN the configuration's env is resolved at connection time
        let env = config
            .resolve_env(Some(&ResolverOver(&store)))
            .await
            .expect("resolved");

        // THEN the placeholder expands to the value, and the configuration
        // itself still carries only the placeholder
        assert_eq!(
            env.get("NOTION_TOKEN").map(String::as_str),
            Some("secret-value")
        );
        assert_eq!(
            config.env.get("NOTION_TOKEN").map(String::as_str),
            Some("${APOLLIA_SECRET:NOTION_API_KEY}")
        );
    }

    #[test]
    fn test_removing_a_secret_twice_succeeds_and_leaves_nothing() {
        // GIVEN a stored secret
        let store = MemoryStore::default();
        let key = store_key("slack", "SLACK_BOT_TOKEN").expect("valid pair");
        write_secret(&store, &key, "xoxb").expect("write");

        // WHEN it is removed twice
        remove_secret(&store, &key).expect("first removal");
        remove_secret(&store, &key).expect("second removal");

        // THEN the store holds nothing under that key
        assert_eq!(store.get(MCP_STATIC_SECRET_SERVICE, &key).unwrap(), None);
    }

    #[test]
    fn test_store_key_refuses_names_and_keys_a_placeholder_cannot_carry() {
        // GIVEN pairs that break the server-name rule or the variable-name rule
        let refused = [
            ("Notion", "KEY"),
            ("", "KEY"),
            ("notion", ""),
            ("notion", "1KEY"),
            ("notion", "KEY-WITH-DASH"),
            ("notion", "A:B"),
        ];

        // WHEN each is turned into a store key
        // THEN each is a bad request
        for (name, key) in refused {
            let (code, _) = store_key(name, key).unwrap_err();
            assert_eq!(
                code,
                StatusCode::BAD_REQUEST,
                "{name}:{key} must be refused"
            );
        }
        // AND a conforming pair is joined by a colon, the resolver's lookup key
        assert_eq!(
            store_key("my-server_2", "_API_KEY").unwrap(),
            "my-server_2:_API_KEY"
        );
    }

    #[tokio::test]
    async fn test_put_refuses_an_empty_value_before_opening_the_store() {
        // GIVEN a conforming pair and an empty value
        let path = Path(("notion".to_owned(), "NOTION_API_KEY".to_owned()));
        let body = Json(PutMcpSecretBody {
            value: String::new(),
        });

        // WHEN it is put
        let (code, _) = put_mcp_secret(path, body).await.unwrap_err();

        // THEN it is a bad request
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_the_request_body_never_prints_its_value() {
        // GIVEN a body carrying a secret
        let body = PutMcpSecretBody {
            value: "sk-live-123".to_owned(),
        };

        // WHEN it is formatted for a log line
        let printed = format!("{body:?}");

        // THEN the value is not in it
        assert!(!printed.contains("sk-live-123"));
    }
}
