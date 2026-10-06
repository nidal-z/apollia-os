//! The journal of the configuration: one entry per change, under the origin
//! it came from, chained and signed, and never a value.

use apollia_core::config::writer::{fingerprint, update_config, ConfigEdit, KeyChange};
use apollia_core::config::{validate, PolicySettings};
use axum::http::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::api::test_support::call;
use crate::audit_journal::verify::verify_entries;
use crate::audit_journal::{HmacSigner, JournalEntry};
use crate::config_reload::tests::Rig;
use crate::config_reload::{ConfigChannels, ConfigReloadHandle};

const KEY: &[u8] = b"configuration-journal-key-0054";

async fn signed_journal(rig: &Rig) -> AuditJournalHandle {
    let dir = rig.path.parent().expect("the folder of the file");
    AuditJournalHandle::open_with_key_bytes(&dir.join("audit_journal.db"), KEY.to_vec())
        .await
        .expect("open signed journal")
}

/// A runtime started on `text`, its journal open and handed to the reloader.
async fn journaled(text: &str) -> (Rig, AuditJournalHandle) {
    let rig = Rig::on(text);
    let journal = signed_journal(&rig).await;
    rig.reload.attach_journal(journal.clone()).await;
    (rig, journal)
}

/// The runtime of `rig` started again on its file, over the same journal.
async fn started_again(rig: &Rig, journal: &AuditJournalHandle) -> ConfigReloadHandle {
    let text = std::fs::read_to_string(&rig.path).unwrap_or_default();
    let next = ConfigReloadHandle::spawn(
        rig.path.clone(),
        ConfigChannels::new(&validate(&text).unwrap_or_default()),
        PolicySettings::none(),
        &rig.state.event_sender,
    );
    next.attach_journal(journal.clone()).await;
    next
}

async fn entries(journal: &AuditJournalHandle) -> Vec<JournalEntry> {
    journal.query_run(CONFIG_RUN_ID).await
}

fn origins(entries: &[JournalEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|e| e.payload["origin"].clone())
        .collect()
}

/// Saves `text` through `PUT /api/v1/config`, declaring `origin` when given.
async fn put(rig: &Rig, text: &str, origin: Option<&str>) -> (StatusCode, Value) {
    let on_disk = std::fs::read_to_string(&rig.path).unwrap_or_default();
    let mut body = json!({ "text": text, "sha256": fingerprint(&on_disk) });
    if let Some(origin) = origin {
        body["origin"] = json!(origin);
    }
    call(rig.state.clone(), "PUT", "/api/v1/config", Some(body)).await
}

// Criterion 1: the entry names the keys, the origin and the two fingerprints,
// and nothing of the values.
#[tokio::test]
async fn the_entry_names_the_keys_the_origin_and_the_fingerprints_never_a_value() {
    // GIVEN a runtime on a file that sets one port, its journal open
    let before = "[api]\nport = 7790\n";
    let (rig, journal) = journaled(before).await;
    let baseline = entries(&journal).await.len();

    // WHEN the editor saves another port and a path only this machine knows
    let after = "[api]\nport = 7791\n\n[tools]\npython_interpreter = \"/opt/only-here/python3\"\n";
    let (status, body) = put(&rig, after, Some("editor")).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // THEN one entry is added, of the configuration kind, whose payload is
    // exactly the keys, the origin and the fingerprints of the two texts
    let all = entries(&journal).await;
    assert_eq!(all.len(), baseline + 1);
    let entry = all.last().expect("an entry");
    assert_eq!(entry.kind, JournalEntryKind::ConfigurationChanged);
    assert_eq!(
        entry.payload,
        json!({
            "event": "configuration_changed",
            "keys": ["api.port", "tools.python_interpreter"],
            "origin": "editor",
            "sha256_before": fingerprint(before),
            "sha256_after": fingerprint(after),
        })
    );
    // AND no value of either text is anywhere in it
    let written = entry.payload.to_string();
    for value in ["7790", "7791", "only-here", "python3"] {
        assert!(!written.contains(value), "{value} in {written}");
    }
    journal.shutdown().await;
}

