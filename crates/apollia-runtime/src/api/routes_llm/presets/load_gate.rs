//! Whether a preset is written with load settings.
//!
//! `[models] preset_load_settings` is off by default: a preset is then
//! created and modified with request settings only. A write that carries load
//! settings is refused by an error that names the setting, and a modification
//! keeps the load settings the preset already holds. On, the load settings
//! are written as sent.

use std::path::Path;

use apollia_core::config::{resolve_setting, PolicySettings, PRESET_LOAD_SETTINGS_KEY};

/// The load settings of a preset, as the routes carry them.
pub(super) type LoadSettings = serde_json::Map<String, serde_json::Value>;

/// A write of load settings refused because the setting is off.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "load settings are not written to a preset while {key} is off: \
     send request settings only, or turn the setting on"
)]
pub(super) struct LoadSettingsOff {
    /// The dotted key of the setting to turn on.
    pub key: &'static str,
}

/// Whether the settings in force let a preset carry load settings. Without a
/// configuration file, or when it cannot be read, the default applies: off.
pub(super) fn load_settings_allowed(config_file: Option<&Path>, imposed: &PolicySettings) -> bool {
    let Some(path) = config_file else {
        return false;
    };
    match resolve_setting(path, PRESET_LOAD_SETTINGS_KEY, imposed) {
        Ok(resolved) => resolved.value.as_bool().unwrap_or(false),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                key = PRESET_LOAD_SETTINGS_KEY,
                error = %error,
                detail = "the default applies",
                "llm.preset.setting_unreadable"
            );
            false
        }
    }
}

/// The load settings a write stores.
///
/// `allowed`, they are the ones sent. Otherwise the ones the preset already
/// holds (`existing`, none for a creation) are kept, and a body that sends
/// others is refused.
///
/// # Errors
///
/// [`LoadSettingsOff`] when the setting is off and the body carries load
/// settings that are not the ones already held.
pub(super) fn stored_load_settings(
    allowed: bool,
    sent: LoadSettings,
    existing: Option<&LoadSettings>,
) -> Result<LoadSettings, LoadSettingsOff> {
    if allowed {
        return Ok(sent);
    }
    let held = existing.cloned().unwrap_or_default();
    if sent.is_empty() || sent == held {
        Ok(held)
    } else {
        Err(LoadSettingsOff {
            key: PRESET_LOAD_SETTINGS_KEY,
        })
    }
}
