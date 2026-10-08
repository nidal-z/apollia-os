//! Tauri IPC commands for managing automatic updates.
//!
//! Checks for new versions on GitHub Releases and triggers the Tauri
//! application update via `tauri-plugin-updater`.
//!
//! The endpoint follows the update channel of `[desktop] update_channel`. The
//! stable channel asks the manifest the bundle was built with; the preview
//! channel asks the preview manifest of the fixed `preview` release first,
//! then the stable one, and keeps the newer version of the two.

use apollia_core::config::UpdateChannel;
use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_updater::{Error as UpdaterError, Update, UpdaterExt};
use thiserror::Error;
use url::Url;

use crate::commands::config::settings::resolve_in_force;

/// The named setting that holds the update channel.
const UPDATE_CHANNEL_KEY: &str = "desktop.update_channel";

/// The path of a stable manifest: an asset of the latest published release.
const STABLE_PATH: &str = "/releases/latest/download/";

/// The path of a preview manifest: an asset of the fixed `preview` release,
/// which the preview-channel workflow refreshes when a trial version is
/// published.
const PREVIEW_PATH: &str = "/releases/download/preview/";

/// What a preview manifest adds to the name of its stable counterpart:
/// `latest.json` becomes `latest-preview.json`.
const PREVIEW_SUFFIX: &str = "-preview";

/// Possible errors when querying the updater plugin.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UpdateError {
    #[error("updater plugin unavailable: {0}")]
    Plugin(String),
    #[error("update check failed: {0}")]
    Check(String),
    #[error("no update available")]
    NoUpdate,
    #[error("install failed: {0}")]
    Install(String),
    /// This build carries no updater signing key, so self-update is inert.
    #[error(
        "self-update is not configured for this build: it needs a signing key and a signed \
         release manifest. Download the new version from the releases page instead."
    )]
    NotConfigured,
}

/// Whether this build lacks the updater signing key.
///
/// Read from the Tauri config the app was built with, so the answer follows the
/// build rather than a duplicated constant. An absent or empty `pubkey` means the
/// plugin can never verify an artifact, and therefore can never install one.
fn updater_signing_key_is_absent(app: &tauri::AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .is_none_or(str::is_empty)
}

/// Download progress emitted during [`install_update`] via the
/// `"update-download-progress"` Tauri event.
///
/// `total` mirrors the server's `Content-Length`: it is `None` (serialised as
/// `null`) when the endpoint does not advertise a size, in which case the
/// frontend renders an indeterminate progress bar.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateDownloadProgress {
    /// Cumulative bytes downloaded so far.
    pub downloaded: u64,
    /// Total download size in bytes, or `None` when the server omits it.
    pub total: Option<u64>,
}

/// Result of the update check.
#[derive(Debug, Serialize)]
pub struct UpdateCheckResult {
    /// Whether an update is available.
    pub available: bool,
    /// Current application version.
    pub current_version: String,
    /// Available version (if `available` is `true`).
    pub new_version: Option<String>,
    /// Release notes (if `available` is `true`).
    pub release_notes: Option<String>,
    /// The endpoint answered, but it publishes no manifest this build can use.
    ///
    /// True on the first public release, when no `latest.json` is attached yet:
    /// the endpoint returns 404 and the plugin reports [`UpdaterError::ReleaseNotFound`].
    /// That is not a failure the operator can act on, so it is reported as a
    /// state rather than as an error.
    pub channel_unavailable: bool,
}

/// The preview counterpart of a stable manifest endpoint, or `None` when the
/// endpoint does not have the shape of a stable manifest.
///
/// A bundle installed from a former CUDA edition points at
/// `latest-cuda.json`, so the preview endpoint is derived from the one the
/// bundle carries rather than written once: such a bundle keeps asking its
/// own manifest on either channel, and the release publishes it as a copy of
/// the standard one.
fn preview_endpoint(stable: &str) -> Option<String> {
    let (head, file) = stable.split_once(STABLE_PATH)?;
    let (stem, extension) = file.rsplit_once('.')?;
    Some(format!(
        "{head}{PREVIEW_PATH}{stem}{PREVIEW_SUFFIX}.{extension}"
    ))
}

