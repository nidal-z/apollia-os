use serde::{Deserialize, Serialize};

/// The author an exported agent is published under (`[author]` section in
/// `apollia.toml`).
///
/// The name is written as the publisher of every `.apo` package exported from
/// this machine, and in its signature when the package is signed; the key
/// fingerprint written beside it is what identifies the author. Empty by
/// default: an export then asks for a name, and a caller may always give one
/// for a single export without changing this setting. Resolved as the named
/// setting `author.name`.
///
/// Unknown keys are refused, so `apollia-os config set author.<typo>` fails
/// instead of writing a value nothing reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorConfig {
    /// The publisher name of the packages exported from this machine.
    #[serde(default)]
    pub name: String,
}

/// Dotted key of the publisher name, as [`super::resolve_setting`] reads it.
pub const AUTHOR_NAME_KEY: &str = "author.name";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_author_name_resolves_from_the_file_and_defaults_to_empty() {
        // GIVEN a file without the section, and one that names an author
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("absent.toml");
        let named = dir.path().join("named.toml");
        std::fs::write(&named, "[author]\nname = \"Ada Lovelace\"\n").unwrap();

        // WHEN the named setting is resolved against each
        let none = super::super::PolicySettings::none();
        let default = super::super::resolve_setting(&absent, AUTHOR_NAME_KEY, &none).unwrap();
        let user = super::super::resolve_setting(&named, AUTHOR_NAME_KEY, &none).unwrap();

        // THEN the absent section gives the empty default and the file its name
        assert_eq!(default.value, serde_json::json!(""));
        assert_eq!(default.source, super::super::SettingSource::Default);
        assert_eq!(user.value, serde_json::json!("Ada Lovelace"));
        assert_eq!(user.source, super::super::SettingSource::User);
    }

    #[test]
    fn test_author_name_is_written_as_a_string_only() {
        // GIVEN an empty configuration file
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        let none = super::super::PolicySettings::none();

        // WHEN a name, then a number, is written under the key
        let written =
            super::super::write_setting(&path, AUTHOR_NAME_KEY, serde_json::json!("Ada"), &none)
                .unwrap();
        let refused =
            super::super::write_setting(&path, AUTHOR_NAME_KEY, serde_json::json!(3), &none);

        // THEN the name reads back and the number is refused
        assert_eq!(written.value, serde_json::json!("Ada"));
        assert!(refused.is_err());
    }
}