// Criterion 2, the route: the caller declares its origin.
#[tokio::test]
async fn a_save_through_the_route_is_journaled_under_the_origin_its_caller_declares() {
    // GIVEN a runtime on a file that names its model, its journal open
    let llm = "[llm]\ndefault = \"cloud\"\n\n[[llm.backends]]\nname = \"cloud\"\ntype = \"api\"\napi_url = \"https://api.example.org/v1\"\napi_key_env = \"EXAMPLE_KEY\"\nmodel = \"m\"\n";
    let (rig, journal) = journaled(llm).await;
    let baseline = entries(&journal).await.len();

    // WHEN the editor saves, then a client that names no origin, then the
    // observability page of the settings
    let first = format!("{llm}\n[hitl]\nscan_interval_secs = 30\n");
    let second = format!("{llm}\n[hitl]\nscan_interval_secs = 31\n");
    let (editor, _) = put(&rig, &first, Some("editor")).await;
    let (unnamed, _) = put(&rig, &second, None).await;
    let (settings, body) = call(
        rig.state.clone(),
        "PUT",
        "/api/v1/config/observability",
        Some(json!({ "retention_days": 11 })),
    )
    .await;
    assert_eq!(
        (editor, unnamed, settings),
        (StatusCode::OK, StatusCode::OK, StatusCode::OK),
        "{body}"
    );
    // AND the watcher then sees the file the settings page wrote
    rig.reload.file_changed().await;

    // THEN each change has one entry, under its origin
    let all = entries(&journal).await.split_off(baseline);
    assert_eq!(
        origins(&all),
        vec![json!("editor"), json!("api"), json!("settings")]
    );
    assert_eq!(all[1].payload["keys"], json!(["hitl.scan_interval_secs"]));
    assert!(all[2].payload["keys"]
        .as_array()
        .expect("keys")
        .contains(&json!("observability.retention_days")));

    // WHEN a caller declares an origin only the runtime finds by itself
    let (refused, _) = put(&rig, llm, Some("at_startup")).await;

    // THEN the save is refused, the file is as it was, and nothing is
    // journaled
    assert_eq!(refused, StatusCode::UNPROCESSABLE_ENTITY);
    assert_ne!(std::fs::read_to_string(&rig.path).unwrap(), llm);
    assert_eq!(entries(&journal).await.len(), baseline + 3);
    journal.shutdown().await;
}

