use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError, WebReadConfig, WebSearchConfig};

// ─────────────────────────────────────────────
// ToolsConfig
// ─────────────────────────────────────────────

/// Native tool output configuration (`[tools]` section in `apollia.toml`).
///
/// Controls the limits the runtime applies to native tool outputs before
/// forwarding them to the LLM. Protects the LLM context window against large
/// outputs (one of the non-negotiable safeguards).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsConfig {
    /// Maximum size of a tool output forwarded to the LLM, in UTF-8 bytes.
    ///
    /// Applied by every dispatcher the native tool factory builds
    /// (`NativeDispatcherConfig::max_output_chars`), on the chat and agent
    /// paths alike.
    /// Outputs exceeding this limit are truncated with a "middle-trim" strategy:
    /// the start and end are preserved, the middle is removed and replaced by a
    /// message stating how many lines were lost.
    /// Default: 30 000. Bounds: [10, 1 000 000].
    #[serde(default = "default_max_output_chars")]
    pub max_output_chars: usize,

    /// Native tools statically disabled by the operator in `apollia.toml`.
    ///
    /// The names listed here are removed from the dispatcher at boot, so any
    /// invocation results in `UnknownTool`. This list complements the `tools`
    /// table in `governance.db`: a tool disabled in either one is inactive.
    /// Default: `[]`.
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Configuration of the native `web_search` tool.
    #[serde(default)]
    pub web_search: WebSearchConfig,

    /// Configuration of the native `web_read` tool.
    #[serde(default)]
    pub web_read: WebReadConfig,

    /// Absolute path of a Python interpreter the operator chose over the
    /// bundled one, for the `python_executor` tool and the agent virtualenvs.
    ///
    /// `None`, the default, means the interpreter Apollia ships with. That is
    /// the only configuration the product is tested in, and the only one that
    /// depends on nothing installed on the machine (principle #2). A system
    /// interpreter is used when, and only when, it is named here.
    ///
    /// The path is checked before it is accepted and again before each use: it
    /// must exist, start, and report a minor version the bundled standard
    /// library can be read by. An interpreter that stops satisfying that, after
    /// an upgrade or an uninstall, is dropped with a warning and the bundled
    /// one answers instead, rather than the tool failing.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [tools]
    /// python_interpreter = "/opt/corp/python3.13/bin/python3"
    /// ```
    #[serde(default)]
    pub python_interpreter: Option<String>,

    /// What an agent created without code is granted when nobody chose for
    /// it.
    #[serde(default)]
    pub new_agent: NewAgentConfig,
}

/// The access an agent created without code is granted by default
/// (`[tools.new_agent]` in `apollia.toml`): whole families of tools, then
/// tools added by name.
///
/// The families are read against the tools active on the machine when the
/// agent is created, so the default follows what is installed and connected;
/// the names are the accesses an operator chose to give every new agent on
/// top of them. The Apollia agent reads the same default. Both keys resolve as
/// named settings, which the organization policy can fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewAgentConfig {
    /// Families granted whole, by the name a tool descriptor carries
    /// (`read`, `read_personal`, `write_or_delete`, `send_external`,
    /// `execute_code`). A name that is not a family grants nothing.
    /// Default: `["read"]`.
    #[serde(default = "default_new_agent_families")]
    pub families: Vec<String>,

    /// Tools granted by name on top of the families. A name the machine does
    /// not hold grants nothing. Default: `[]`.
    #[serde(default)]
    pub access: Vec<String>,
}

impl Default for NewAgentConfig {
    fn default() -> Self {
        Self {
            families: default_new_agent_families(),
            access: Vec::new(),
        }
    }
}

/// Dotted key of the families a new agent is granted, as
/// [`super::resolve_setting`] reads it.
pub const NEW_AGENT_FAMILIES_KEY: &str = "tools.new_agent.families";

/// Dotted key of the tools a new agent is granted by name.
pub const NEW_AGENT_ACCESS_KEY: &str = "tools.new_agent.access";

fn default_new_agent_families() -> Vec<String> {
    vec!["read".to_owned()]
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_chars: default_max_output_chars(),
            disabled: Vec::new(),
            web_search: WebSearchConfig::default(),
            web_read: WebReadConfig::default(),
            python_interpreter: None,
            new_agent: NewAgentConfig::default(),
        }
    }
}

impl ToolsConfig {
    /// Validates the tools configuration bounds at startup (fail-fast).
    ///
    /// - `max_output_chars`: must be in [10, 1 000 000].
    /// - `web_search.brave.max_results`: must be in [1, 20].
    /// - `web_search.brave.timeout_secs` and `web_search.duckduckgo.timeout_secs`: `[1, 120]`.
    /// - `web_search.duckduckgo.max_response_kb`: `[16, 16 384]`.
    /// - `web_read.timeout_secs`: `[1, 120]`. `web_read.max_response_kb`: `[64, 32 768]`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.max_output_chars",
            self.max_output_chars,
            10_usize,
            1_000_000_usize,
        )?;
        self.web_search.validate()?;
        self.web_read.validate()?;
        Ok(())
    }
}

fn default_max_output_chars() -> usize {
    30_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{resolve_setting, write_setting, PolicySettings, SettingSource};

    #[test]
    fn test_a_new_agent_is_granted_the_reading_family_until_the_file_widens_it() {
        // GIVEN a file without the section
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        let none = PolicySettings::none();

        // WHEN the two keys are read, then a tool is added by name
        let families = resolve_setting(&path, NEW_AGENT_FAMILIES_KEY, &none).unwrap();
        let before = resolve_setting(&path, NEW_AGENT_ACCESS_KEY, &none).unwrap();
        let after = write_setting(
            &path,
            NEW_AGENT_ACCESS_KEY,
            serde_json::json!(["file_write"]),
            &none,
        )
        .unwrap();

        // THEN the default is the reading family alone, and the file widens it
        assert_eq!(families.value, serde_json::json!(["read"]));
        assert_eq!(before.value, serde_json::json!([]));
        assert_eq!(before.source, SettingSource::Default);
        assert_eq!(after.value, serde_json::json!(["file_write"]));
        assert_eq!(after.source, SettingSource::User);
        assert_eq!(ToolsConfig::default().new_agent, NewAgentConfig::default());
    }

    #[test]
    fn test_a_policy_fixes_what_a_new_agent_is_granted() {
        // GIVEN a policy that imposes an empty list of added tools
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[tools.new_agent]\naccess = [\"bash\"]\n").unwrap();
        let policy = PolicySettings::none().impose(NEW_AGENT_ACCESS_KEY, serde_json::json!([]));

        // WHEN the key is read, then written by the user
        let resolved = resolve_setting(&path, NEW_AGENT_ACCESS_KEY, &policy).unwrap();
        let written = write_setting(
            &path,
            NEW_AGENT_ACCESS_KEY,
            serde_json::json!(["bash"]),
            &policy,
        );

        // THEN the policy answers over the file and the write is refused
        assert_eq!(resolved.value, serde_json::json!([]));
        assert_eq!(resolved.source, SettingSource::Policy);
        assert!(written.is_err(), "{written:?}");
    }
}
