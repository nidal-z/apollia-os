use std::collections::HashMap;
use std::io::Write;

use apollia_mcp::config::{McpConfig, McpConfigError, McpServerConfig};
use apollia_mcp::config_writer::McpConfigWriter;
use tempfile::NamedTempFile;

fn sample_server(name: &str) -> McpServerConfig {
    McpServerConfig {
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        tags: vec![],
    }
}

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

/// An env value referencing an absent variable must be rejected at resolution time.
#[test]
fn test_config_unresolved_env_var_fails() {
    // GIVEN a server config whose env map references a variable absent from the environment
    let var_name = "APOLLIA_INT_TEST_MISSING_339";
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
        tags: vec![],
    };

    // WHEN env var resolution is requested
    let result = config.resolve_env(None);

    // THEN an UnresolvedEnvVar error is returned
    assert!(matches!(
        result,
        Err(McpConfigError::UnresolvedEnvVar { .. })
    ));
}

/// A server added via the writer must survive a full write → load roundtrip.
#[test]
fn test_config_writer_roundtrip() {
    // GIVEN an empty mcp.toml file
    let tmp = NamedTempFile::new().unwrap();
    let writer = McpConfigWriter::new(tmp.path().to_path_buf());

    // WHEN a server is added via the writer
    writer
        .add_server(&sample_server("roundtrip-server"))
        .unwrap();

    // THEN McpConfig::load parses the file and the server is present with the expected fields
    let loaded = McpConfig::load(tmp.path()).unwrap();
    assert_eq!(loaded.servers.len(), 1);
    assert_eq!(loaded.servers[0].name, "roundtrip-server");
    assert_eq!(loaded.servers[0].command, "python3");
    assert_eq!(loaded.servers[0].transport, "stdio");
}

/// Adding then removing a server leaves a valid, reloadable file.
#[test]
fn test_config_writer_add_then_remove_roundtrip() {
    // GIVEN an empty mcp.toml file
    let tmp = NamedTempFile::new().unwrap();
    let writer = McpConfigWriter::new(tmp.path().to_path_buf());
    writer.add_server(&sample_server("alpha")).unwrap();
    writer.add_server(&sample_server("beta")).unwrap();

    // WHEN one server is removed
    writer.remove_server("alpha").unwrap();

    // THEN the remaining file is valid and contains only the other server
    let loaded = McpConfig::load(tmp.path()).unwrap();
    assert_eq!(loaded.servers.len(), 1);
    assert_eq!(loaded.servers[0].name, "beta");
}
