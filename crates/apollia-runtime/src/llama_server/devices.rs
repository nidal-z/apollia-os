//! The graphics devices a launch is given: every device the engine lists,
//! less the ones the settings hold back (`[llm.devices] disabled`).
//!
//! The setting is read at every load and at every read of the hardware, then
//! laid on the hardware probe, so the estimate that sizes a model and the
//! `--device` list the engine is launched with never disagree. Nothing held
//! back leaves the command line as it always was; every device held back
//! launches on the processor alone, `--device none` and `-ngl 0`.

use std::path::Path;

use apollia_core::config::{resolve_setting, PolicySettings, DISABLED_DEVICES_KEY};
use apollia_llm::hardware::GpuDevice;

use super::LlamaServerConfig;

/// The identifiers held back now, read from `config_file` under `imposed`.
/// A missing file holds nothing back; an unreadable one neither, with a
/// warning, so a typo never keeps a model from loading.
#[must_use]
pub fn read_disabled(config_file: Option<&Path>, imposed: &PolicySettings) -> Vec<String> {
    let Some(path) = config_file else {
        return Vec::new();
    };
    match resolve_setting(path, DISABLED_DEVICES_KEY, imposed) {
        Ok(resolved) => resolved
            .value
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %error,
                detail = "the engine keeps every device",
                "llama.server.devices.unreadable"
            );
            Vec::new()
        }
    }
}

/// Read the devices held back and lay them on the hardware probe, so every
/// estimate made after this call leaves their memory out.
///
/// Blocking: it reads a file. Call it off the async workers.
pub fn sync_selection(config_file: Option<&Path>, imposed: &PolicySettings) {
    let disabled = read_disabled(config_file, imposed);
    apollia_llm::hardware::set_disabled_devices(&disabled);
}

/// The `--device` list of a launch on `devices`: `None` when nothing is held
/// back, which leaves the choice to the engine as before; otherwise the
/// identifiers left on, empty when every device is held back.
#[must_use]
pub(super) fn launch_list(devices: &[GpuDevice]) -> Option<Vec<String>> {
    if devices.iter().all(|device| device.enabled) {
        return None;
    }
    Some(
        devices
            .iter()
            .filter(|device| device.enabled)
            .filter_map(|device| device.id.clone())
            .collect(),
    )
}

/// Lay the selection on `config`. Every device held back also sets the
/// offload count to zero, whatever a preset or the plan asked for: there is
/// nothing left to offload to.
pub(super) fn apply(config: &mut LlamaServerConfig, devices: &[GpuDevice]) {
    config.devices = launch_list(devices);
    if config.devices.as_ref().is_some_and(Vec::is_empty) {
        config.n_gpu_layers = 0;
    }
}

/// Append the `--device` argument of `selection`, when there is one.
pub(super) fn push_args(selection: Option<&[String]>, args: &mut Vec<String>) {
    let Some(ids) = selection else {
        return;
    };
    args.push("--device".to_owned());
    args.push(if ids.is_empty() {
        "none".to_owned()
    } else {
        ids.join(",")
    });
}

#[cfg(test)]
mod tests {
    use super::super::config::build_args;
    use super::*;

    fn card(id: &str, enabled: bool) -> GpuDevice {
        GpuDevice {
            id: Some(id.to_owned()),
            name: format!("card {id}"),
            api: "CUDA".to_owned(),
            vram_gb: 24.0,
            enabled,
        }
    }

    /// The value following `flag` in `args`.
    fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let at = args.iter().position(|a| a == flag)?;
        args.get(at + 1).map(String::as_str)
    }

    #[test]
    fn test_nothing_held_back_leaves_the_command_line_as_it_was() {
        // GIVEN two cards, both on
        let mut config = LlamaServerConfig::default();

        // WHEN the selection is laid on the launch
        apply(&mut config, &[card("CUDA0", true), card("CUDA1", true)]);
        let args = build_args(&config, 8080);

        // THEN no `--device` is passed and the offload count is untouched
        assert!(!args.iter().any(|a| a == "--device"), "{args:?}");
        assert_eq!(value_of(&args, "-ngl"), Some("999"));
    }

    #[test]
    fn test_a_card_held_back_is_left_out_of_the_device_list_beside_ngl() {
        // GIVEN three cards, the second held back
        let mut config = LlamaServerConfig::default();
        let devices = [
            card("CUDA0", true),
            card("CUDA1", false),
            card("CUDA2", true),
        ];

        // WHEN the selection is laid on the launch
        apply(&mut config, &devices);
        let args = build_args(&config, 8080);

        // THEN the engine is given the two others, and still offloads
        assert_eq!(value_of(&args, "--device"), Some("CUDA0,CUDA2"));
        assert_eq!(value_of(&args, "-ngl"), Some("999"));
    }

    #[test]
    fn test_every_card_held_back_launches_on_the_processor_alone() {
        // GIVEN two cards, both held back, and a launch that asked for layers
        let mut config = LlamaServerConfig {
            n_gpu_layers: 33,
            ..LlamaServerConfig::default()
        };

        // WHEN the selection is laid on the launch
        apply(&mut config, &[card("CUDA0", false), card("CUDA1", false)]);
        let args = build_args(&config, 8080);

        // THEN it is `-ngl 0`, and the engine is told to use no device
        assert_eq!(value_of(&args, "-ngl"), Some("0"));
        assert_eq!(value_of(&args, "--device"), Some("none"));
    }

    #[test]
    fn test_the_held_back_devices_are_read_from_the_settings_file() {
        // GIVEN a settings file naming one device, and a machine without one
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[llm.devices]\ndisabled = [\"CUDA1\"]\n").unwrap();

        // WHEN both are read
        let named = read_disabled(Some(&path), &PolicySettings::none());
        let absent = read_disabled(Some(&dir.path().join("none.toml")), &PolicySettings::none());

        // THEN the file's device is held back, and nothing without a file
        assert_eq!(named, vec!["CUDA1".to_owned()]);
        assert!(absent.is_empty());
    }

    #[test]
    fn test_a_list_the_policy_imposes_wins_over_the_file() {
        // GIVEN a file holding nothing back, and a policy reserving a card
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[llm.devices]\ndisabled = []\n").unwrap();
        let policy =
            PolicySettings::none().impose(DISABLED_DEVICES_KEY, serde_json::json!(["CUDA0"]));

        // WHEN the list is read
        let disabled = read_disabled(Some(&path), &policy);

        // THEN the imposed list is the one in force
        assert_eq!(disabled, vec!["CUDA0".to_owned()]);
    }

    #[test]
    fn test_an_unreadable_file_holds_nothing_back() {
        // GIVEN a file that is not valid TOML
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[llm.devices\ndisabled = ").unwrap();

        // WHEN the list is read
        let disabled = read_disabled(Some(&path), &PolicySettings::none());

        // THEN the engine keeps every device
        assert!(disabled.is_empty());
    }
}
