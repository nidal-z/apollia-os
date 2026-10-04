//! Tauri IPC commands for the Settings view.
//!
//! `apollia.toml` is read and written whole by the editor of Settings >
//! Configuration, over the configuration routes of the runtime (`file`); a
//! named setting is written key by key through `settings`. Both go through the
//! one writer of the file, which keeps its comments and layout.
//!
//! Three neighbouring families live in their own modules: `lifecycle` for the
//! commands that reset, restart or end a session, `settings` for the named
//! settings read with their source and written key by key, `system` for the
//! Advanced and Security sections and the local-model setup helper.

pub mod file;
pub mod lifecycle;
pub mod settings;
pub mod system;

use apollia_runtime::api::routes_config::settings::ObservabilitySettings;
use serde_json::Value;
use tauri::State;

use crate::attached::RuntimeLink;

/// Resolves the standard path `~/.apollia/apollia.toml`.
fn default_config_path() -> std::path::PathBuf {
    apollia_core::paths::config_file_under(apollia_core::paths::home_dir_or_temp())
}

/// Route of the observability policy on the runtime.
pub(crate) const OBSERVABILITY_ROUTE: &str = "/api/v1/config/observability";

/// Reads the observability policy of the runtime this application drives:
/// the capture switches, byte limits and retention of `[observability]`, and
/// `debug_log_prompt` of `[llm.observability]`, in one form.
pub(crate) async fn observability_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(OBSERVABILITY_ROUTE).await
}

/// Writes the observability policy on the runtime this application drives,
/// through the one writer of its `apollia.toml`; applied at its next start.
pub(crate) async fn save_observability_on(
    link: &RuntimeLink,
    config: &ObservabilitySettings,
) -> Result<(), String> {
    let body = serde_json::to_value(config).map_err(|e| e.to_string())?;
    link.put_json(OBSERVABILITY_ROUTE, &body).await?;
    tracing::event!(
        tracing::Level::INFO,
        max_input_bytes = config.max_input_bytes,
        retention_days = config.retention_days,
        debug_log_prompt = config.debug_log_prompt,
        "observability.config_saved"
    );
    Ok(())
}

/// Returns the observability policy, over `GET /api/v1/config/observability`,
/// in both modes.
#[tauri::command]
pub async fn get_observability_config(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    observability_on(link.inner()).await
}

/// Persists the observability policy, over `PUT
/// /api/v1/config/observability`, in both modes.
#[tauri::command]
pub async fn set_observability_config(
    link: State<'_, RuntimeLink>,
    config: ObservabilitySettings,
) -> Result<(), String> {
    save_observability_on(link.inner(), &config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path_ends_with_apollia_toml() {
        // GIVEN the default config path
        let path = default_config_path();
        // WHEN it is resolved
        // THEN it ends with apollia.toml inside .apollia directory
        assert!(path.ends_with("apollia.toml"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }

    mod attached {
        use serde_json::json;
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::super::*;
        use crate::attached::{gate, AttachedClient, LinkMode};

        const TOKEN: &str = "token-of-this-installation";

        fn attached(server: &MockServer) -> RuntimeLink {
            RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
        }

        fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
            Mock::given(method(verb))
                .and(path(route))
                .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        }

        fn policy(retention_days: u32) -> ObservabilitySettings {
            ObservabilitySettings {
                retention_days,
                debug_log_prompt: true,
                ..ObservabilitySettings::default()
            }
        }

        #[tokio::test]
        async fn test_the_observability_policy_is_read_from_the_daemon() {
            // GIVEN a daemon holding a retention of 30 days
            let server = MockServer::start().await;
            let held = serde_json::to_value(policy(30)).unwrap();
            authorized("GET", OBSERVABILITY_ROUTE)
                .respond_with(ResponseTemplate::new(200).set_body_json(held.clone()))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the Observability screen reads the policy
            let read = observability_on(&attached(&server)).await.unwrap();

            // THEN it is the daemon's, as it answered
            assert_eq!(read, held);
        }

        #[tokio::test]
        async fn test_the_observability_policy_is_written_to_the_daemon() {
            // GIVEN a daemon that takes the whole policy
            let server = MockServer::start().await;
            let sent = serde_json::to_value(policy(7)).unwrap();
            authorized("PUT", OBSERVABILITY_ROUTE)
                .and(body_json(sent.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_json(sent))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the screen saves a retention of 7 days
            let saved = save_observability_on(&attached(&server), &policy(7)).await;

            // THEN the write succeeds
            assert!(saved.is_ok(), "{saved:?}");
        }

        #[tokio::test]
        async fn test_a_refusal_of_the_daemon_is_shown_as_it_said_it() {
            // GIVEN a daemon that refuses the write with its sentence
            let server = MockServer::start().await;
            authorized("PUT", OBSERVABILITY_ROUTE)
                .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                    "error": "observability.retention_days is fixed by the organisation policy",
                    "code": "locked",
                    "diagnostics": [],
                })))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the screen saves the policy
            let refused = save_observability_on(&attached(&server), &policy(7)).await;

            // THEN the error is the daemon's sentence, word for word
            assert_eq!(
                refused.unwrap_err(),
                "observability.retention_days is fixed by the organisation policy"
            );
        }

        #[test]
        fn test_the_observability_commands_are_routed_to_a_daemon() {
            // GIVEN the two commands of the Observability policy
            let commands = ["get_observability_config", "set_observability_config"];

            // WHEN each passes the gate of an attached window
            // THEN none is refused
            for command in commands {
                assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
            }
        }
    }
}
