//! The one writer of `apollia.toml`: one test per check it runs.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::writer::{
    check_text, default_text_under, fingerprint, read_snapshot, update_config, write_config,
    ConfigEdit, ConfigWriteError, KeyChange,
};
use super::{validate, PolicySettings};

fn file_with(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn developer_mode_imposed() -> PolicySettings {
    PolicySettings::none().impose("desktop.developer_mode", json!(false))
}

#[test]
fn a_valid_text_is_written_and_its_fingerprint_returned() {
    // GIVEN a file and its fingerprint
    let (_dir, path) = file_with("# mine\n[api]\nport = 7771\n");
    let before = read_snapshot(&path).unwrap();

    // WHEN a valid text is saved against that fingerprint
    let text = "# mine\n[api]\nport = 7772\n";
    let written = write_config(
        &path,
        ConfigEdit::Text(text.to_owned()),
        &before.sha256,
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the file holds the text, and the returned fingerprint is the new one
    assert_eq!(read(&path), text);
    assert_eq!(written.sha256, fingerprint(text));
    assert_ne!(written.sha256, before.sha256);
}

#[test]
fn an_invalid_text_is_refused_at_its_line_and_the_file_is_unchanged() {
    // GIVEN a valid file
    let original = "[api]\nport = 7771\n";
    let (_dir, path) = file_with(original);
    let sha = fingerprint(original);

    // WHEN a text with an unknown key on line 3 is saved
    let refused = write_config(
        &path,
        ConfigEdit::Text("[api]\nport = 7771\ntypo = 1\n".to_owned()),
        &sha,
        &PolicySettings::none(),
    );

    // THEN it is refused with a diagnostic at line 3, and the file is unchanged
    match refused {
        Err(ConfigWriteError::Invalid { diagnostics, .. }) => {
            assert_eq!(diagnostics[0].line, 3, "{diagnostics:?}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(read(&path), original);
}

#[test]
fn a_value_other_than_the_imposed_one_is_refused_with_its_line_and_key() {
    // GIVEN a policy that imposes the developer mode off, and an empty file
    let (_dir, path) = file_with("");
    let policy = developer_mode_imposed();

    // WHEN a text turns it on, on line 2
    let refused = write_config(
        &path,
        ConfigEdit::Text("[desktop]\ndeveloper_mode = true\n".to_owned()),
        &fingerprint(""),
        &policy,
    );

    // THEN the writer refuses it, naming line 2 and the key, and writes nothing
    match refused {
        Err(ConfigWriteError::LockedByPolicy { line, key, imposed }) => {
            assert_eq!(line, 2);
            assert_eq!(key, "desktop.developer_mode");
            assert_eq!(imposed, json!(false));
        }
        other => panic!("expected LockedByPolicy, got {other:?}"),
    }
    assert_eq!(read(&path), "");
}

#[test]
fn the_imposed_value_itself_is_accepted() {
    // GIVEN a policy that imposes the developer mode off
    let (_dir, path) = file_with("");
    let policy = developer_mode_imposed();

    // WHEN a text writes the imposed value
    let text = "[desktop]\ndeveloper_mode = false\n";
    let written = write_config(
        &path,
        ConfigEdit::Text(text.to_owned()),
        &fingerprint(""),
        &policy,
    );

    // THEN it is written
    assert!(written.is_ok(), "{written:?}");
    assert_eq!(read(&path), text);
}

#[test]
fn a_key_change_to_a_locked_key_is_refused_too() {
    // GIVEN a policy that imposes the developer mode off
    let (_dir, path) = file_with("# mine\n");
    let policy = developer_mode_imposed();

    // WHEN a key edit turns it on, then another writes the imposed value
    let refused = update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("desktop.developer_mode", json!(true))]),
        &policy,
    );
    let same = update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("desktop.developer_mode", json!(false))]),
        &policy,
    );

    // THEN both are refused, the setting being locked, and the file is unchanged
    assert!(
        matches!(refused, Err(ConfigWriteError::LockedByPolicy { .. })),
        "{refused:?}"
    );
    assert!(
        matches!(same, Err(ConfigWriteError::LockedByPolicy { .. })),
        "{same:?}"
    );
    assert_eq!(read(&path), "# mine\n");
}

#[test]
fn a_file_modified_since_it_was_read_is_not_overwritten() {
    // GIVEN a file read by a first writer, then changed by a second one
    let (_dir, path) = file_with("[api]\nport = 7771\n");
    let read_first = read_snapshot(&path).unwrap();
    std::fs::write(&path, "[api]\nport = 7773\n").unwrap();

    // WHEN the first writer saves against its reading
    let refused = write_config(
        &path,
        ConfigEdit::Text("[api]\nport = 7772\n".to_owned()),
        &read_first.sha256,
        &PolicySettings::none(),
    );

    // THEN it is refused as modified in the meantime, and the second change stays
    match refused {
        Err(e @ ConfigWriteError::Modified { .. }) => {
            assert!(e.to_string().contains("modified in the meantime"), "{e}");
        }
        other => panic!("expected Modified, got {other:?}"),
    }
    assert_eq!(read(&path), "[api]\nport = 7773\n");
}

