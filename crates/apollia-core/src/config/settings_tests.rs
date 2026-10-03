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
