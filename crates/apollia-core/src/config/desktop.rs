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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopConfig {
    /// Shows the advanced surfaces of the application: technical identifiers,
    /// logs, engines, debugging details. Off by default.
    #[serde(default)]
    pub developer_mode: bool,
    /// Fetches the Open Graph preview of a link shown in a conversation. Off
    /// by default: the fetch leaves the machine. Read through
    /// [`DesktopConfig::link_preview_enabled_in`], which still honours the
    /// key's former place, `[ui] link_preview_enabled`.
    #[serde(default)]
    pub link_preview_enabled: bool,
    /// The update channel the application asks for a new version. Stable by
    /// default; the organization policy can impose it.
    #[serde(default)]
    pub update_channel: UpdateChannel,
    /// Whether the application may install the runtime as a service of the
    /// user's session ("Keep Apollia running without the application"). On by
    /// default; the organization policy imposes `false` to forbid it. It is an
    /// authorization only: whether the service is installed is what the OS
    /// service manager reports, never a value of this file.
    #[serde(default = "allowed")]
    pub background_service_allowed: bool,
}

const fn allowed() -> bool {
    true
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            developer_mode: false,
            link_preview_enabled: false,
            update_channel: UpdateChannel::default(),
            background_service_allowed: allowed(),
        }
    }
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

    /// Whether the link preview is on, read from a whole `apollia.toml`.
    ///
    /// `[desktop] link_preview_enabled` wins. While it is absent, the key's
    /// former place, `[ui] link_preview_enabled`, is still read, with a
    /// `config.key.moved` warning, so a file written for an earlier version
    /// keeps its choice. A document that does not parse, or names neither,
    /// leaves the preview off.
    pub fn link_preview_enabled_in(content: &str) -> bool {
        let Ok(doc) = toml::from_str::<toml::Table>(content) else {
            return false;
        };
        let read = |section: &str| {
            doc.get(section)
                .and_then(|s| s.get("link_preview_enabled"))
                .and_then(toml::Value::as_bool)
        };
        if let Some(enabled) = read("desktop") {
            return enabled;
        }
        match read("ui") {
            Some(enabled) => {
                tracing::warn!(
                    key = "ui.link_preview_enabled",
                    detail = "read until desktop.link_preview_enabled is written",
                    moved_to = "desktop.link_preview_enabled",
                    "config.key.moved"
                );
                enabled
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_config_allows_the_background_service_by_default() {
        // GIVEN a [desktop] section that does not name the authorization
        let content = "[desktop]\ndeveloper_mode = true\n";

        // WHEN the section is read, and when the defaults are taken
        let read = DesktopConfig::from_toml_str(content).unwrap();
        let default = DesktopConfig::default();

        // THEN installing the service is allowed in both
        assert!(read.background_service_allowed);
        assert!(default.background_service_allowed);
    }

    #[test]
    fn test_desktop_config_reads_a_forbidden_background_service() {
        // GIVEN a [desktop] section that forbids the service
        let content = "[desktop]\nbackground_service_allowed = false\n";

        // WHEN the section is read
        let desktop = DesktopConfig::from_toml_str(content).unwrap();

        // THEN the authorization is off
        assert!(!desktop.background_service_allowed);
    }

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

    #[test]
    fn test_link_preview_is_read_under_desktop() {
        // GIVEN the key in its place, and a file that names it nowhere
        let set = "[desktop]\nlink_preview_enabled = true\n";
        let absent = "[chat]\nplan_mode_default = true\n";

        // WHEN the link preview switch is read from each
        let on = DesktopConfig::link_preview_enabled_in(set);
        let off = DesktopConfig::link_preview_enabled_in(absent);

        // THEN the file's value is read, and the default leaves it off
        assert!(on);
        assert!(!off);
        assert!(
            DesktopConfig::from_toml_str(set)
                .unwrap()
                .link_preview_enabled
        );
    }

    #[test]
    fn test_the_former_ui_key_is_read_while_the_new_one_is_absent() {
        // GIVEN a file written for an earlier version, under [ui], and one
        // carrying both places with opposite values
        let former = "[ui]\nlink_preview_enabled = true\n";
        let both = "[ui]\nlink_preview_enabled = true\n[desktop]\nlink_preview_enabled = false\n";

        // WHEN the switch is read from each
        let from_former = DesktopConfig::link_preview_enabled_in(former);
        let from_both = DesktopConfig::link_preview_enabled_in(both);

        // THEN the former place is honoured until the new one is written, and
        // the new one wins once it is
        assert!(from_former);
        assert!(!from_both);
    }

    #[test]
    fn test_the_former_ui_key_is_accepted_by_the_validation() {
        // GIVEN a file that still carries [ui] link_preview_enabled
        let former = "[ui]\nlink_preview_enabled = true\n";

        // WHEN the whole file is validated
        let result = crate::config::validate(former);

        // THEN it is accepted (set aside as a withdrawn key, with its warning),
        // and the registry points to the new place
        assert!(result.is_ok(), "{result:?}");
        let entry = crate::config::keys::withdrawn_covering("ui.link_preview_enabled")
            .expect("the former key is withdrawn");
        assert!(entry
            .withdrawn
            .unwrap_or_default()
            .contains("desktop.link_preview_enabled"));
    }
}
