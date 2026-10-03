use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// DesktopConfig
// ─────────────────────────────────────────────

/// Settings of the desktop application that Rust reads (`[desktop]` section in
/// `apollia.toml`).
///
/// A setting lives here, rather than in the webview's storage, when a reader on
/// the Rust side needs it or when the organization policy must be able to lock
/// it. A pure display preference stays out of the file. Every field has a
/// default, so a file without the section loads with [`Default`], and every
/// field is a named setting resolved through
/// [`resolve_setting`](super::resolve_setting) under the key
/// `desktop.<field>`.
///
/// Unknown keys are refused, so `apollia-os config set desktop.<typo>` fails
/// instead of writing a value nothing reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopConfig {
    /// Shows the advanced surfaces of the application: technical identifiers,
    /// logs, engines, debugging details. Off by default.
    #[serde(default)]
    pub developer_mode: bool,
    /// The update channel the application asks for a new version. Stable by
    /// default; the organization policy can impose it.
    #[serde(default)]
    pub update_channel: UpdateChannel,
}

/// The channel the desktop application reads its update manifest from.
///
/// Stable receives the published versions only. Preview receives the trial
/// versions as well: it reads the preview manifest and the stable one, and
/// keeps the newer of the two.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// The published versions only.
    #[default]
    Stable,
    /// The published versions and the trial versions.
    Preview,
}

impl DesktopConfig {
    /// Reads the `[desktop]` section of a whole `apollia.toml` document.
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
            desktop: Option<DesktopConfig>,
        }
        let sections: Sections = toml::from_str(content)?;
        Ok(sections.desktop.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_config_absent_section_loads_the_defaults() {
        // GIVEN an apollia.toml carrying other sections but no [desktop]
        let content = "[chat]\nplan_mode_default = true\n";

        // WHEN the desktop section is read
        let desktop = DesktopConfig::from_toml_str(content).unwrap();

        // THEN every field holds its default: developer mode off
        assert_eq!(desktop, DesktopConfig::default());
        assert!(!desktop.developer_mode);
    }

    #[test]
    fn test_desktop_config_reads_developer_mode() {
        // GIVEN a [desktop] section turning developer mode on
        let content = "[desktop]\ndeveloper_mode = true\n";

        // WHEN it is read
        let desktop = DesktopConfig::from_toml_str(content).unwrap();

        // THEN the value is the file's
        assert!(desktop.developer_mode);
    }

    #[test]
    fn test_desktop_config_refuses_an_unknown_key() {
        // GIVEN a [desktop] section with a key no field declares
        let content = "[desktop]\nnot_a_field = true\n";

        // WHEN it is read
        let result = DesktopConfig::from_toml_str(content);

        // THEN it is refused, naming the key
        let error = result.unwrap_err().to_string();
        assert!(error.contains("not_a_field"), "{error}");
    }

    #[test]
    fn test_desktop_config_update_channel_is_stable_by_default() {
        // GIVEN a [desktop] section that does not name the update channel
        let content = "[desktop]\ndeveloper_mode = true\n";

        // WHEN it is read
        let desktop = DesktopConfig::from_toml_str(content).unwrap();

        // THEN the channel is stable
        assert_eq!(desktop.update_channel, UpdateChannel::Stable);
    }

    #[test]
    fn test_desktop_config_reads_the_preview_channel() {
        // GIVEN a [desktop] section choosing the preview channel
        let content = "[desktop]\nupdate_channel = \"preview\"\n";

        // WHEN it is read
        let desktop = DesktopConfig::from_toml_str(content).unwrap();

        // THEN the channel is the file's
        assert_eq!(desktop.update_channel, UpdateChannel::Preview);
    }

    #[test]
    fn test_desktop_config_refuses_an_unknown_channel() {
        // GIVEN a [desktop] section naming a channel that does not exist
        let content = "[desktop]\nupdate_channel = \"nightly\"\n";

        // WHEN it is read
        let result = DesktopConfig::from_toml_str(content);

        // THEN it is refused
        assert!(result.is_err());
    }

    #[test]
    fn test_desktop_config_refuses_a_value_of_the_wrong_type() {
        // GIVEN developer_mode written as a string
        let content = "[desktop]\ndeveloper_mode = \"yes\"\n";

        // WHEN it is read
        let result = DesktopConfig::from_toml_str(content);

        // THEN it is refused
        assert!(result.is_err());
    }
}
