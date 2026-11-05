//! Which engine serves each model format.
//!
//! The settings name, per format, the automatic choice (the default), the
//! engine shipped with the application, or a variant of the engine store.
//! A named variant designates the newest verified build of that variant; when
//! the store no longer holds one, the automatic choice applies again and a
//! warning says so. The environment overrides of the launchers keep priority
//! over all of it.

use std::path::Path;

use apollia_core::config::{resolve_setting, EngineChoice, PolicySettings};

use crate::engine_store::compat::{Rating, Verdict};
use crate::engine_store::installed::{
    installed_llama_variants, newest_installed, wanted_llama_variant, InstalledPackage,
};
use crate::engine_store::manifest::{Engine, Variant};

/// The dotted key of the engine that serves GGUF models.
pub const GGUF_ENGINE_SETTING: &str = "engines.gguf";
/// The dotted key of the engine that serves MLX models.
pub const MLX_ENGINE_SETTING: &str = "engines.mlx";
/// The dotted key of the automatic update of the chosen engines.
pub const AUTO_UPDATE_SETTING: &str = "engines.auto_update";
/// The dotted key of the automatic removal of the builds used the least.
pub const AUTO_PURGE_SETTING: &str = "engines.auto_purge";

/// The engine settings in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineChoices {
    /// The engine that serves GGUF models.
    pub gguf: EngineChoice,
    /// The engine that serves MLX models.
    pub mlx: EngineChoice,
    /// Whether an announced update of a chosen engine is installed unasked.
    pub auto_update: bool,
    /// Whether an update is followed by the removal of the builds of its
    /// variant used the least.
    pub auto_purge: bool,
}

impl Default for EngineChoices {
    fn default() -> Self {
        Self {
            gguf: EngineChoice::Auto,
            mlx: EngineChoice::Auto,
            auto_update: true,
            auto_purge: false,
        }
    }
}

impl EngineChoices {
    /// The settings in force, read from `config_file` under `imposed`. A
    /// missing file gives the defaults; an unreadable one gives them too,
    /// with a warning, so a typo never keeps an engine from starting.
    #[must_use]
    pub fn read(config_file: Option<&Path>, imposed: &PolicySettings) -> Self {
        let defaults = Self::default();
        let Some(path) = config_file else {
            return defaults;
        };
        let value = |key: &str| match resolve_setting(path, key, imposed) {
            Ok(resolved) => Some(resolved.value),
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %path.display(),
                    key,
                    error = %error,
                    detail = "the default applies",
                    "engine_store.selection.unreadable"
                );
                None
            }
        };
        let choice = |key: &str| {
            value(key)
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .and_then(EngineChoice::from_wire)
                .unwrap_or_default()
        };
        Self {
            gguf: choice(GGUF_ENGINE_SETTING),
            mlx: choice(MLX_ENGINE_SETTING),
            auto_update: value(AUTO_UPDATE_SETTING)
                .as_ref()
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.auto_update),
            auto_purge: value(AUTO_PURGE_SETTING)
                .as_ref()
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.auto_purge),
        }
    }
}

/// The store variants a choice names, the preferred first: none for the
/// automatic choice and for the shipped engine, CUDA 13 then CUDA 12 for
/// `cuda`, the one it names otherwise.
#[must_use]
pub const fn named_variants(choice: EngineChoice) -> &'static [Variant] {
    match choice {
        EngineChoice::Auto | EngineChoice::Embedded => &[],
        EngineChoice::Cuda => &[Variant::Cuda13, Variant::Cuda12],
        EngineChoice::Cuda12 => &[Variant::Cuda12],
        EngineChoice::Cuda13 => &[Variant::Cuda13],
        EngineChoice::Rocm => &[Variant::Rocm],
        EngineChoice::Vulkan => &[Variant::Vulkan],
        EngineChoice::Cpu => &[Variant::Cpu],
        EngineChoice::Metal => &[Variant::Metal],
    }
}

/// What a choice comes down to on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The launcher chooses, as it does without a setting.
    Automatic,
    /// The engine shipped with the application.
    Embedded,
    /// This verified build of the store.
    Stored(InstalledPackage),
}

/// A choice resolved against the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// What serves.
    pub resolved: Resolved,
    /// True when a variant was named and the store no longer holds it: the
    /// automatic choice applies in its place.
    pub fell_back: bool,
}

