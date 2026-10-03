//! The engine log level the settings choose, laid on the launch configuration
//! under the environment.
//!
//! The order is fixed: `APOLLIA_LLAMA_LOG_VERBOSITY` wins, then
//! `[engines] log_level` of `apollia.toml` (or the value the organisation
//! policy imposes on it), then the default, debug (`-lv 4`). The setting is
//! read at every load, so a change applies when a model is next loaded or
//! reloaded.

use std::path::Path;

use apollia_core::config::{resolve_setting, EngineLogLevel, PolicySettings};

use super::LlamaServerConfig;

/// The dotted key of the setting.
pub(super) const LOG_LEVEL_SETTING: &str = "engines.log_level";

/// The engine log level in force now, read from `config_file` under
/// `imposed`. A missing file is the default; an unreadable one is the
/// default too, with a warning, so a typo never keeps a model from loading.
pub(super) fn read(config_file: Option<&Path>, imposed: &PolicySettings) -> EngineLogLevel {
    let Some(path) = config_file else {
        return EngineLogLevel::default();
    };
    match resolve_setting(path, LOG_LEVEL_SETTING, imposed) {
        Ok(resolved) => resolved
            .value
            .as_str()
            .and_then(EngineLogLevel::from_wire)
            .unwrap_or_default(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %error,
                detail = "the engine logs at the default level",
                "llama.server.log_level.unreadable"
            );
            EngineLogLevel::default()
        }
    }
}

/// Lays `level` on `config` as its `-lv` threshold. The environment variable,
/// applied after, overrides it.
pub(super) fn apply(config: &mut LlamaServerConfig, level: EngineLogLevel) {
    config.log_verbosity = Some(level.llama_verbosity());
}

#[cfg(test)]
mod tests {
    use super::super::load_settings::{compose, LoadSettings};
    use super::*;

    /// The `-lv` threshold a launch gets with `file` as `apollia.toml` and
    /// `env` as the value of `APOLLIA_LLAMA_LOG_VERBOSITY`.
    fn launched_verbosity(file: Option<&str>, env: Option<&str>) -> Option<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        if let Some(content) = file {
            std::fs::write(&path, content).unwrap();
        }
        let mut base = LlamaServerConfig::default();
        apply(
            &mut base,
            read(file.map(|_| path.as_path()), &PolicySettings::none()),
        );
        let env = env.map(str::to_owned);
        let config = compose(
            &base,
            &LoadSettings::default(),
            |name| {
                (name == "APOLLIA_LLAMA_LOG_VERBOSITY")
                    .then(|| env.clone())
                    .flatten()
            },
            |_| {},
            |_| {},
        );
        config.log_verbosity
    }

    #[test]
    fn test_the_variable_wins_over_the_setting_which_wins_over_the_default() {
        // GIVEN a setting asking for warnings, with and without the variable,
        // and no setting at all
        let setting = "[engines]\nlog_level = \"warning\"\n";

        // WHEN each launch configuration is composed
        let variable_and_setting = launched_verbosity(Some(setting), Some("1"));
        let setting_alone = launched_verbosity(Some(setting), None);
        let nothing = launched_verbosity(None, None);
        let empty_file = launched_verbosity(Some("[desktop]\ndeveloper_mode = true\n"), None);

        // THEN the variable wins, then the setting, then the default 4
        assert_eq!(variable_and_setting, Some(1));
        assert_eq!(setting_alone, Some(2));
        assert_eq!(nothing, Some(4));
        assert_eq!(empty_file, Some(4));
    }

    #[test]
    fn test_a_level_the_policy_imposes_wins_over_the_file() {
        // GIVEN a file asking for debug, and a policy imposing errors only
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[engines]\nlog_level = \"debug\"\n").unwrap();
        let policy = PolicySettings::none().impose(LOG_LEVEL_SETTING, serde_json::json!("error"));

        // WHEN the level is read
        let level = read(Some(&path), &policy);

        // THEN the imposed level is the one in force
        assert_eq!(level, EngineLogLevel::Error);
    }

    #[test]
    fn test_an_unreadable_file_falls_back_to_the_default() {
        // GIVEN a file that is not valid TOML
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[engines\nlog_level = ").unwrap();

        // WHEN the level is read
        let level = read(Some(&path), &PolicySettings::none());

        // THEN the default applies, debug
        assert_eq!(level, EngineLogLevel::Debug);
    }
}
