use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// ModelsConfig
// ─────────────────────────────────────────────

/// Default address of the signed index of model cards.
///
/// A host the project serves, never the storage bucket behind it, so that an
/// organisation can point the same key at an internal mirror.
pub const DEFAULT_MODEL_REGISTRY_URL: &str = "https://registry.apollia.fr/models/stable/index.json";

/// Default fallback address, tried when the main one does not answer: the same
/// index attached to the latest release of the project.
pub const DEFAULT_MODEL_REGISTRY_MIRROR_URL: &str =
    "https://github.com/Apollia-OS/apollia-os/releases/latest/download/models-stable-index.json";

/// Model catalogue configuration (`[models]` section in `apollia.toml`).
///
/// The catalogue reads one signed index of model cards, and only when it is
/// opened: a request with no body and no identifier, answered by a static file.
/// The signature, not the address, decides whether the index is accepted, so
/// replacing the address by an internal mirror never widens what is trusted.
/// Every field has a sane default via [`Default`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsConfig {
    /// Address of the index. Its signature is read at the same address with
    /// `.minisig` appended.
    #[serde(default = "default_registry_url")]
    pub registry_url: String,

    /// Address tried when `registry_url` does not answer. An empty string
    /// disables the fallback.
    #[serde(default = "default_registry_mirror_url")]
    pub registry_mirror_url: String,

    /// `false` turns the online index off: the catalogue then lists nothing,
    /// and a model is still imported from a disk.
    #[serde(default = "default_registry_enabled")]
    pub registry_enabled: bool,

    /// Rules applied to a model imported from a disk (`[models.import]`).
    #[serde(default)]
    pub import: ModelsImportConfig,
}

/// Rules applied to a model imported from a disk (`[models.import]` section
/// in `apollia.toml`), read by the desktop import command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsImportConfig {
    /// `true` refuses a file whose digest matches no signed model card.
    #[serde(default)]
    pub require_verified: bool,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            registry_url: default_registry_url(),
            registry_mirror_url: default_registry_mirror_url(),
            registry_enabled: default_registry_enabled(),
            import: ModelsImportConfig::default(),
        }
    }
}

fn default_registry_url() -> String {
    DEFAULT_MODEL_REGISTRY_URL.to_owned()
}

fn default_registry_mirror_url() -> String {
    DEFAULT_MODEL_REGISTRY_MIRROR_URL.to_owned()
}

fn default_registry_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Sections {
        models: Option<ModelsConfig>,
    }

    fn read(content: &str) -> Result<ModelsConfig, toml::de::Error> {
        toml::from_str::<Sections>(content).map(|s| s.models.unwrap_or_default())
    }

    #[test]
    fn an_absent_section_gives_the_project_registry() {
        // GIVEN an apollia.toml without a [models] section
        let content = "[chat]\nplan_mode_default = true\n";

        // WHEN the section is read
        let config = read(content).unwrap_or_default();

        // THEN the registry is on, at the project address and its mirror
        assert_eq!(config, ModelsConfig::default());
        assert!(config.registry_enabled);
        assert_eq!(config.registry_url, DEFAULT_MODEL_REGISTRY_URL);
    }

    #[test]
    fn a_mirror_or_a_cut_is_read_from_the_section() {
        // GIVEN a section that points at an internal mirror and turns it off
        let content = "[models]\nregistry_url = \"https://mirror.example.org/index.json\"\nregistry_enabled = false\n";

        // WHEN the section is read
        let config = read(content);

        // THEN both keys land, and the fallback keeps its default
        let config = config.unwrap_or_default();
        assert_eq!(config.registry_url, "https://mirror.example.org/index.json");
        assert!(!config.registry_enabled);
        assert_eq!(
            config.registry_mirror_url,
            DEFAULT_MODEL_REGISTRY_MIRROR_URL
        );
    }

    #[test]
    fn a_misshapen_section_is_an_error() {
        // GIVEN a section whose switch is not a boolean
        let content = "[models]\nregistry_enabled = \"no\"\n";

        // WHEN the section is read
        let result = read(content);

        // THEN it is refused rather than read as the default
        assert!(result.is_err());
    }
}
