//! A secret typed into a configuration sent to the MCP routes ends in the
//! secret store, never in `mcp.db`, and is never read back in clear.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_mcp::config::MCP_STATIC_SECRET_SERVICE;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;

use super::governance_tests::{harness, mock_server, persisted, Harness};
use super::{add_server, get_server_raw_config, update_server_config};
use crate::api::test_support::MemorySecrets;

async fn runtime() -> (Harness, MemorySecrets) {
    let mut h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let secrets = MemorySecrets::default();
    h.state.secret_store = secrets.opener();
    (h, secrets)
}

fn stored_env(h: &Harness, name: &str) -> HashMap<String, String> {
    persisted(h)
        .into_iter()
        .find(|c| c.name == name)
        .map(|c| c.env)
        .unwrap_or_default()
}

#[tokio::test]
async fn test_a_key_typed_at_the_addition_goes_to_the_store() {
    // GIVEN a server whose key and setting are typed in clear
    let (h, secrets) = runtime().await;
    let mut config = mock_server("calc");
    config.env = HashMap::from([
        ("CALC_API_KEY".into(), "sk-typed".into()),
        ("LOG_LEVEL".into(), "debug".into()),
    ]);

    // WHEN it is added through the route
    let _added = add_server(State(h.state.clone()), Json(config))
        .await
        .unwrap();

    // THEN mcp.db holds a placeholder for the key, and the store the value
    let env = stored_env(&h, "calc");
    assert_eq!(env["CALC_API_KEY"], "${APOLLIA_SECRET:CALC_API_KEY}");
    assert_eq!(env["LOG_LEVEL"], "debug");
    assert_eq!(
        secrets
            .value(MCP_STATIC_SECRET_SERVICE, "calc:CALC_API_KEY")
            .as_deref(),
        Some("sk-typed")
    );
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_a_key_typed_at_a_change_goes_to_the_store() {
    // GIVEN a running server with no key
    let (h, secrets) = runtime().await;
    let _added = add_server(State(h.state.clone()), Json(mock_server("calc")))
        .await
        .unwrap();

    // WHEN a token is typed in clear through a change of its configuration
    let _status = update_server_config(
        State(h.state.clone()),
        Path("calc".to_string()),
        Json(json!({"env": {"CALC_TOKEN": "tok-typed"}})),
    )
    .await
    .unwrap();

    // THEN mcp.db holds a placeholder, and the store the value
    let env = stored_env(&h, "calc");
    assert_eq!(env["CALC_TOKEN"], "${APOLLIA_SECRET:CALC_TOKEN}");
    assert_eq!(
        secrets
            .value(MCP_STATIC_SECRET_SERVICE, "calc:CALC_TOKEN")
            .as_deref(),
        Some("tok-typed")
    );
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_the_raw_configuration_masks_a_header_kept_in_clear() {
    // GIVEN a stored server reached at an address with a header in clear, as
    // an earlier version wrote it
    let (h, _secrets) = runtime().await;
    let mut config = mock_server("remote");
    config.command = String::new();
    config.args = Vec::new();
    config.transport = "streamable-http".into();
    config.url = Some("https://mcp.example.org/mcp".into());
    config.env = HashMap::from([("Authorization".into(), "Bearer sk-clear".into())]);
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .save(&config)
        .unwrap();

    // WHEN its raw configuration is read
    let Json(shown) = get_server_raw_config(State(h.state.clone()), Path("remote".to_string()))
        .await
        .unwrap();

    // THEN the header is a placeholder, and the value appears nowhere
    assert_eq!(
        shown.env["Authorization"],
        "${APOLLIA_SECRET:Authorization}"
    );
    assert!(!serde_json::to_string(&shown).unwrap().contains("sk-clear"));
    h.manager.shutdown().await;
}
