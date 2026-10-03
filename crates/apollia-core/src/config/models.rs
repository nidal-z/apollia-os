use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{resolve_setting, PolicySettings};

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

    /// Folder the language models, their vision projectors and the embedding
    /// models are downloaded to and looked up in. Empty means the default,
    /// `<data dir>/models`; a path starting with `~/` is taken from the home
    /// directory. Changing it moves no file. Resolved by [`models_dir`].
    #[serde(default)]
    pub dir: String,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            registry_url: default_registry_url(),
            registry_mirror_url: default_registry_mirror_url(),
            registry_enabled: default_registry_enabled(),
            dir: String::new(),
        }
    }
}

/// Name of the default models folder inside the data directory.
pub const MODELS_DIR_NAME: &str = "models";

/// The named setting that holds the models folder.
pub const MODELS_DIR_KEY: &str = "models.dir";

/// The default models folder: `<data_dir>/models`.
///
/// The dictation models stay in this folder whatever the setting says: the
/// setting covers the language models, their projectors and the embedding
/// models.
pub fn default_models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MODELS_DIR_NAME)
}

/// The models folder in force for the data directory `data_dir`: the
/// `[models] dir` key of `<data_dir>/apollia.toml`, else the default.
///
/// The file is read on every call, so a changed folder applies to the next
/// download, listing or launch without a restart.
pub fn models_dir(data_dir: &Path) -> PathBuf {
    resolve_models_dir(data_dir, None)
}

/// The models folder, in this order: the folder given on the command line,
/// then the `[models] dir` key of `<data_dir>/apollia.toml`, then
/// `<data_dir>/models`.
///
/// A file that cannot be read falls back to the default with a warning, never
/// to a guess.
pub fn resolve_models_dir(data_dir: &Path, cli_override: Option<&Path>) -> PathBuf {
    if let Some(dir) = cli_override {
        return dir.to_path_buf();
    }
    let config = data_dir.join(crate::paths::CONFIG_FILE_NAME);
    match resolve_setting(&config, MODELS_DIR_KEY, &PolicySettings::none()) {
        Ok(resolved) => match resolved.value.as_str().map(str::trim) {
            Some(dir) if !dir.is_empty() => expand_home(dir),
            _ => default_models_dir(data_dir),
        },
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %config.display(),
                error = %e,
                "config.models_dir.unreadable"
            );
            default_models_dir(data_dir)
        }
    }
}

/// `~/rest` under the home directory; any other path as written.
fn expand_home(dir: &str) -> PathBuf {
    match (dir.strip_prefix("~/"), crate::paths::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(dir),
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
    fn the_command_line_folder_wins_over_the_setting() {
        // GIVEN a data directory whose apollia.toml names a models folder
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join("apollia.toml"),
            "[models]\ndir = \"/from/setting\"\n",
        )
        .unwrap();

        // WHEN the folder is resolved with a folder given on the command line
        let dir = resolve_models_dir(data.path(), Some(Path::new("/from/option")));

        // THEN the command line wins
        assert_eq!(dir, PathBuf::from("/from/option"));
    }

    #[test]
    fn the_setting_wins_over_the_default() {
        // GIVEN a data directory whose apollia.toml names a models folder
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join("apollia.toml"),
            "[models]\ndir = \"/from/setting\"\n",
        )
        .unwrap();

        // WHEN the folder is resolved without a command-line folder
        let dir = resolve_models_dir(data.path(), None);

        // THEN the setting is used, and models_dir agrees
        assert_eq!(dir, PathBuf::from("/from/setting"));
        assert_eq!(models_dir(data.path()), dir);
    }

    #[test]
    fn without_option_or_setting_the_default_folder_applies() {
        // GIVEN a data directory with no apollia.toml, and one with an empty key
        let bare = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        std::fs::write(empty.path().join("apollia.toml"), "[models]\ndir = \"\"\n").unwrap();

        // WHEN the folder is resolved for each
        let from_bare = resolve_models_dir(bare.path(), None);
        let from_empty = resolve_models_dir(empty.path(), None);

        // THEN both fall back to <data dir>/models
        assert_eq!(from_bare, default_models_dir(bare.path()));
        assert_eq!(from_empty, empty.path().join(MODELS_DIR_NAME));
    }

    #[test]
    fn an_unreadable_file_falls_back_to_the_default_folder() {
        // GIVEN an apollia.toml that is not valid TOML
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join("apollia.toml"), "[models\n").unwrap();

        // WHEN the folder is resolved
        let dir = models_dir(data.path());

        // THEN the default applies rather than a guess
        assert_eq!(dir, default_models_dir(data.path()));
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
