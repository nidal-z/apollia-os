use std::path::PathBuf;

use serde_json::json;

use super::*;

const KEY: &str = "desktop.developer_mode";

fn config_with(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

// ── Resolution ───────────────────────────────────────────────────────────

#[test]
fn test_a_policy_imposing_a_value_resolves_to_it_with_the_source_policy() {
    // GIVEN a file where the user turned developer mode off, and a test policy
    // imposing it on
    let (_dir, path) = config_with("[desktop]\ndeveloper_mode = false\n");
    let policy = PolicySettings::none().impose(KEY, json!(true));

    // WHEN the setting is resolved
    let resolved = resolve_setting(&path, KEY, &policy).unwrap();

    // THEN the imposed value wins and its source is the policy
    assert_eq!(resolved.value, json!(true));
    assert_eq!(resolved.source, SettingSource::Policy);
    assert_eq!(resolved.key, KEY);
}

#[test]
fn test_without_policy_a_written_key_resolves_with_the_source_user() {
    // GIVEN a file carrying the key, and no policy
    let (_dir, path) = config_with("# mine\n[desktop]\ndeveloper_mode = true\n");

    // WHEN the setting is resolved
    let resolved = resolve_setting(&path, KEY, &PolicySettings::none()).unwrap();

    // THEN the file's value is returned, from the user
    assert_eq!(resolved.value, json!(true));
    assert_eq!(resolved.source, SettingSource::User);
}

#[test]
fn test_without_policy_an_absent_file_resolves_with_the_source_default() {
    // GIVEN no apollia.toml at all
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");

    // WHEN the setting is resolved
    let resolved = resolve_setting(&path, KEY, &PolicySettings::none()).unwrap();

    // THEN the section's default applies
    assert_eq!(resolved.value, json!(false));
    assert_eq!(resolved.source, SettingSource::Default);
}

#[test]
fn test_an_unknown_key_is_refused_on_read_and_on_write() {
    // GIVEN keys that name no setting: a typo, a whole section, another file
    let (_dir, path) = config_with("");
    for key in [
        "desktop.not_a_field",
        "desktop",
        "chat.plan_mode_default",
        "",
    ] {
        // WHEN they are resolved and written
        let read = resolve_setting(&path, key, &PolicySettings::none());
        let write = write_setting(&path, key, json!(true), &PolicySettings::none());

        // THEN both are refused as unknown
        assert!(
            matches!(read, Err(SettingError::UnknownKey { .. })),
            "{key}: {read:?}"
        );
        assert!(
            matches!(write, Err(SettingError::UnknownKey { .. })),
            "{key}: {write:?}"
        );
    }
}

#[test]
fn test_a_value_of_the_wrong_type_in_the_file_is_reported() {
    // GIVEN the key written as a string
    let (_dir, path) = config_with("[desktop]\ndeveloper_mode = \"yes\"\n");

    // WHEN it is resolved
    let resolved = resolve_setting(&path, KEY, &PolicySettings::none());

    // THEN the type error is named, never a silent default
    assert!(matches!(resolved, Err(SettingError::InvalidValue { .. })));
}

// ── Writing ──────────────────────────────────────────────────────────────

#[test]
fn test_a_write_on_a_key_the_policy_locks_is_refused_and_the_file_is_unchanged() {
    // GIVEN a file with a comment, and a policy that locks the key
    let original = "# set by me\n[desktop]\ndeveloper_mode = false # off\n";
    let (_dir, path) = config_with(original);
    let policy = PolicySettings::none().impose(KEY, json!(false));

    // WHEN the user writes the key
    let result = write_setting(&path, KEY, json!(true), &policy);

    // THEN the typed error says the setting is locked by the policy
    let error = result.unwrap_err();
    assert!(
        matches!(&error, SettingError::LockedByPolicy { key } if key == KEY),
        "{error:?}"
    );
    assert!(error
        .to_string()
        .contains("locked by the organization policy"));
    // AND the file is byte for byte what it was
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn test_without_policy_a_write_passes_and_resolves_with_the_source_user() {
    // GIVEN a file with a comment and another section, and no policy
    let (_dir, path) = config_with("# keep me\n[chat]\nplan_mode_default = true\n");

    // WHEN developer mode is written
    let written = write_setting(&path, KEY, json!(true), &PolicySettings::none()).unwrap();

    // THEN the write returns the setting from the user
    assert_eq!(written.source, SettingSource::User);
    assert_eq!(written.value, json!(true));
    // AND reading it back agrees
    let resolved = resolve_setting(&path, KEY, &PolicySettings::none()).unwrap();
    assert_eq!(resolved.source, SettingSource::User);
    assert_eq!(resolved.value, json!(true));
    // AND the file gained a [desktop] section and kept the rest
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("# keep me"), "{content}");
    assert!(content.contains("plan_mode_default = true"), "{content}");
    assert!(content.contains("[desktop]"), "{content}");
    assert!(content.contains("developer_mode = true"), "{content}");
}

#[test]
fn test_without_policy_a_write_creates_an_absent_file() {
    // GIVEN no file and no folder yet
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("home").join("apollia.toml");

    // WHEN developer mode is written
    write_setting(&path, KEY, json!(true), &PolicySettings::none()).unwrap();

    // THEN the file exists and the typed section reads the value
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(
        DesktopConfig::from_toml_str(&content)
            .unwrap()
            .developer_mode
    );
    // AND no staging file is left behind
    assert!(!path.with_extension("toml.writing").exists());
}

#[test]
fn test_a_write_of_the_wrong_type_is_refused_and_the_file_is_unchanged() {
    // GIVEN a file without the key
    let original = "[chat]\nplan_mode_default = true\n";
    let (_dir, path) = config_with(original);

    // WHEN a string is written to a boolean setting
    let result = write_setting(&path, KEY, json!("yes"), &PolicySettings::none());

    // THEN it is refused and nothing is written
    assert!(matches!(result, Err(SettingError::InvalidValue { .. })));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn test_a_write_into_an_invalid_file_is_refused_and_the_file_is_unchanged() {
    // GIVEN a file that is not TOML
    let original = "[desktop\ndeveloper_mode = ";
    let (_dir, path) = config_with(original);

    // WHEN the setting is written
    let result = write_setting(&path, KEY, json!(true), &PolicySettings::none());

    // THEN the parse error is returned and the file is left as it was
    assert!(matches!(result, Err(SettingError::Parse { .. })));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn test_the_source_serializes_as_the_interface_reads_it() {
    // GIVEN the three sources
    // WHEN they are serialized
    let names: Vec<String> = [
        SettingSource::Default,
        SettingSource::User,
        SettingSource::Policy,
    ]
    .iter()
    .map(|s| serde_json::to_string(s).unwrap())
    .collect();

    // THEN they read as the interface expects
    assert_eq!(names, ["\"default\"", "\"user\"", "\"policy\""]);
}

// ── What a policy may impose ─────────────────────────────────────────────

#[test]
fn test_a_policy_may_impose_a_known_setting_with_its_type() {
    // GIVEN a setting of the socle and a value of its type
    let value = json!(true);

    // WHEN a policy imposing it is checked
    let checked = check_imposed_setting(KEY, &value);

    // THEN it is accepted
    assert!(checked.is_ok(), "{checked:?}");
}

#[test]
fn test_a_policy_may_not_impose_an_unknown_key_nor_a_wrong_type() {
    // GIVEN a key the socle does not declare, a whole section, and a known
    // key given a value of another type
    let cases = [
        ("desktop.inconnu", json!(true)),
        ("desktop", json!(true)),
        (KEY, json!("yes")),
    ];

    // WHEN a policy imposing each is checked
    let [unknown, section, mistyped] = cases.map(|(key, value)| check_imposed_setting(key, &value));

    // THEN each is refused, naming the key
    assert!(matches!(unknown, Err(SettingError::UnknownKey { key }) if key == "desktop.inconnu"));
    assert!(matches!(section, Err(SettingError::UnknownKey { key }) if key == "desktop"));
    assert!(matches!(mistyped, Err(SettingError::InvalidValue { key, .. }) if key == KEY));
}

// ── The compatible server's keys ─────────────────────────────────────────

#[test]
fn test_the_server_keys_resolve_to_their_defaults() {
    // GIVEN no apollia.toml at all
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");

    // WHEN both server keys are resolved
    let enabled = resolve_setting(&path, "server.enabled", &PolicySettings::none()).unwrap();
    let port = resolve_setting(&path, "server.port", &PolicySettings::none()).unwrap();

    // THEN the server is off, on port 7772, both from the defaults
    assert_eq!(enabled.value, json!(false));
    assert_eq!(port.value, json!(7772));
    assert_eq!(port.source, SettingSource::Default);
}

#[test]
fn test_server_cors_is_a_setting_off_by_default_and_writable() {
    // GIVEN a file without a [server] section
    let (_dir, path) = config_with("[desktop]\ndeveloper_mode = true\n");
    let before = resolve_setting(&path, "server.cors", &PolicySettings::none()).unwrap();

    // WHEN cross-origin answers are switched on
    write_setting(&path, "server.cors", json!(true), &PolicySettings::none()).unwrap();

    // THEN the key was off from the defaults, and now reads on from the user
    assert_eq!(before.value, json!(false));
    assert_eq!(before.source, SettingSource::Default);
    let after = resolve_setting(&path, "server.cors", &PolicySettings::none()).unwrap();
    assert_eq!(after.value, json!(true));
    assert_eq!(after.source, SettingSource::User);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(ServerConfig::from_toml_str(&written).unwrap().cors);
}

#[test]
fn test_writing_server_enabled_keeps_the_rest_of_the_file() {
    // GIVEN a file carrying a comment and the desktop section
    let (_dir, path) = config_with("# mine\n[desktop]\ndeveloper_mode = true\n");

    // WHEN the server is switched on
    write_setting(
        &path,
        "server.enabled",
        json!(true),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the key is written under [server] and the rest is untouched
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.starts_with("# mine\n[desktop]\ndeveloper_mode = true\n"),
        "{written}"
    );
    assert!(ServerConfig::from_toml_str(&written).unwrap().enabled);
}

#[test]
fn test_a_server_port_out_of_range_is_refused_and_the_file_unchanged() {
    // GIVEN a file with a valid server port
    let (_dir, path) = config_with("[server]\nport = 8000\n");

    // WHEN a port no TCP listener can take is written
    let result = write_setting(&path, "server.port", json!(70000), &PolicySettings::none());

    // THEN it is refused and the file keeps its port
    assert!(
        matches!(result, Err(SettingError::InvalidValue { .. })),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[server]\nport = 8000\n"
    );
}

// ── The MCP bridge switch ────────────────────────────────────────────────

#[test]
fn test_the_mcp_bridge_switch_is_a_named_setting_off_by_default() {
    // GIVEN a file without an [mcp] section
    let (_dir, path) = config_with("[desktop]\ndeveloper_mode = true\n");

    // WHEN the bridge switch is resolved
    let resolved = resolve_setting(&path, "mcp.server_enabled", &PolicySettings::none()).unwrap();

    // THEN it is known, off, and comes from the default
    assert_eq!(resolved.value, json!(false));
    assert_eq!(resolved.source, SettingSource::Default);
}

#[test]
fn test_the_mcp_bridge_switch_written_on_resolves_on_from_the_user() {
    // GIVEN an empty file
    let (_dir, path) = config_with("");

    // WHEN the bridge switch is written on, then resolved
    write_setting(
        &path,
        "mcp.server_enabled",
        json!(true),
        &PolicySettings::none(),
    )
    .unwrap();
    let resolved = resolve_setting(&path, "mcp.server_enabled", &PolicySettings::none()).unwrap();

    // THEN the file carries it under [mcp], and it resolves on from the user
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("[mcp]"), "{written}");
    assert_eq!(resolved.value, json!(true));
    assert_eq!(resolved.source, SettingSource::User);
}