// Criterion 2, the command line: it writes the file itself, then shows the
// running runtime what it wrote.
#[tokio::test]
async fn a_write_of_the_command_line_shown_to_the_route_is_journaled_as_cli_once() {
    // GIVEN a running runtime on a file of one comment, its journal open: the
    // start recorded the file, which sets no key
    let (rig, journal) = journaled("# mine\n").await;
    let at_start = entries(&journal).await;
    assert_eq!(origins(&at_start), vec![json!("at_startup")]);
    assert_eq!(at_start[0].payload["keys"], json!([]));

    // WHEN the command line sets a key through the writer of the file, as it
    // does with or without a daemon, then presents the text it wrote
    let written = update_config(
        &rig.path,
        ConfigEdit::Keys(vec![KeyChange::set("hitl.scan_interval_secs", json!(45))]),
        &PolicySettings::none(),
    )
    .expect("the command line writes");
    let (status, body) = call(
        rig.state.clone(),
        "PUT",
        "/api/v1/config",
        Some(json!({ "text": written.text, "sha256": written.sha256, "origin": "cli" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // AND the watcher sees the same file afterwards
    rig.reload.file_changed().await;

    // THEN the file is as the command line wrote it, the change is applied,
    // and the journal holds it once, as made by the command line
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), written.text);
    assert_eq!(rig.feed().hitl.get().scan_interval_secs, 45);
    let all = entries(&journal).await.split_off(1);
    assert_eq!(origins(&all), vec![json!("cli")]);
    assert_eq!(all[0].payload["keys"], json!(["hitl.scan_interval_secs"]));
    assert_eq!(all[0].payload["sha256_before"], fingerprint("# mine\n"));
    assert_eq!(all[0].payload["sha256_after"], written.sha256);
    journal.shutdown().await;
}

// Criterion 2, the watch: a file changed from outside while the runtime runs.
#[tokio::test]
async fn a_file_changed_on_disk_is_journaled_as_an_external_file() {
    // GIVEN a running runtime, its journal open
    let (rig, journal) = journaled("").await;

    // WHEN another program rewrites the file and the watcher reads it, then
    // rewrites it with a text that does not validate
    let text = "[hitl]\nscan_interval_secs = 50\n";
    std::fs::write(&rig.path, text).unwrap();
    rig.reload.file_changed().await;
    std::fs::write(&rig.path, "[api]\nport = \"x\"\n").unwrap();
    rig.reload.file_changed().await;

    // THEN the valid change is journaled as an external file, and the refused
    // text, which changed nothing in force, is not
    let all = entries(&journal).await;
    assert_eq!(origins(&all), vec![json!("external_file")]);
    assert_eq!(all[0].payload["keys"], json!(["hitl.scan_interval_secs"]));
    assert_eq!(all[0].payload["sha256_after"], fingerprint(text));
    journal.shutdown().await;
}

// Criterion 2, the start: the file changed while the runtime was stopped.
#[tokio::test]
async fn a_file_changed_during_a_stop_is_journaled_at_the_next_start_with_its_keys() {
    // GIVEN a runtime that applied a text, then stopped
    let (rig, journal) = journaled("").await;
    let applied = "[hitl]\nscan_interval_secs = 30\n";
    put(&rig, applied, Some("editor")).await;

    // WHEN the command line, with no daemon to show it to, writes the file,
    // and the runtime starts again
    let offline = update_config(
        &rig.path,
        ConfigEdit::Keys(vec![KeyChange::set("api.port", json!(7790))]),
        &PolicySettings::none(),
    )
    .expect("the command line writes");
    started_again(&rig, &journal).await;

    // THEN the start records the change: the key, and the fingerprints of the
    // last text applied and of the file found
    let all = entries(&journal).await;
    assert_eq!(origins(&all), vec![json!("editor"), json!("at_startup")]);
    assert_eq!(
        all[1].payload,
        json!({
            "event": "configuration_changed",
            "keys": ["api.port"],
            "origin": "at_startup",
            "sha256_before": fingerprint(applied),
            "sha256_after": offline.sha256,
        })
    );

    // WHEN it starts once more on the same file
    started_again(&rig, &journal).await;

    // THEN nothing is added
    assert_eq!(entries(&journal).await.len(), 2);
    journal.shutdown().await;
}

#[tokio::test]
async fn the_first_start_on_a_file_measures_it_from_the_defaults() {
    // GIVEN a file that sets one key, and a journal that never saw it
    // WHEN the runtime starts
    let text = "[api]\nport = 7790\n";
    let (_rig, journal) = journaled(text).await;

    // THEN the start records the key that departs from the defaults
    let all = entries(&journal).await;
    assert_eq!(origins(&all), vec![json!("at_startup")]);
    assert_eq!(all[0].payload["keys"], json!(["api.port"]));
    assert_eq!(all[0].payload["sha256_before"], fingerprint(""));
    assert_eq!(all[0].payload["sha256_after"], fingerprint(text));
    journal.shutdown().await;
}

#[tokio::test]
async fn a_start_on_a_file_without_a_text_or_on_a_refused_one_records_nothing() {
    // GIVEN an empty file
    // WHEN the runtime starts
    let (_empty, journal) = journaled("").await;
    // THEN nothing is recorded: the configuration is the defaults
    assert!(entries(&journal).await.is_empty());
    journal.shutdown().await;

    // GIVEN a file that does not validate
    // WHEN the runtime starts on it
    let (_refused, journal) = journaled("[api]\nport = \"x\"\n").await;
    // THEN nothing is recorded: the file is not applied
    assert!(entries(&journal).await.is_empty());
    journal.shutdown().await;
}

#[tokio::test]
async fn a_change_during_a_stop_says_its_keys_are_not_known_when_the_copy_was_altered() {
    // GIVEN a runtime that applied a text, then stopped, and a copy of that
    // text which no longer has the fingerprint the journal recorded
    let (rig, journal) = journaled("").await;
    put(&rig, "[hitl]\nscan_interval_secs = 30\n", Some("editor")).await;
    std::fs::write(applied_copy_path(&rig.path), "[api]\nport = 1\n").unwrap();

    // WHEN the file changes during the stop and the runtime starts again
    std::fs::write(&rig.path, "[hitl]\nscan_interval_secs = 31\n").unwrap();
    started_again(&rig, &journal).await;

    // THEN the change is recorded with its fingerprints and without keys,
    // rather than with keys measured from a text nobody vouches for
    let all = entries(&journal).await;
    assert_eq!(origins(&all), vec![json!("editor"), json!("at_startup")]);
    assert!(all[1].payload.get("keys").is_none(), "{}", all[1].payload);
    journal.shutdown().await;
}

// Criterion 3: one entry of each origin, chained and signed like the other
// natures recorded outside a run.
#[tokio::test]
async fn an_entry_of_each_origin_is_chained_and_signed() {
    // GIVEN a signed journal
    let (rig, journal) = journaled("").await;

    // WHEN one change of each origin is recorded
    let all_origins = [
        ConfigOrigin::Editor,
        ConfigOrigin::Settings,
        ConfigOrigin::Cli,
        ConfigOrigin::Api,
        ConfigOrigin::ExternalFile,
        ConfigOrigin::AtStartup,
    ];
    let mut before = fingerprint("");
    for (n, origin) in all_origins.into_iter().enumerate() {
        let after = fingerprint(&format!("[hitl]\nscan_interval_secs = {}\n", 30 + n));
        let record = ConfigChangeRecord::new(
            Some(vec!["hitl.scan_interval_secs".into()]),
            origin,
            before,
            after.clone(),
        );
        append(&journal, &record);
        before = after;
    }

    // THEN the run holds the six, in order, each of the configuration kind
    let run = entries(&journal).await;
    assert_eq!(
        origins(&run),
        vec![
            json!("editor"),
            json!("settings"),
            json!("cli"),
            json!("api"),
            json!("external_file"),
            json!("at_startup"),
        ]
    );
    assert!(run
        .iter()
        .all(|e| e.kind == JournalEntryKind::ConfigurationChanged));
    for entry in &run {
        let back: ConfigChangeRecord =
            serde_json::from_value(entry.payload.clone()).expect("payload round trip");
        assert_eq!(back.sha256_after.len(), 64);
    }
    // AND the run verifies with signatures required, and so does the journal
    assert!(run.iter().all(|e| e.signature.is_some()));
    let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
    let report = verify_entries(CONFIG_RUN_ID, &run, Some(&signer), true);
    assert!(report.ok, "{report:?}");
    let whole = journal.verify_journal().await.expect("verify journal");
    assert!(whole.ok);
    assert_eq!(whole.entries_checked, 6);
    // AND the last fingerprint recorded is the one a start compares with
    assert_eq!(last_applied(&journal).await, Some(before));
    drop(rig);
    journal.shutdown().await;
}