/// Resolve `choice` for `engine` against the store under `root`, `ratings`
/// being those of this machine for `llama-server`.
///
/// A choice naming several variants (`cuda`) takes, among those installed,
/// the recommended one, then a compatible one, then an incompatible one, each
/// time in the order the choice names them. A variant named explicitly is
/// taken even when it is rated incompatible, the choice of the user coming
/// first, with a warning. A named variant the store does not hold falls back
/// to the automatic choice, with a warning.
#[must_use]
pub fn resolve(
    root: Option<&Path>,
    engine: Engine,
    choice: EngineChoice,
    ratings: &[Rating],
) -> Resolution {
    let variants = named_variants(choice);
    if variants.is_empty() {
        let resolved = if choice == EngineChoice::Embedded {
            Resolved::Embedded
        } else {
            Resolved::Automatic
        };
        return Resolution {
            resolved,
            fell_back: false,
        };
    }
    let verdict = |variant: Variant| {
        ratings
            .iter()
            .find(|r| r.variant == variant)
            .map(|r| r.verdict)
    };
    let rank = |package: &InstalledPackage| match verdict(package.variant) {
        Some(Verdict::Recommended) => 0,
        Some(Verdict::Compatible) | None => 1,
        Some(Verdict::Incompatible) => 2,
    };
    let installed = root.and_then(|root| {
        variants
            .iter()
            .filter_map(|variant| newest_installed(root, engine, *variant))
            .min_by_key(rank)
    });
    match installed {
        Some(package) => {
            if verdict(package.variant) == Some(Verdict::Incompatible) {
                tracing::event!(
                    tracing::Level::WARN,
                    engine = engine.as_str(),
                    variant = package.variant.as_str(),
                    tag = %package.tag,
                    detail = "the chosen variant is rated incompatible with this machine, it is started as chosen",
                    "engine_store.selection.incompatible_choice"
                );
            }
            Resolution {
                resolved: Resolved::Stored(package),
                fell_back: false,
            }
        }
        None => {
            tracing::event!(
                tracing::Level::WARN,
                engine = engine.as_str(),
                variant = choice.as_str(),
                detail = "the chosen variant is no longer installed, the automatic choice applies",
                "engine_store.selection.fell_back"
            );
            Resolution {
                resolved: Resolved::Automatic,
                fell_back: true,
            }
        }
    }
}

/// The ratings of `llama-server` a choice of the GGUF engine is resolved by:
/// asked for only when the store under `root` holds a `llama-server` build
/// and the choice is not the shipped engine, empty otherwise.
pub(crate) fn ratings_for_choice(
    root: Option<&Path>,
    choice: EngineChoice,
    ratings: impl FnOnce() -> Vec<Rating>,
) -> (Vec<Variant>, Vec<Rating>) {
    let installed = match (root, choice) {
        (Some(root), choice) if choice != EngineChoice::Embedded => installed_llama_variants(root),
        _ => Vec::new(),
    };
    if installed.is_empty() {
        return (installed, Vec::new());
    }
    let ratings = ratings();
    (installed, ratings)
}