/// The manifest endpoints the update check asks for `channel`, in order.
///
/// `configured` is the endpoint list of `plugins.updater.endpoints`. The
/// preview channel reads the preview manifest and then the stable one, so a
/// stable release published after the last trial version still reaches it.
fn channel_endpoints(configured: &[String], channel: UpdateChannel) -> Vec<String> {
    match channel {
        UpdateChannel::Stable => configured.to_vec(),
        UpdateChannel::Preview => configured
            .iter()
            .flat_map(|stable| preview_endpoint(stable).into_iter().chain([stable.clone()]))
            .collect(),
    }
}

/// The endpoints of `plugins.updater.endpoints` the app was built with.
fn configured_endpoints(app: &tauri::AppHandle) -> Vec<String> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("endpoints"))
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The update channel in force. A setting that cannot be read, or that names
/// no channel this build knows, falls back to stable.
fn update_channel() -> UpdateChannel {
    match resolve_in_force(UPDATE_CHANNEL_KEY)
        .and_then(|resolved| serde_json::from_value(resolved.value).map_err(|e| e.to_string()))
    {
        Ok(channel) => channel,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "update.channel.unreadable"
            );
            UpdateChannel::Stable
        }
    }
}

/// A candidate update, reduced to what choosing between channels needs.
trait Versioned {
    fn version(&self) -> &str;
}

impl Versioned for Update {
    fn version(&self) -> &str {
        &self.version
    }
}

/// What the endpoints of a channel said, taken together.
#[derive(Debug)]
enum Verdict<T> {
    /// The newest update any endpoint offered.
    Available(T),
    /// At least one endpoint answered, and none offered an update.
    UpToDate,
    /// No endpoint publishes a manifest this build can use.
    Unavailable,
}

/// Folds the answers of every endpoint of a channel into one verdict.
///
/// The newest update wins, by SemVer; a version that does not parse ranks
/// below any that does. Without an update, a real failure of any endpoint is
/// reported, so a check that could not ask the preview manifest does not
/// claim the build is up to date.
fn fold_answers<T: Versioned>(
    answers: Vec<Result<Option<T>, UpdaterError>>,
) -> Result<Verdict<T>, UpdaterError> {
    let mut newest: Option<(Option<semver::Version>, T)> = None;
    let mut answered = false;
    let mut failure = None;
    for answer in answers {
        match answer {
            Ok(Some(update)) => {
                let version = semver::Version::parse(update.version()).ok();
                if newest.as_ref().is_none_or(|(best, _)| version > *best) {
                    newest = Some((version, update));
                }
            }
            Ok(None) => answered = true,
            Err(e) if is_missing_manifest(&e) => {}
            Err(e) => {
                failure.get_or_insert(e);
            }
        }
    }
    if let Some((_, update)) = newest {
        return Ok(Verdict::Available(update));
    }
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(if answered {
        Verdict::UpToDate
    } else {
        Verdict::Unavailable
    })
}

/// Asks every endpoint of the channel in force and folds their answers.
async fn check_channel(app: &tauri::AppHandle) -> Result<Verdict<Update>, UpdateError> {
    let channel = update_channel();
    let configured = configured_endpoints(app);
    let endpoints = channel_endpoints(&configured, channel);
    tracing::event!(
        tracing::Level::INFO,
        channel = ?channel,
        endpoints = endpoints.len(),
        "update.check.started"
    );
    let mut answers = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let url = Url::parse(&endpoint).map_err(|e| UpdateError::Plugin(e.to_string()))?;
        let updater = app
            .updater_builder()
            .endpoints(vec![url])
            .and_then(|builder| builder.build())
            .map_err(|e| UpdateError::Plugin(e.to_string()))?;
        answers.push(updater.check().await);
    }
    fold_answers(answers).map_err(|e| UpdateError::Check(e.to_string()))
}

/// Whether this updater error means "no manifest to read", not "the check broke".
///
/// The plugin folds a 404 into [`UpdaterError::ReleaseNotFound`]: a non-success
/// status is logged and dropped, `last_error` stays unset, and the missing
/// release surfaces at the end of the endpoint loop. A manifest that exists but
/// omits the running platform lands on the two `Target*NotFound` variants, which
/// mean the same thing from the operator's side. A transport failure keeps its
/// own variants and stays a real error, because retrying it can succeed.
fn is_missing_manifest(err: &UpdaterError) -> bool {
    matches!(
        err,
        UpdaterError::ReleaseNotFound
            | UpdaterError::TargetNotFound(_)
            | UpdaterError::TargetsNotFound(_)
    )
}

