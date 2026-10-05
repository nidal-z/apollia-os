//! The automatic update of the chosen engines.
//!
//! The followed channel is checked when the runtime starts, when the channel
//! changes and when a check is asked for. An update it announces for a build
//! the settings put in service is then downloaded and installed, by the same
//! path as an install asked by hand: the manifest signature is verified
//! before any archive is requested, and the archive digest is compared before
//! anything is extracted. With the setting off, the update is only announced.
//!
//! A check runs on its own task, at most one per store at a time.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use apollia_core::config::PolicySettings;

use crate::engine_store::fetch::{install_package, refresh_manifest, updates, VariantUpdate};
use crate::engine_store::installed::InstalledPackage;
use crate::engine_store::manifest::{Engine, Variant};
use crate::engine_store::selection::{chosen_variants, EngineChoices};
use crate::engine_store::settings::EngineStoreSettings;
use crate::engine_store::{EngineStore, EngineStoreError};

/// What one check of the channel did.
#[derive(Debug)]
pub struct AutoUpdateReport {
    /// What the verified manifest says of each variant it offers.
    pub updates: Vec<VariantUpdate>,
    /// The builds downloaded, verified and installed by this check.
    pub installed: Vec<InstalledPackage>,
    /// The updates that were refused, each with its reason. Nothing of a
    /// refused update is installed.
    pub refused: Vec<(Engine, Variant, EngineStoreError)>,
}

/// Verify the manifest of the followed channel, then, when `enabled`, install
/// the update of each build in `chosen` the manifest announces.
///
/// # Errors
///
/// Every failure of [`refresh_manifest`]: without a verified manifest nothing
/// is announced and no archive is requested. A failure on one archive does
/// not fail the check: it is reported in [`AutoUpdateReport::refused`].
pub async fn check_and_update(
    store: &EngineStore,
    settings: &EngineStoreSettings,
    enabled: bool,
    chosen: &[(Engine, Variant)],
) -> Result<AutoUpdateReport, EngineStoreError> {
    let (manifest, _) = refresh_manifest(store, settings).await?;
    let found = updates(&store.root, &manifest, &store.platform);
    let mut report = AutoUpdateReport {
        updates: found.clone(),
        installed: Vec::new(),
        refused: Vec::new(),
    };
    for update in found.iter().filter(|u| u.update_available) {
        let is_chosen = chosen.contains(&(update.engine, update.variant));
        if !enabled || !is_chosen {
            tracing::event!(
                tracing::Level::INFO,
                engine = update.engine.as_str(),
                variant = update.variant.as_str(),
                latest = %update.latest_tag,
                chosen = is_chosen,
                "engine_store.update.announced"
            );
            continue;
        }
        match install_package(store, settings, update.engine, update.variant).await {
            Ok(package) => {
                tracing::event!(
                    tracing::Level::INFO,
                    engine = update.engine.as_str(),
                    variant = update.variant.as_str(),
                    tag = %package.tag,
                    "engine_store.update.installed"
                );
                report.installed.push(package);
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    engine = update.engine.as_str(),
                    variant = update.variant.as_str(),
                    error = %error,
                    "engine_store.update.refused"
                );
                report.refused.push((update.engine, update.variant, error));
            }
        }
    }
    Ok(report)
}

/// The stores a check is running on. The lock is held for a lookup, an
/// insertion or a removal, never across an await.
static RUNNING: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

fn running() -> &'static Mutex<HashSet<PathBuf>> {
    RUNNING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Frees the store's place when its check ends, however it ends.
struct Running(PathBuf);

impl Drop for Running {
    fn drop(&mut self) {
        if let Ok(mut set) = running().lock() {
            set.remove(&self.0);
        }
    }
}

fn claim(root: &Path) -> Option<Running> {
    let mut set = running().lock().ok()?;
    set.insert(root.to_path_buf())
        .then(|| Running(root.to_path_buf()))
}

/// Start one check of the store under `data_dir` on its own task, unless one
/// is already running there. The settings are read from `config_file` under
/// `imposed` when the task starts.
///
/// A build without a distribution key checks nothing: no manifest could be
/// verified.
pub fn spawn(data_dir: &Path, config_file: Option<PathBuf>, imposed: PolicySettings) {
    let Ok(store) = EngineStore::open(data_dir) else {
        tracing::event!(
            tracing::Level::DEBUG,
            detail = "no distribution key in this build",
            "engine_store.update.skipped"
        );
        return;
    };
    let Some(guard) = claim(&store.root) else {
        return;
    };
    tokio::spawn(async move {
        let _guard = guard;
        let root = store.root.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let settings = EngineStoreSettings::load(&root)?;
            let choices = EngineChoices::read(config_file.as_deref(), &imposed);
            let chosen = chosen_variants(&root, choices, || {
                matches!(
                    apollia_llm::hardware::detect().accelerator,
                    apollia_llm::AcceleratorProfile::Cuda { .. }
                )
            });
            Ok::<_, EngineStoreError>((settings, choices.auto_update, chosen))
        })
        .await;
        let (settings, enabled, chosen) = match prepared {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(error)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "engine_store.update.check_failed"
                );
                return;
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "engine_store.update.check_failed"
                );
                return;
            }
        };
        if let Err(error) = check_and_update(&store, &settings, enabled, &chosen).await {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "engine_store.update.check_failed"
            );
        }
    });
}