#[test]
fn the_write_goes_through_a_sibling_file_renamed_over_the_original() {
    // GIVEN a file whose folder holds nothing else
    let (dir, path) = file_with("[api]\nport = 7771\n");
    let inode_before = std::fs::metadata(&path).unwrap();

    // WHEN a key is changed
    update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("api.port", json!(7772))]),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the folder holds the file alone (the staging file was renamed), and
    // the file is a new one rather than the old one written in place
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["apollia.toml".to_owned()]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let inode_after = std::fs::metadata(&path).unwrap();
        assert_ne!(inode_before.ino(), inode_after.ino());
    }
    #[cfg(not(unix))]
    let _ = inode_before;
}

#[test]
fn a_key_change_keeps_the_comments_and_the_order_of_the_file() {
    // GIVEN a file with comments and two sections in a chosen order
    let original = "# my settings\n[tools]\n# my interpreter\npython_interpreter = \"/a\"\n\n[api]\nport = 7771 # fixed\n";
    let (_dir, path) = file_with(original);

    // WHEN the port changes
    update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("api.port", json!(7772))]),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN only the value changed
    assert_eq!(
        read(&path),
        original.replace("port = 7771", "port = 7772"),
        "comments and order are kept"
    );
}

#[test]
fn a_key_change_creates_the_tables_it_needs() {
    // GIVEN an empty file
    let (_dir, path) = file_with("");

    // WHEN a key two tables deep is written
    update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set(
            "tools.web_search.backend",
            json!("brave"),
        )]),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the table is declared and holds the key
    let after = read(&path);
    assert!(after.contains("[tools.web_search]"), "{after}");
    assert!(after.contains("backend = \"brave\""), "{after}");
}

#[test]
fn a_removed_key_leaves_the_rest_of_its_table() {
    // GIVEN a file that names an interpreter
    let (_dir, path) = file_with("[tools]\n# keep\npython_interpreter = \"/a\"\n");

    // WHEN the key is removed
    update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::remove("tools.python_interpreter")]),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the key is gone and the table stays
    let after = read(&path);
    assert!(!after.contains("python_interpreter"), "{after}");
    assert!(after.contains("[tools]"), "{after}");
}

#[test]
fn an_unknown_or_withdrawn_key_is_refused_by_name() {
    // GIVEN an empty file
    let (_dir, path) = file_with("");

    // WHEN an unknown key, then a withdrawn one, is changed
    let unknown = update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("api.typo", json!(1))]),
        &PolicySettings::none(),
    );
    let withdrawn = update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("triggers.x", json!(1))]),
        &PolicySettings::none(),
    );

    // THEN both are refused with the messages of the command line
    assert_eq!(
        unknown.unwrap_err().to_string(),
        "unknown config key: 'api.typo'"
    );
    assert!(withdrawn
        .unwrap_err()
        .to_string()
        .starts_with("'triggers.x' is a withdrawn setting:"));
    assert_eq!(read(&path), "");
}

#[test]
fn a_key_edit_may_leave_a_section_incomplete() {
    // GIVEN an empty file
    let (_dir, path) = file_with("");

    // WHEN `llm.default` is written before any backend
    let written = update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("llm.default", json!("local"))]),
        &PolicySettings::none(),
    );

    // THEN it is written: completeness is the validation's report, not a refusal
    assert!(written.is_ok(), "{written:?}");
    assert!(read(&path).contains("default = \"local\""));
}

#[test]
fn an_absent_file_is_created_against_the_fingerprint_of_an_empty_text() {
    // GIVEN no file
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("apollia.toml");

    // WHEN a text is saved against the fingerprint of the empty reading
    let snapshot = read_snapshot(&path).unwrap();
    assert!(!snapshot.exists);
    write_config(
        &path,
        ConfigEdit::Text("[api]\nport = 7772\n".to_owned()),
        &snapshot.sha256,
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the file exists with the text
    assert_eq!(read(&path), "[api]\nport = 7772\n");
}

#[test]
fn the_default_text_keeps_the_imposed_keys_and_is_accepted_by_the_writer() {
    // GIVEN a policy that imposes the developer mode on
    let policy = PolicySettings::none().impose("desktop.developer_mode", json!(true));

    // WHEN the default text is asked for under it
    let text = default_text_under(&policy);

    // THEN it validates, carries the imposed value, and passes the writer's checks
    assert!(validate(&text).is_ok());
    assert!(text.contains("developer_mode = true"), "{text}");
    assert!(check_text(&text, &policy).is_empty());
}

#[test]
fn check_text_reports_a_locked_line_as_a_diagnostic() {
    // GIVEN a policy that imposes the developer mode off
    let policy = developer_mode_imposed();

    // WHEN a text that turns it on is checked
    let diagnostics = check_text("[desktop]\ndeveloper_mode = true\n", &policy);

    // THEN one diagnostic names line 2 and the key
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 2);
    assert_eq!(diagnostics[0].path, "desktop.developer_mode");
}
