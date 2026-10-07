//! The configuration reloader: what a save applies, what waits for a
//! restart, and what a change on disk does.

use std::path::PathBuf;
use std::time::Duration;

use apollia_core::config::writer::fingerprint;
use apollia_core::config::{validate, PolicySettings};
use apollia_core::RuntimeEvent;
use axum::http::StatusCode;
use serde_json::{json, Value};
use tempfile::TempDir;

use super::{ConfigChannels, ConfigFeed, ConfigReloadHandle};
use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

/// A runtime started on an `apollia.toml`, following it.
pub(crate) struct Rig {
    _dir: TempDir,
    pub(crate) path: PathBuf,
    pub(crate) state: AppState<MockBackend>,
    pub(crate) reload: ConfigReloadHandle,
}

impl Rig {
    /// A runtime started on a file holding `text`.
    pub(crate) fn on(text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, text).unwrap();
        let mut state = app_state(dir.path().to_path_buf());
        state.config_path = Some(path.clone());
        let channels = ConfigChannels::new(&validate(text).unwrap_or_default());
        let reload = ConfigReloadHandle::spawn(
            path.clone(),
            channels,
            PolicySettings::none(),
            &state.event_sender,
        );
        state.config_reload = Some(reload.clone());
        Self {
            _dir: dir,
            path,
            state,
            reload,
        }
    }

    /// The live sections the consumers hold.
    pub(crate) fn feed(&self) -> ConfigFeed {
        self.reload.feed()
    }

    /// Saves `text` through `PUT /api/v1/config`, as the editor does.
    pub(crate) async fn save(&self, text: &str) -> Value {
        let on_disk = std::fs::read_to_string(&self.path).unwrap_or_default();
        let (status, body) = call(
            self.state.clone(),
            "PUT",
            "/api/v1/config",
            Some(json!({ "text": text, "sha256": fingerprint(&on_disk) })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Reads the configuration through `GET /api/v1/config`.
    pub(crate) async fn read(&self) -> Value {
        let (status, body) = call(self.state.clone(), "GET", "/api/v1/config", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}

/// Waits for `condition`, polling, at most ten seconds.
async fn until(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    condition()
}

#[tokio::test]
async fn the_observability_section_saved_by_the_route_is_read_at_the_next_request() {
    // GIVEN a runtime started on a file that keeps the default truncation
    let rig = Rig::on("");
    let coordinator_view = rig.state.observability_live();

    // WHEN the route saves a new input limit
    rig.save("[observability]\nmax_input_bytes = 1234\n").await;

    // THEN the next request reads it, and so does the coordinator of an agent
    // loaded before the save, without a restart
    assert_eq!(rig.state.observability().max_input_bytes, 1234);
    assert_eq!(coordinator_view.get().max_input_bytes, 1234);
}

#[tokio::test]
async fn the_desktop_section_saved_by_the_route_reaches_its_channel() {
    // GIVEN a runtime started with the link preview off
    let rig = Rig::on("");
    let mut desktop = rig.feed().desktop;
    assert!(!desktop.get().link_preview_enabled);

    // WHEN the route turns it on
    rig.save("[desktop]\nlink_preview_enabled = true\n").await;

    // THEN the section channel carries the new value once
    assert!(desktop.changed().expect("a change").link_preview_enabled);
    assert!(desktop.changed().is_none());
}

#[tokio::test]
async fn a_restart_key_changed_by_the_route_is_listed_with_its_line_until_the_next_start() {
    // GIVEN a runtime started on the default port
    let rig = Rig::on("# mine\n");

    // WHEN the route moves the port and changes a reloadable key beside it
    let saved = rig
        .save("# mine\n[api]\nport = 7790\n\n[hitl]\nscan_interval_secs = 30\n")
        .await;
    let read = rig.read().await;

    // THEN the port waits for a restart at its line, in the answer of the save
    // and in the read, and the reloadable key does not
    let expected = json!([{ "key": "api.port", "line": 3 }]);
    assert_eq!(saved["pendingRestart"], expected, "{saved}");
    assert_eq!(read["pendingRestart"], expected, "{read}");

    // WHEN the runtime starts again on the saved file
    let next = ConfigReloadHandle::spawn(
        rig.path.clone(),
        ConfigChannels::new(&validate(&std::fs::read_to_string(&rig.path).unwrap()).unwrap()),
        PolicySettings::none(),
        &rig.state.event_sender,
    );

    // THEN nothing waits any more
    assert!(next.status().await.pending_restart.is_empty());
}

#[tokio::test]
async fn a_key_set_back_to_its_start_value_no_longer_waits() {
    // GIVEN a runtime whose port change waits for a restart
    let rig = Rig::on("");
    rig.save("[api]\nport = 7790\n").await;

    // WHEN the port is set back to the value the runtime started with
    let saved = rig.save("").await;

    // THEN nothing waits
    assert_eq!(saved["pendingRestart"], json!([]), "{saved}");
}

#[tokio::test]
async fn each_application_publishes_the_sections_applied_and_the_keys_waiting() {
    // GIVEN a runtime and a reader of the bus
    let rig = Rig::on("");
    let mut bus = rig.state.event_sender.subscribe();

    // WHEN the route saves a tool setting and a new port
    rig.save("[api]\nport = 7790\n\n[tools]\nmax_output_chars = 900\n")
        .await;

    // THEN ConfigReloaded, sent before the save answered, names the tools
    // section applied and the port waiting
    let event = std::iter::from_fn(|| bus.try_recv().ok())
        .find_map(|event| match event {
            RuntimeEvent::ConfigReloaded {
                sections,
                pending_restart,
            } => Some((sections, pending_restart)),
            _ => None,
        })
        .expect("ConfigReloaded");
    assert_eq!(event.0, vec!["tools".to_owned()]);
    assert_eq!(event.1, vec!["api.port".to_owned()]);
}

#[tokio::test]
async fn a_save_of_the_application_is_applied_once_although_the_file_changed() {
    // GIVEN a runtime following its file
    let rig = Rig::on("");

    // WHEN the route saves a text, then the watcher reports the change it made
    // on disk
    rig.save("[hitl]\nscan_interval_secs = 70\n").await;
    rig.reload.file_changed().await;

    // THEN the text was applied once
    assert_eq!(rig.reload.status().await.applied, 1);
}

#[tokio::test]
async fn a_valid_file_changed_outside_the_application_is_applied_like_a_save() {
    // GIVEN a runtime watching its file
    let rig = Rig::on("");
    rig.reload.watch_file(rig.path.clone());
    let hitl = rig.feed().hitl;
    // The folder watch is armed asynchronously on some platforms.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // WHEN another editor writes a valid file, without the writer
    std::fs::write(&rig.path, "[hitl]\ntimeout_hours = 3\n").unwrap();

    // THEN once the quiet period has passed the new section is in force
    assert!(until(|| hitl.get().timeout_hours == Some(3)).await);
    assert_eq!(rig.reload.status().await.applied, 1);
}

#[tokio::test]
async fn an_invalid_file_changed_outside_keeps_the_last_valid_configuration() {
    // GIVEN a runtime on a valid file, and a reader of the bus
    let rig = Rig::on("[hitl]\ntimeout_hours = 2\n");
    let mut bus = rig.state.event_sender.subscribe();

    // WHEN another editor leaves a wrong type on line 2, and the watcher says so
    std::fs::write(&rig.path, "[hitl]\ntimeout_hours = \"two\"\n").unwrap();
    rig.reload.file_changed().await;

    // THEN the last valid section stays in force, ConfigInvalid is published
    // with the fault at its line, and the read route shows the fault
    assert_eq!(rig.feed().hitl.get().timeout_hours, Some(2));
    let status = rig.reload.status().await;
    assert_eq!(status.applied, 0);
    assert_eq!(status.diagnostics[0].line, 2);
    let invalid = std::iter::from_fn(|| bus.try_recv().ok())
        .find_map(|event| match event {
            RuntimeEvent::ConfigInvalid { diagnostics, .. } => Some(diagnostics),
            _ => None,
        })
        .expect("ConfigInvalid");
    assert_eq!(invalid[0].line, 2);
    let read = rig.read().await;
    assert_eq!(read["diagnostics"][0]["line"], 2, "{read}");
}

#[test]
fn every_live_section_has_reloadable_keys_and_no_restart_key() {
    // GIVEN the sections broadcast on a channel
    let sections = super::LIVE_SECTIONS;

    // WHEN the key registry is read for each of them
    let restart: Vec<&str> = apollia_core::config::keys::KEYS
        .iter()
        .filter(|k| k.withdrawn.is_none())
        .filter(|k| sections.contains(&super::diff::section_of(k.path)))
        .filter(|k| k.reload != apollia_core::config::keys::ReloadClass::Reloadable)
        .map(|k| k.path)
        .collect();

    // THEN none of their keys waits for a restart: a channel is never sent a
    // value the engine cannot apply
    assert!(restart.is_empty(), "{restart:?}");
}