/// Checks whether an update is available on GitHub Releases.
///
/// Queries the endpoints of the update channel in force (see
/// [`channel_endpoints`]), derived from `plugins.updater.endpoints` of
/// `tauri.conf.json`. Returns a serializable description of the current state.
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    // Displayed, not compared: the plugin compares against the version of
    // `tauri.conf.json`, which equals the crate version on a release and
    // differs on a trial version, whose bundle is stamped `X.Y.Z-N` because
    // WiX rejects a non-numeric pre-release identifier. Showing the crate
    // version here keeps this panel and the About screen on one string.
    //
    // `tauri build` does not emit `latest.json`: with `createUpdaterArtifacts`
    // it emits one minisign signature per updater artifact, and the release
    // workflow composes the manifest from them (the "Compose the updater
    // manifest" step of release.yml, driven by packaging/artifacts.json).
    // That step must stamp the `tauri.conf.json` version, never the git tag:
    // SemVer ranks a numeric pre-release identifier below an alphanumeric
    // one, so a manifest carrying the tag `X.Y.Z-rc.N` would read as
    // permanently newer than the bundle `X.Y.Z-N` it describes.
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    // Self-update needs a signing key: the plugin verifies the downloaded
    // artifact against `plugins.updater.pubkey` before installing it, and a
    // release must publish a signed `latest.json`. Until both exist, say so
    // plainly instead of surfacing the plugin's internal error, which reads like
    // a network failure and sends the user looking in the wrong place.
    if updater_signing_key_is_absent(&app) {
        return Err(UpdateError::NotConfigured.to_string());
    }

    match check_channel(&app).await.map_err(|e| e.to_string())? {
        Verdict::Available(update) => Ok(UpdateCheckResult {
            available: true,
            current_version,
            new_version: Some(update.version.clone()),
            release_notes: update.body.clone(),
            channel_unavailable: false,
        }),
        Verdict::UpToDate => Ok(UpdateCheckResult {
            available: false,
            current_version,
            new_version: None,
            release_notes: None,
            channel_unavailable: false,
        }),
        Verdict::Unavailable => {
            tracing::event!(tracing::Level::INFO, "update.channel_unavailable");
            Ok(UpdateCheckResult {
                available: false,
                current_version,
                new_version: None,
                release_notes: None,
                channel_unavailable: true,
            })
        }
    }
}