/// The store builds the settings put in service: the ones an automatic update
/// keeps current. The shipped engines are not among them, since the store
/// does not replace them.
///
/// `ratings`, those of this machine for `llama-server`, is only asked for
/// when the store holds a `llama-server` build the GGUF choice may take.
pub fn chosen_variants(
    root: &Path,
    choices: EngineChoices,
    ratings: impl FnOnce() -> Vec<Rating>,
) -> Vec<(Engine, Variant)> {
    let mut chosen = Vec::new();
    let (installed, ratings) = ratings_for_choice(Some(root), choices.gguf, ratings);
    match resolve(Some(root), Engine::LlamaServer, choices.gguf, &ratings).resolved {
        Resolved::Stored(package) => chosen.push((Engine::LlamaServer, package.variant)),
        Resolved::Automatic => {
            if let Some(variant) = wanted_llama_variant(&ratings, &installed) {
                chosen.push((Engine::LlamaServer, variant));
            }
        }
        Resolved::Embedded => {}
    }
    match resolve(Some(root), Engine::VllmMlx, choices.mlx, &[]).resolved {
        Resolved::Stored(package) => chosen.push((Engine::VllmMlx, package.variant)),
        Resolved::Automatic => {
            if newest_installed(root, Engine::VllmMlx, Variant::Metal).is_some() {
                chosen.push((Engine::VllmMlx, Variant::Metal));
            }
        }
        Resolved::Embedded => {}
    }
    chosen
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn config(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn test_the_choices_written_in_the_file_are_the_ones_read() {
        // GIVEN a file choosing an engine per format and turning updates off,
        // a file that says nothing of the engines, and no file at all
        let (_a, written) =
            config("[engines]\ngguf = \"cuda\"\nmlx = \"embedded\"\nauto_update = false\nauto_purge = true\n");
        let (_b, silent) = config("[desktop]\ndeveloper_mode = true\n");

        // WHEN the choices are read
        let chosen = EngineChoices::read(Some(&written), &PolicySettings::none());
        let defaults = EngineChoices::read(Some(&silent), &PolicySettings::none());
        let no_file = EngineChoices::read(None, &PolicySettings::none());

        // THEN the file's choices are in force, and without them the
        // automatic engines, kept up to date
        assert_eq!(chosen.gguf, EngineChoice::Cuda);
        assert_eq!(chosen.mlx, EngineChoice::Embedded);
        assert!(!chosen.auto_update);
        assert!(chosen.auto_purge);
        assert!(!defaults.auto_purge);
        assert_eq!(defaults, EngineChoices::default());
        assert_eq!(no_file, EngineChoices::default());
        assert!(defaults.auto_update);
    }

    #[test]
    fn test_a_choice_the_policy_imposes_wins_and_an_unreadable_file_gives_the_defaults() {
        // GIVEN a file choosing CUDA, a policy imposing the embedded engine,
        // and a file that is not valid TOML
        let (_a, written) = config("[engines]\ngguf = \"cuda\"\n");
        let policy =
            PolicySettings::none().impose(GGUF_ENGINE_SETTING, serde_json::json!("embedded"));
        let (_b, broken) = config("[engines\ngguf = ");

        // WHEN the choices are read
        let imposed = EngineChoices::read(Some(&written), &policy);
        let fallback = EngineChoices::read(Some(&broken), &PolicySettings::none());

        // THEN the imposed engine is in force, and the broken file reads as
        // the defaults
        assert_eq!(imposed.gguf, EngineChoice::Embedded);
        assert_eq!(fallback, EngineChoices::default());
    }

    #[test]
    fn test_only_a_named_variant_absent_from_the_store_falls_back() {
        // GIVEN an empty store
        let dir = tempfile::tempdir().unwrap();

        // WHEN each kind of choice is resolved
        let automatic = resolve(
            Some(dir.path()),
            Engine::LlamaServer,
            EngineChoice::Auto,
            &[],
        );
        let embedded = resolve(
            Some(dir.path()),
            Engine::LlamaServer,
            EngineChoice::Embedded,
            &[],
        );
        let removed = resolve(
            Some(dir.path()),
            Engine::LlamaServer,
            EngineChoice::Cuda,
            &[],
        );

        // THEN the first two are what they say, and the named variant falls
        // back to the automatic choice, flagged
        assert_eq!(
            (automatic.resolved, automatic.fell_back),
            (Resolved::Automatic, false)
        );
        assert_eq!(
            (embedded.resolved, embedded.fell_back),
            (Resolved::Embedded, false)
        );
        assert_eq!(
            (removed.resolved, removed.fell_back),
            (Resolved::Automatic, true)
        );
    }

    /// File a verified build of `variant` under `root`, as the store does.
    fn file(root: &Path, variant: Variant) {
        use crate::engine_store::installed::{package_dir, Source, RECORD_FILE};
        let dir = package_dir(root, Engine::LlamaServer, "b6000", variant);
        std::fs::create_dir_all(&dir).unwrap();
        let record = InstalledPackage {
            engine: Engine::LlamaServer,
            tag: "b6000".to_owned(),
            variant,
            platform: "linux-x64".to_owned(),
            sha256: "0".repeat(64),
            source: Source::Downloaded,
            released: None,
            path: dir.clone(),
        };
        std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&record).unwrap()).unwrap();
    }

    fn stored_variant(resolution: Resolution) -> Option<Variant> {
        match resolution.resolved {
            Resolved::Stored(package) => Some(package.variant),
            _ => None,
        }
    }

    #[test]
    fn test_cuda_takes_cuda_13_when_installed_and_cuda_12_otherwise() {
        // GIVEN a store holding CUDA 12 only, and one holding both runtimes
        let twelve = tempfile::tempdir().unwrap();
        file(twelve.path(), Variant::Cuda12);
        let both = tempfile::tempdir().unwrap();
        file(both.path(), Variant::Cuda12);
        file(both.path(), Variant::Cuda13);

        // WHEN `cuda` and each named runtime are resolved
        let resolve_in =
            |root: &Path, choice| resolve(Some(root), Engine::LlamaServer, choice, &[]);
        let alone = resolve_in(twelve.path(), EngineChoice::Cuda);
        let best = resolve_in(both.path(), EngineChoice::Cuda);
        let named = resolve_in(both.path(), EngineChoice::Cuda12);
        let missing = resolve_in(twelve.path(), EngineChoice::Cuda13);

        // THEN `cuda` is the best one installed, a named runtime is itself,
        // and a named runtime absent falls back
        assert_eq!(stored_variant(alone), Some(Variant::Cuda12));
        assert_eq!(stored_variant(best), Some(Variant::Cuda13));
        assert_eq!(stored_variant(named), Some(Variant::Cuda12));
        assert!(missing.fell_back);
    }
}
