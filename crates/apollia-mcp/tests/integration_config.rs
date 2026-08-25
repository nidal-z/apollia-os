#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::io::Write;

use apollia_mcp::config::{McpConfig, McpConfigError, McpServerConfig};
use tempfile::NamedTempFile;

/// Two servers sharing the same name must be rejected at load time.
#[test]
fn test_config_duplicate_server_name_fails() {
    // GIVEN a mcp.toml containing two servers with the same name
    let toml = r#"
        [[servers]]
        name = "notion"
        command = "npx"
        [[servers]]
        name = "notion"
        command = "uvx"
    "#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(toml.as_bytes()).unwrap();

    // WHEN the config is loaded
    let result = McpConfig::load(file.path());

    // THEN a DuplicateServerName error is returned
    assert!(matches!(
        result,
        Err(McpConfigError::DuplicateServerName(_))
    ));
}

/// Serialises the env mutation below against any future sibling in this
/// binary: environment variables are process globals.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// An env value referencing an absent variable must be rejected at resolution time.
#[tokio::test]
async fn test_config_unresolved_env_var_fails() {
    // GIVEN a server config whose env map references a variable absent from the environment
    let _guard = ENV_LOCK.lock().await;
    let var_name = "APOLLIA_INT_TEST_MISSING_339";
    let previous = std::env::var_os(var_name);
    std::env::remove_var(var_name);
    let config = McpServerConfig {
        name: "test".to_string(),
        command: "python3".to_string(),
        args: vec![],
        env: HashMap::from([("KEY".to_string(), format!("${{{var_name}}}"))]),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
    };

    // WHEN env var resolution is requested
    let result = config.resolve_env(None).await;

    // Restore whatever the environment carried before the removal above.
    if let Some(v) = previous {
        std::env::set_var(var_name, v);
    }

    // THEN an UnresolvedEnvVar error is returned
    assert!(matches!(
        result,
        Err(McpConfigError::UnresolvedEnvVar { .. })
    ));
}
