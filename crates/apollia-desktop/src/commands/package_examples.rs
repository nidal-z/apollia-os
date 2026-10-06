//! Tauri IPC commands behind the example packages the Agents screen proposes.
//!
//! - `list_example_packages`: `GET /api/v1/packages/examples`, the examples
//!   the runtime carries, each with its agents and their mission, the
//!   connections it requires, and whether the user installed it.
//! - `preview_example_package`: `POST /api/v1/packages/examples/{name}/preview`,
//!   what the example contains and asks for. Nothing is written.
//! - `install_example_package`: `POST /api/v1/packages/examples/{name}/install`,
//!   with the confirmations the user gave in the preview.
//!
//! An example is named by its package, never by a path: the runtime, in this
//! process or as a daemon, carries its files. The answers are the routes'
//! JSON, and a refusal reaches the interface as the runtime wrote it.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// The examples the runtime proposes.
const EXAMPLES: &str = "/api/v1/packages/examples";

/// How long an installation may take on the daemon, as for a package file.
const INSTALL_DEADLINE: Duration = Duration::from_secs(30 * 60);

/// `/api/v1/packages/examples/{name}/{action}`, the name encoded.
fn example_path(name: &str, action: &str) -> String {
    format!("{EXAMPLES}/{}/{action}", segment(name))
}

pub(crate) async fn list_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(EXAMPLES).await
}

pub(crate) async fn preview_on(link: &RuntimeLink, name: &str) -> Result<Value, String> {
    link.post_json(&example_path(name, "preview"), &json!({}))
        .await
}

pub(crate) async fn install_on(
    link: &RuntimeLink,
    name: &str,
    deps_confirmed: bool,
    changes_confirmed: bool,
) -> Result<Value, String> {
    let body = json!({
        "depsConfirmed": deps_confirmed,
        "changesConfirmed": changes_confirmed,
    });
    let report = link
        .post_json_within(&example_path(name, "install"), &body, INSTALL_DEADLINE)
        .await?;
    tracing::event!(
        tracing::Level::INFO,
        example = %name,
        "package.example.installed"
    );
    Ok(report)
}

/// The example packages the runtime proposes, installed or not.
#[tauri::command]
pub async fn list_example_packages(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    list_on(link.inner()).await
}

/// What the example `name` contains and asks for; nothing is written.
#[tauri::command]
pub async fn preview_example_package(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<Value, String> {
    preview_on(link.inner(), &name).await
}

/// Installs the example `name`. `deps_confirmed` says the user accepted the
/// Python requirements it installs, `changes_confirmed` that they accepted
/// every access and connection an update adds.
#[tauri::command]
pub async fn install_example_package(
    link: State<'_, RuntimeLink>,
    name: String,
    deps_confirmed: bool,
    changes_confirmed: bool,
) -> Result<Value, String> {
    install_on(link.inner(), &name, deps_confirmed, changes_confirmed).await
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
    async fn test_the_list_answers_the_route() {
        // GIVEN a daemon that proposes one example
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(EXAMPLES))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "name": "veille", "version": "1.0.0", "installedVersion": null,
            }])))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the Agents screen asks for the examples
        let examples = list_on(&link(&server)).await.unwrap();

        // THEN the route's answer comes back as it is
        assert_eq!(examples[0]["name"], "veille");
        assert_eq!(examples[0]["installedVersion"], Value::Null);
    }

    #[tokio::test]
    async fn test_the_preview_names_the_example_in_the_path() {
        // GIVEN a daemon that previews the example `veille web`
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/packages/examples/veille%20web/preview"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "veille web", "version": "1.0.0", "refusal": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the dialog asks for its preview
        let preview = preview_on(&link(&server), "veille web").await.unwrap();

        // THEN the name went encoded and the preview comes back
        assert_eq!(preview["name"], "veille web");
    }

    #[tokio::test]
    async fn test_the_install_carries_the_confirmations_given() {
        // GIVEN a daemon that installs the example `veille`
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/packages/examples/veille/install"))
            .and(body_json(json!({
                "depsConfirmed": false,
                "changesConfirmed": true,
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "name": "veille", "version": "1.1.0",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the user accepted what the update adds
        let report = install_on(&link(&server), "veille", false, true)
            .await
            .unwrap();

        // THEN the confirmations reach the route and its report comes back
        assert_eq!(report["version"], "1.1.0");
    }

    #[tokio::test]
    async fn test_an_unknown_example_reports_the_runtime_reason() {
        // GIVEN a daemon that proposes no example of that name
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/packages/examples/ghost/install"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": "no example package named ghost",
                "code": "not_found",
            })))
            .mount(&server)
            .await;

        // WHEN it is installed
        let refused = install_on(&link(&server), "ghost", false, false).await;

        // THEN the runtime's reason comes back
        assert!(refused.unwrap_err().contains("ghost"));
    }

    #[test]
    fn test_the_example_commands_are_routed_to_a_daemon() {
        // GIVEN the three commands of the examples
        let commands = [
            "list_example_packages",
            "preview_example_package",
            "install_example_package",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
