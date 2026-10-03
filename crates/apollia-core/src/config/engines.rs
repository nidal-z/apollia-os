use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// EnginesConfig
// ─────────────────────────────────────────────

/// Wire names of the engine log levels, the values `engines.log_level`
/// accepts, from the quietest to the most verbose.
pub const ENGINE_LOG_LEVELS: &[&str] = &["error", "warning", "info", "debug"];

/// Settings of the local inference engines (`[engines]` section in
/// `apollia.toml`).
///
/// The section stands outside `[llm]` because a `[llm]` table must carry its
/// `default` and `backends` keys to be read at all, and a setting written
/// alone under it would make the whole file unreadable.
///
/// Every field is a named setting, resolved through
/// [`resolve_setting`](super::resolve_setting) under `engines.<field>`.
/// Unknown keys are refused, so a typo fails instead of writing a value
/// nothing reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnginesConfig {
    /// Log level of the local engine, applied when a model is next loaded or
    /// reloaded. Debug by default, the lowest level at which the engine names
    /// the device a model loads onto and how many layers it offloads.
    #[serde(default)]
    pub log_level: EngineLogLevel,
}

/// How much the local engine writes to its log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineLogLevel {
    /// Errors only.
    Error,
    /// Errors and warnings.
    Warning,
    /// Errors, warnings and information.
    Info,
    /// Everything, the load summary included. The default.
    #[default]
    Debug,
}

impl EngineLogLevel {
    /// Reads a wire name, `None` for anything else.
    #[must_use]
    pub fn from_wire(name: &str) -> Option<Self> {
        match name {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    /// The verbosity threshold llama-server takes on `-lv`: 1 error,
    /// 2 warning, 3 info, 4 debug.
    #[must_use]
    pub const fn llama_verbosity(self) -> u8 {
        match self {
            Self::Error => 1,
            Self::Warning => 2,
            Self::Info => 3,
            Self::Debug => 4,
        }
    }
}

impl EnginesConfig {
    /// Reads the `[engines]` section of a whole `apollia.toml` document.
    ///
    /// A document without the section gives the defaults; the other sections
    /// are not looked at.
    ///
    /// # Errors
    ///
    /// The document is not valid TOML, or the section carries an unknown key or
    /// a value of the wrong type.
    pub fn from_toml_str(content: &str) -> Result<Self, toml::de::Error> {
        #[derive(Deserialize)]
        struct Sections {
            engines: Option<EnginesConfig>,
        }
        let sections: Sections = toml::from_str(content)?;
        Ok(sections.engines.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engines_config_absent_section_is_debug() {
        // GIVEN an apollia.toml without an [engines] section
        let content = "[desktop]\ndeveloper_mode = true\n";

        // WHEN the section is read
        let engines = EnginesConfig::from_toml_str(content).unwrap();

        // THEN the engine logs at debug, verbosity 4
        assert_eq!(engines.log_level, EngineLogLevel::Debug);
        assert_eq!(engines.log_level.llama_verbosity(), 4);
    }

    #[test]
    fn test_engines_config_reads_the_log_level() {
        // GIVEN an [engines] section asking for warnings
        let content = "[engines]\nlog_level = \"warning\"\n";

        // WHEN it is read
        let engines = EnginesConfig::from_toml_str(content).unwrap();

        // THEN the level is the file's, verbosity 2
        assert_eq!(engines.log_level, EngineLogLevel::Warning);
        assert_eq!(engines.log_level.llama_verbosity(), 2);
    }

    #[test]
    fn test_engines_config_refuses_an_unknown_key_and_level() {
        // GIVEN a section with a key no field declares, and one with a level
        // the engine does not have
        let unknown_key = "[engines]\nlevel = \"info\"\n";
        let unknown_level = "[engines]\nlog_level = \"trace\"\n";

        // WHEN they are read
        let key_result = EnginesConfig::from_toml_str(unknown_key);
        let level_result = EnginesConfig::from_toml_str(unknown_level);

        // THEN both are refused
        assert!(key_result.is_err());
        assert!(level_result.is_err());
    }

    #[test]
    fn test_every_wire_name_reads_back_to_its_level() {
        // GIVEN the wire names the setting accepts
        // WHEN each is read and serialized again
        // THEN each round-trips, and a name outside the list reads as none
        for name in ENGINE_LOG_LEVELS {
            let level = EngineLogLevel::from_wire(name).unwrap();
            assert_eq!(serde_json::to_value(level).unwrap(), *name);
        }
        assert_eq!(EngineLogLevel::from_wire("trace"), None);
    }
}
