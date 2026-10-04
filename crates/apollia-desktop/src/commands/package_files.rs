//! Tauri IPC commands behind the preview of a `.apo` package before it is
//! installed or updated.
//!
//! - `preview_package_file`: `POST /api/v1/packages/preview`, what the package
//!   contains, who signed it, what it asks for, the connections it requires
//!   and brings, and what an update adds. Nothing is written.
//! - `install_package_file`: `POST /api/v1/packages/install`, with the
//!   confirmations the user gave in the preview.
//! - `trust_package_publisher`: `POST /api/v1/packages/trusted-keys`, the key
//!   that signed the package, under the name the user sees.
//!
//! The archive is named by its path on this machine: the runtime, in this
//! process or as a daemon, reads it there. The answers are the routes' JSON,
//! and a refusal reaches the interface as the runtime wrote it.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::State;

use crate::attached::RuntimeLink;

/// The preview of a package file.
const PREVIEW: &str = "/api/v1/packages/preview";

/// The installation, or the update, of a package file.
const INSTALL: &str = "/api/v1/packages/install";

/// The keyring of trusted publisher keys.
const TRUSTED_KEYS: &str = "/api/v1/packages/trusted-keys";

/// How long an installation may take on the daemon: it builds the Python
/// environment of the package and installs its requirements.
const INSTALL_DEADLINE: Duration = Duration::from_secs(30 * 60);

pub(crate) async fn preview_on(link: &RuntimeLink, path: &str) -> Result<Value, String> {
    link.post_json(PREVIEW, &json!({ "path": path })).await
}

pub(crate) async fn install_on(
    link: &RuntimeLink,
    path: &str,
    deps_confirmed: bool,
    changes_confirmed: bool,
) -> Result<Value, String> {
    let body = json!({
        "path": path,
        "depsConfirmed": deps_confirmed,
        "changesConfirmed": changes_confirmed,
    });
    let report = link
        .post_json_within(INSTALL, &body, INSTALL_DEADLINE)
        .await?;
    tracing::event!(
        tracing::Level::INFO,
        changes_confirmed = changes_confirmed,
        "package.file.installed"
    );
    Ok(report)
}

pub(crate) async fn trust_on(
    link: &RuntimeLink,
    public_key: &str,
    label: &str,
) -> Result<Value, String> {
    let body = json!({ "publicKey": public_key, "label": label });
    link.post_json(TRUSTED_KEYS, &body).await
}

/// What the package at `path` contains and asks for; nothing is written.
#[tauri::command]
pub async fn preview_package_file(
    link: State<'_, RuntimeLink>,
    path: String,
) -> Result<Value, String> {
    preview_on(link.inner(), &path).await
}

/// Installs, or updates, the package at `path`. `deps_confirmed` says the
/// user accepted the Python requirements it installs, `changes_confirmed`
/// that they accepted every access and connection an update adds.
#[tauri::command]
pub async fn install_package_file(
    link: State<'_, RuntimeLink>,
    path: String,
    deps_confirmed: bool,
    changes_confirmed: bool,
) -> Result<Value, String> {
    install_on(link.inner(), &path, deps_confirmed, changes_confirmed).await
}

/// Trusts the publisher key `public_key`, shown as `label`.
#[tauri::command]
pub async fn trust_package_publisher(
    link: State<'_, RuntimeLink>,
    public_key: String,
    label: String,
) -> Result<Value, String> {
    trust_on(link.inner(), &public_key, &label).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn link(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[tokio::test]
    async fn test_the_preview_names_the_file_and_answers_the_route() {
        // GIVEN a daemon that previews `crm.apo`
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(PREVIEW))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(json!({ "path": "/tmp/crm.apo" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "crm", "version": "1.0.0", "refusal": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the dialog asks for the preview of that file
        let preview = preview_on(&link(&server), "/tmp/crm.apo").await.unwrap();

        // THEN the route's answer comes back as it is
        assert_eq!(preview["name"], "crm");
        assert_eq!(preview["refusal"], Value::Null);
    }

    #[tokio::test]
    async fn test_the_install_carries_the_confirmations_given() {
        // GIVEN a daemon that installs `crm.apo`
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(INSTALL))
            .and(body_json(json!({
                "path": "/tmp/crm.apo",
                "depsConfirmed": true,
                "changesConfirmed": true,
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "name": "crm", "version": "1.1.0", "updated": true,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the user accepted the requirements and what the update adds
        let report = install_on(&link(&server), "/tmp/crm.apo", true, true)
            .await
            .unwrap();

        // THEN both confirmations reach the route and its report comes back
        assert_eq!(report["version"], "1.1.0");
    }

    #[tokio::test]
    async fn test_a_refused_install_reports_the_runtime_reason() {
        // GIVEN a daemon that wants the changes of the update confirmed
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(INSTALL))
            .respond_with(ResponseTemplate::new(428).set_body_json(json!({
                "error": "the update adds accesses: confirm them",
                "code": "changes_confirmation_required",
            })))
            .mount(&server)
            .await;

        // WHEN the install is sent without that confirmation
        let refused = install_on(&link(&server), "/tmp/crm.apo", false, false).await;

        // THEN the runtime's reason comes back
        assert!(refused.unwrap_err().contains("confirm"));
    }

    #[tokio::test]
    async fn test_trusting_a_publisher_imports_its_key_under_its_name() {
        // GIVEN a daemon whose keyring accepts a key
        let server = MockServer::start().await;
        let key = "ab".repeat(32);
        Mock::given(method("POST"))
            .and(path(TRUSTED_KEYS))
            .and(body_json(json!({ "publicKey": key, "label": "Acme" })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "fingerprint": "f00d", "label": "Acme", "origin": "user",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the user trusts the publisher of the package
        let trusted = trust_on(&link(&server), &key, "Acme").await.unwrap();

        // THEN the key is in the keyring under that name
        assert_eq!(trusted["origin"], "user");
    }

    #[test]
    fn test_the_package_file_commands_are_routed_to_a_daemon() {
        // GIVEN the three commands of the preview dialog
        let commands = [
            "preview_package_file",
            "install_package_file",
            "trust_package_publisher",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
