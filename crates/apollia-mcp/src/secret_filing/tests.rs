use std::collections::HashMap;
use std::sync::Mutex;

use apollia_auth::{AuthError, SecretStore};

use super::*;

/// A secret store held in memory, standing in for the keychain.
#[derive(Default)]
struct Memory(Mutex<HashMap<(String, String), String>>);

impl Memory {
    fn value(&self, user: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .get(&(MCP_STATIC_SECRET_SERVICE.to_owned(), user.to_owned()))
            .cloned()
    }
}

impl SecretStore for Memory {
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

/// A store that refuses every write.
struct Locked;

impl SecretStore for Locked {
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), AuthError> {
        Err(AuthError::Keyring("locked".into()))
    }
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, AuthError> {
        Ok(None)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
    fn backend_id(&self) -> &'static str {
        "locked"
    }
}

fn server(transport: &str, env: &[(&str, &str)]) -> McpServerConfig {
    let mut config: McpServerConfig =
        toml::from_str(&format!("name = \"acme\"\ntransport = \"{transport}\"")).unwrap();
    config.env = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    config
}

#[test]
fn test_every_literal_header_of_an_address_goes_to_the_store() {
    // GIVEN a server reached at an address with two literal headers and one
    // already a placeholder
    let store = Memory::default();
    let mut config = server(
        "streamable-http",
        &[
            ("Authorization", "Bearer sk-live"),
            ("X-Workspace", "w-42"),
            ("X-Other", "${APOLLIA_SECRET:OTHER}"),
        ],
    );

    // WHEN its secrets are filed
    let moved = file_literal_secrets(&mut config, &store, None).unwrap();

    // THEN both literal headers are in the store and replaced by placeholders
    assert_eq!(moved, vec!["Authorization", "X-Workspace"]);
    assert_eq!(
        store.value("acme:Authorization").as_deref(),
        Some("Bearer sk-live")
    );
    assert_eq!(store.value("acme:X-Workspace").as_deref(), Some("w-42"));
    assert_eq!(
        config.env["Authorization"],
        "${APOLLIA_SECRET:Authorization}"
    );
    assert_eq!(config.env["X-Workspace"], "${APOLLIA_SECRET:X-Workspace}");
    // AND the placeholder is untouched
    assert_eq!(config.env["X-Other"], "${APOLLIA_SECRET:OTHER}");
}

#[test]
fn test_a_process_keeps_its_plain_variables_and_files_its_secret_ones() {
    // GIVEN a process with a key, a password and a plain setting
    let store = Memory::default();
    let mut config = server(
        "stdio",
        &[
            ("ACME_API_KEY", "k-1"),
            ("db_password", "p-2"),
            ("LOG_LEVEL", "debug"),
            ("HOME_DIR", "${HOME}"),
        ],
    );

    // WHEN its secrets are filed
    let moved = file_literal_secrets(&mut config, &store, None).unwrap();

    // THEN only the two marked secret are moved
    assert_eq!(moved, vec!["ACME_API_KEY", "db_password"]);
    assert_eq!(store.value("acme:ACME_API_KEY").as_deref(), Some("k-1"));
    assert_eq!(config.env["db_password"], "${APOLLIA_SECRET:db_password}");
    assert_eq!(config.env["LOG_LEVEL"], "debug");
    assert_eq!(config.env["HOME_DIR"], "${HOME}");
}

#[test]
fn test_a_changed_value_overwrites_the_key_it_had() {
    // GIVEN a stored configuration pointing TOKEN at the key T, and a new
    // literal value for it
    let store = Memory::default();
    let previous = server("stdio", &[("TOKEN", "${APOLLIA_SECRET:T}")]);
    let mut config = server("stdio", &[("TOKEN", "new")]);

    // WHEN its secrets are filed
    file_literal_secrets(&mut config, &store, Some(&previous)).unwrap();

    // THEN the new value takes the key T
    assert_eq!(store.value("acme:T").as_deref(), Some("new"));
    assert_eq!(config.env["TOKEN"], "${APOLLIA_SECRET:T}");
}

#[test]
fn test_a_key_another_variable_names_is_not_taken() {
    // GIVEN a variable already pointing at the key TOKEN, and a literal TOKEN
    let store = Memory::default();
    let mut config = server(
        "stdio",
        &[("API", "${APOLLIA_SECRET:TOKEN}"), ("TOKEN", "lit")],
    );

    // WHEN its secrets are filed
    file_literal_secrets(&mut config, &store, None).unwrap();

    // THEN the literal goes under another key
    assert_eq!(config.env["TOKEN"], "${APOLLIA_SECRET:TOKEN_2}");
    assert_eq!(store.value("acme:TOKEN_2").as_deref(), Some("lit"));
    assert_eq!(store.value("acme:TOKEN"), None);
}

#[test]
fn test_a_store_that_refuses_is_an_error_naming_the_variable() {
    // GIVEN a header and a store refusing every write
    let mut config = server("sse", &[("Authorization", "Bearer x")]);

    // WHEN its secrets are filed
    let error = file_literal_secrets(&mut config, &Locked, None).unwrap_err();

    // THEN the error names the variable, never the value
    let text = error.to_string();
    assert!(text.contains("Authorization"));
    assert!(!text.contains("Bearer x"));
    assert_eq!(config.env["Authorization"], "Bearer x");
}

#[test]
fn test_a_masked_configuration_shows_no_secret_and_comes_back_whole() {
    // GIVEN a stored configuration still holding a header in clear
    let stored = server("streamable-http", &[("Authorization", "Bearer sk-live")]);

    // WHEN it is masked, then sent back as shown
    let shown = masked(&stored);
    let mut back = shown.clone();
    restore_masked(&mut back, &stored);

    // THEN the shown value is a placeholder, and the stored value comes back
    assert_eq!(
        shown.env["Authorization"],
        "${APOLLIA_SECRET:Authorization}"
    );
    assert_eq!(back.env["Authorization"], "Bearer sk-live");
}

#[test]
fn test_secret_names_are_recognised_without_case() {
    // GIVEN names that mark a secret and names that do not
    let secret = ["api_key", "GITHUB_TOKEN", "ClientSecret", "PASSWORD"];
    let plain = ["LOG_LEVEL", "REGION", "PORT"];

    // WHEN each is classified
    // THEN the first ones are secret and the others are not
    assert!(secret.iter().all(|n| is_secret_name(n)));
    assert!(plain.iter().all(|n| !is_secret_name(n)));
}