/// Downloads and installs the available update, then restarts.
///
/// Must be called after `check_for_update`. Returns `NoUpdate` if nothing is
/// available anymore (a concurrent instance, or a pulled release).
///
/// Emits `"update-download-progress"` ([`UpdateDownloadProgress`]) on every
/// downloaded chunk so the frontend can render a live progress bar instead of
/// blocking silently until completion.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let Verdict::Available(update) = check_channel(&app).await.map_err(|e| e.to_string())? else {
        return Err(UpdateError::NoUpdate.to_string());
    };

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let payload = UpdateDownloadProgress {
                    downloaded,
                    total: content_length,
                };
                if let Err(e) = progress_app.emit("update-download-progress", &payload) {
                    tracing::warn!(error = %e, "update.progress.emit.failed");
                }
            },
            || {},
        )
        .await
        .map_err(|e| UpdateError::Install(e.to_string()).to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STABLE: &str =
        "https://github.com/Apollia-OS/apollia-os/releases/latest/download/latest.json";
    const PREVIEW: &str =
        "https://github.com/Apollia-OS/apollia-os/releases/download/preview/latest-preview.json";

    struct Candidate(&'static str);

    impl Versioned for Candidate {
        fn version(&self) -> &str {
            self.0
        }
    }

    #[test]
    fn test_preview_channel_asks_the_preview_manifest_then_the_stable_one() {
        // GIVEN the stable endpoint the bundle carries, and the preview channel
        let configured = vec![STABLE.to_owned()];

        // WHEN the endpoints of the channel are built
        let endpoints = channel_endpoints(&configured, UpdateChannel::Preview);

        // THEN the preview manifest comes first, the stable one after it
        assert_eq!(endpoints, vec![PREVIEW.to_owned(), STABLE.to_owned()]);
    }

    #[test]
    fn test_stable_channel_asks_the_stable_manifest_only() {
        // GIVEN the stable endpoint the bundle carries, and the stable channel
        let configured = vec![STABLE.to_owned()];

        // WHEN the endpoints of the channel are built
        let endpoints = channel_endpoints(&configured, UpdateChannel::Stable);

        // THEN only the stable manifest is asked
        assert_eq!(endpoints, vec![STABLE.to_owned()]);
    }

    #[test]
    fn test_a_cuda_bundle_keeps_its_own_manifest_on_the_preview_channel() {
        // GIVEN the endpoint a CUDA bundle is built with
        let cuda =
            "https://github.com/Apollia-OS/apollia-os/releases/latest/download/latest-cuda.json";

        // WHEN its preview endpoint is derived
        let preview = preview_endpoint(cuda);

        // THEN it names the CUDA preview manifest of the preview release
        assert_eq!(
            preview.as_deref(),
            Some("https://github.com/Apollia-OS/apollia-os/releases/download/preview/latest-cuda-preview.json")
        );
    }

    #[test]
    fn test_an_endpoint_of_another_shape_has_no_preview_counterpart() {
        // GIVEN an endpoint that is not an asset of the latest release
        let other = "https://updates.example.org/apollia/latest.json";

        // WHEN the endpoints of the preview channel are built
        let endpoints = channel_endpoints(&[other.to_owned()], UpdateChannel::Preview);

        // THEN only that endpoint is asked, nothing is invented
        assert_eq!(endpoints, vec![other.to_owned()]);
    }

    #[test]
    fn test_the_bundled_endpoint_has_the_shape_the_preview_channel_derives_from() {
        // GIVEN the tauri.conf.json the bundle is built with
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let endpoint = conf["plugins"]["updater"]["endpoints"][0].as_str().unwrap();

        // WHEN its preview counterpart is derived
        let preview = preview_endpoint(endpoint);

        // THEN there is one, named after the stable manifest
        assert_eq!(preview.as_deref(), Some(PREVIEW));
    }

    #[test]
    fn test_the_newer_update_wins_between_the_two_manifests() {
        // GIVEN a trial version from the preview manifest and a later stable one
        let answers = vec![Ok(Some(Candidate("0.3.0-2"))), Ok(Some(Candidate("0.3.0")))];

        // WHEN the answers are folded
        let verdict = fold_answers(answers).unwrap();

        // THEN the stable version is offered
        assert!(matches!(verdict, Verdict::Available(Candidate("0.3.0"))));
    }

    #[test]
    fn test_the_first_release_without_suffix_outranks_every_earlier_preview() {
        // GIVEN this release and the two strings the last preview carried, its
        // product version and the bundle version the updater compared against
        let release = semver::Version::parse("0.3.0").unwrap();
        let product = semver::Version::parse("0.2.0-preview").unwrap();
        let bundle = semver::Version::parse("0.2.0-1").unwrap();

        // WHEN they are ranked the way the updater plugin ranks them
        // THEN an installed preview reads the release as newer, never the reverse
        assert!(release > product);
        assert!(release > bundle);
    }

    #[test]
    fn test_the_bundle_and_the_crate_carry_the_same_version() {
        // GIVEN the configuration the bundle is stamped from
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();

        // WHEN its version is read
        let bundled = config["version"].as_str().unwrap();

        // THEN it is the crate version: the updater compares against one, the
        // About screen shows the other, and a release has no reason to split them
        assert_eq!(bundled, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_a_missing_preview_manifest_does_not_hide_the_stable_answer() {
        // GIVEN no preview release yet, and a stable manifest with nothing newer
        let answers: Vec<Result<Option<Candidate>, UpdaterError>> =
            vec![Err(UpdaterError::ReleaseNotFound), Ok(None)];

        // WHEN the answers are folded
        let verdict = fold_answers(answers).unwrap();

        // THEN the build is up to date
        assert!(matches!(verdict, Verdict::UpToDate));
    }

    #[test]
    fn test_no_manifest_anywhere_is_the_unavailable_state() {
        // GIVEN two endpoints that publish no manifest
        let answers: Vec<Result<Option<Candidate>, UpdaterError>> = vec![
            Err(UpdaterError::ReleaseNotFound),
            Err(UpdaterError::ReleaseNotFound),
        ];

        // WHEN the answers are folded
        let verdict = fold_answers(answers).unwrap();

        // THEN the channel is unavailable, not an error
        assert!(matches!(verdict, Verdict::Unavailable));
    }

    #[test]
    fn test_a_transport_failure_without_update_stays_an_error() {
        // GIVEN a preview manifest that could not be fetched, and nothing newer
        // on the stable one
        let answers: Vec<Result<Option<Candidate>, UpdaterError>> = vec![
            Err(UpdaterError::Network("connection reset".to_owned())),
            Ok(None),
        ];

        // WHEN the answers are folded
        let verdict = fold_answers(answers);

        // THEN the failure is reported rather than an up-to-date claim
        assert!(verdict.is_err());
    }

    #[test]
    fn test_update_check_result_serializes_available() {
        // GIVEN an UpdateCheckResult with a new version available
        let result = UpdateCheckResult {
            available: true,
            current_version: "0.9.0".to_string(),
            new_version: Some("1.0.0".to_string()),
            release_notes: Some("Bug fixes and improvements".to_string()),
            channel_unavailable: false,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["available"], true);
        assert_eq!(json["current_version"], "0.9.0");
        assert_eq!(json["new_version"], "1.0.0");
        assert_eq!(json["release_notes"], "Bug fixes and improvements");
        assert_eq!(json["channel_unavailable"], false);
    }

    #[test]
    fn test_update_check_result_serializes_up_to_date() {
        // GIVEN an UpdateCheckResult with no update available
        let result = UpdateCheckResult {
            available: false,
            current_version: "1.0.0".to_string(),
            new_version: None,
            release_notes: None,
            channel_unavailable: false,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN available is false and version fields are null
        assert_eq!(json["available"], false);
        assert_eq!(json["current_version"], "1.0.0");
        assert!(json["new_version"].is_null());
        assert!(json["release_notes"].is_null());
    }

    #[test]
    fn test_missing_manifest_is_a_state_not_an_error() {
        // GIVEN the three plugin errors that mean "no manifest to read"
        let missing = [
            UpdaterError::ReleaseNotFound,
            UpdaterError::TargetNotFound("darwin-aarch64".to_string()),
            UpdaterError::TargetsNotFound(vec!["darwin-aarch64".to_string()]),
        ];

        // WHEN each is classified
        // THEN each is reported as a state the operator cannot act on
        for err in &missing {
            assert!(is_missing_manifest(err), "{err} should be a state");
        }
    }

    #[test]
    fn test_transport_failure_stays_an_error() {
        // GIVEN a plugin error that a retry could resolve
        let err = UpdaterError::Network("connection reset".to_string());

        // WHEN it is classified
        // THEN it is not folded into the up-to-date path, so the operator can retry
        assert!(!is_missing_manifest(&err));
    }

    #[test]
    fn test_update_download_progress_serializes_with_total() {
        // GIVEN a progress payload with a known total
        let progress = UpdateDownloadProgress {
            downloaded: 1024,
            total: Some(4096),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&progress).expect("serialize");

        // THEN both fields are present under their snake_case names
        assert_eq!(json["downloaded"], 1024);
        assert_eq!(json["total"], 4096);
    }

    #[test]
    fn test_update_download_progress_serializes_indeterminate() {
        // GIVEN a progress payload without a known total (indeterminate)
        let progress = UpdateDownloadProgress {
            downloaded: 512,
            total: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&progress).expect("serialize");

        // THEN total is null so the frontend renders an indeterminate bar
        assert_eq!(json["downloaded"], 512);
        assert!(json["total"].is_null());
    }

    #[tokio::test]
    async fn test_update_error_messages_are_descriptive() {
        // GIVEN each variant of UpdateError
        // WHEN converted to string
        // THEN messages identify the failure kind for the frontend
        assert_eq!(UpdateError::NoUpdate.to_string(), "no update available");
        assert!(UpdateError::Plugin("boom".into())
            .to_string()
            .contains("boom"));
        assert!(UpdateError::Check("net".into())
            .to_string()
            .starts_with("update check failed"));
        assert!(UpdateError::Install("hash".into())
            .to_string()
            .starts_with("install failed"));
    }
}
