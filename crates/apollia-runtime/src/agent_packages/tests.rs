use tempfile::TempDir;

use super::install::inject_triggers;
use super::*;

#[test]
fn test_a_name_that_could_leave_its_folder_is_refused() {
    // GIVEN plain names and names that are paths
    let plain = ["veille", "web-search_worker.v2"];
    let paths = ["", ".", "..", "a/b", "a\\b", &"x".repeat(MAX_NAME_LEN + 1)];

    // WHEN each is checked
    let accepted: Vec<bool> = plain.iter().map(|n| check_name(n).is_ok()).collect();
    let refused: Vec<bool> = paths.iter().map(|n| check_name(n).is_err()).collect();

    // THEN only the plain names pass
    assert!(accepted.iter().all(|ok| *ok));
    assert!(refused.iter().all(|err| *err), "{refused:?}");
}

#[test]
fn test_a_webhook_without_a_secret_is_reported_and_the_others_written() {
    // GIVEN a manifest with a cron trigger and a webhook no one gave a secret
    let dir = TempDir::new().unwrap();
    let toml = r#"
[[triggers]]
id = "daily"
agent = "a"
[triggers.source]
type = "cron"
schedule = "0 7 * * *"

[[triggers]]
id = "inbound"
agent = "a"
[triggers.source]
type = "webhook"
"#;

    // WHEN its triggers are written with no configuration
    let (count, errors) = inject_triggers(dir.path(), toml, &[]);

    // THEN the cron trigger is written and the webhook named as refused
    assert_eq!(count, 1);
    assert_eq!(
        errors,
        vec!["trigger 'inbound': webhook secret is required but not provided".to_owned()]
    );
}

#[test]
fn test_a_manifest_that_breaks_an_invariant_previews_as_invalid() {
    // GIVEN a folder whose manifest declares no agent
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("agent.toml"),
        "[package]\nname = \"empty\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    // WHEN it is previewed
    let preview = preview(dir.path()).unwrap();

    // THEN the preview is marked invalid with the reason, and lists nothing
    assert!(!preview.valid);
    assert!(preview.error.unwrap().contains("no agent"));
    assert!(preview.agents.is_empty());
}
