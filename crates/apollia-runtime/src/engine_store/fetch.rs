//! Fetching, downloading and importing: every path verifies the manifest
//! signature first, then the archive digest, then extracts.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::engine_store::archive;
use crate::engine_store::installed::{
    newest_installed, package_dir, InstalledPackage, Source, RECORD_FILE,
};
use crate::engine_store::manifest::{
    archive_kind, compare_tags, verify_manifest, Engine, Manifest, Package, Variant,
    MANIFEST_MAX_BYTES,
};
use crate::engine_store::settings::{
    accept_sequence, cache_manifest, check_base_url, write_atomic, EngineStoreSettings,
};
use crate::engine_store::{EngineStore, EngineStoreError};

/// Name of the manifest file, online and on a disk.
pub const MANIFEST_FILE: &str = "engines.json";
/// Name of its signature.
pub const SIGNATURE_FILE: &str = "engines.json.minisig";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether a newer build of one engine variant exists on the channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct VariantUpdate {
    /// Engine.
    pub engine: Engine,
    /// Variant.
    pub variant: Variant,
    /// Newest tag installed here, if any.
    pub installed_tag: Option<String>,
    /// Newest tag the verified manifest offers.
    pub latest_tag: String,
    /// True when a tag is installed and the manifest offers a newer one.
    pub update_available: bool,
    /// Size in bytes of the archive of the newest build, as the verified
    /// manifest states it: what installing or updating downloads.
    pub size: u64,
    /// Day the newest build was published, `YYYY-MM-DD`, when the manifest
    /// states it.
    pub latest_released: Option<String>,
    /// Day the installed build was published, when its manifest stated it.
    pub installed_released: Option<String>,
}

/// Compare what is installed with a verified manifest, per variant offered
/// for this platform.
#[must_use]
pub fn updates(root: &Path, manifest: &Manifest, platform: &str) -> Vec<VariantUpdate> {
    let mut out = Vec::new();
    for engine in Engine::ALL {
        for variant in Variant::ALL {
            let Some(latest) = manifest.newest(engine, variant, platform) else {
                continue;
            };
            let installed = newest_installed(root, engine, variant);
            let update_available = installed
                .as_ref()
                .is_some_and(|p| compare_tags(&latest.tag, &p.tag) == std::cmp::Ordering::Greater);
            out.push(VariantUpdate {
                engine,
                variant,
                installed_released: installed.as_ref().and_then(|p| p.released.clone()),
                installed_tag: installed.map(|p| p.tag),
                latest_tag: latest.tag.clone(),
                update_available,
                size: latest.size,
                latest_released: latest.released.clone(),
            });
        }
    }
    out
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> EngineStoreError {
    let path = path.to_path_buf();
    move |source| EngineStoreError::Io { path, source }
}

fn manifest_url(settings: &EngineStoreSettings) -> Result<reqwest::Url, EngineStoreError> {
    let base = check_base_url(&settings.base_url)?;
    let text = format!(
        "{}/engines-{}/{MANIFEST_FILE}",
        base.as_str().trim_end_matches('/'),
        settings.channel.as_str()
    );
    reqwest::Url::parse(&text).map_err(|_| EngineStoreError::InvalidUrl { url: text })
}

fn client(url: &reqwest::Url) -> Result<reqwest::Client, EngineStoreError> {
    apollia_core::net::configured_endpoint_client_builder_for(url.as_str())
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|source| EngineStoreError::Transport {
            url: url.to_string(),
            source,
        })
}

async fn get(
    client: &reqwest::Client,
    url: &reqwest::Url,
) -> Result<reqwest::Response, EngineStoreError> {
    let response =
        client
            .get(url.clone())
            .send()
            .await
            .map_err(|source| EngineStoreError::Transport {
                url: url.to_string(),
                source,
            })?;
    let status = response.status();
    if !status.is_success() {
        return Err(EngineStoreError::HttpStatus {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    Ok(response)
}

async fn get_small(
    client: &reqwest::Client,
    url: &reqwest::Url,
) -> Result<Vec<u8>, EngineStoreError> {
    let response = get(client, url).await?;
    apollia_core::net::read_capped_bytes(response, MANIFEST_MAX_BYTES)
        .await
        .map_err(|e| EngineStoreError::ManifestInvalid {
            reason: format!("{url}: {e}"),
        })
}

/// Fetch the manifest of the followed channel and its signature, verify the
/// signature against the compiled key, refuse a rollback, and keep it.
///
/// # Errors
///
/// Any network, signature, format, channel or rollback failure. No archive is
/// requested here.
pub async fn refresh_manifest(
    store: &EngineStore,
    settings: &EngineStoreSettings,
) -> Result<(Manifest, reqwest::Url), EngineStoreError> {
    let url = manifest_url(settings)?;
    let sig_url = url
        .join(SIGNATURE_FILE)
        .map_err(|_| EngineStoreError::InvalidUrl {
            url: url.to_string(),
        })?;
    let client = client(&url)?;
    let bytes = get_small(&client, &url).await?;
    let signature_bytes = get_small(&client, &sig_url).await?;
    let signature = String::from_utf8_lossy(&signature_bytes).into_owned();
    let manifest = verify_manifest(&bytes, &signature, &store.trusted_key, settings.channel)?;
    accept_sequence(&store.root, &manifest)?;
    cache_manifest(&store.root, settings.channel, &bytes, &signature)?;
    tracing::event!(
        tracing::Level::INFO,
        channel = settings.channel.as_str(),
        sequence = manifest.sequence,
        packages = manifest.packages.len(),
        "engine_store.manifest.verified"
    );
    Ok((manifest, url))
}

/// Download the newest `engine` `variant` package of the followed channel for
/// this platform, after verifying the manifest, then check its digest and
/// install it.
///
/// # Errors
///
/// Every failure of [`refresh_manifest`], [`EngineStoreError::PackageNotFound`],
/// a transport or status failure on the archive,
/// [`EngineStoreError::DigestMismatch`] or [`EngineStoreError::TooLarge`]
/// (the archive is deleted), and extraction failures.
pub async fn install_package(
    store: &EngineStore,
    settings: &EngineStoreSettings,
    engine: Engine,
    variant: Variant,
) -> Result<InstalledPackage, EngineStoreError> {
    install_package_with_progress(store, settings, engine, variant, |_, _| {}).await
}

/// [`install_package`], calling `progress(received, total)` as the archive
/// arrives, `total` being the size the verified manifest states. The first
/// call, `progress(0, total)`, is made once the manifest signature held and
/// before the archive is requested: nothing is called while the signature is
/// still being verified, nor when it is refused.
///
/// The archive being received is deleted however the download ends, a
/// dropped future included.
///
/// # Errors
///
/// Those of [`install_package`].
pub async fn install_package_with_progress(
    store: &EngineStore,
    settings: &EngineStoreSettings,
    engine: Engine,
    variant: Variant,
    progress: impl Fn(u64, u64),
) -> Result<InstalledPackage, EngineStoreError> {
    let (manifest, manifest_url) = refresh_manifest(store, settings).await?;
    let package = manifest
        .newest(engine, variant, &store.platform)
        .ok_or_else(|| EngineStoreError::PackageNotFound {
            engine: engine.as_str(),
            variant: variant.as_str(),
            platform: store.platform.clone(),
        })?
        .clone();
    let url = manifest_url
        .join(&package.url)
        .map_err(|_| EngineStoreError::InvalidUrl {
            url: package.url.clone(),
        })?;
    let downloads = store.root.join(".downloads");
    tokio::fs::create_dir_all(&downloads)
        .await
        .map_err(io(&downloads))?;
    let file_name = archive_file_name(&package);
    let archive = PartialArchive(downloads.join(&file_name));
    progress(0, package.size);
    download(&client(&url)?, &url, &archive.0, &package, &progress).await?;
    install_verified_archive(store, &package, &archive.0, Source::Downloaded)
}

/// An archive being received, deleted when the download ends: installed,
/// refused, or dropped because its task was cancelled.
struct PartialArchive(PathBuf);

impl Drop for PartialArchive {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn archive_file_name(package: &Package) -> String {
    package
        .url
        .rsplit('/')
        .next()
        .unwrap_or(package.url.as_str())
        .to_owned()
}

async fn download(
    client: &reqwest::Client,
    url: &reqwest::Url,
    dest: &Path,
    package: &Package,
    progress: &impl Fn(u64, u64),
) -> Result<(), EngineStoreError> {
    let mut response = get(client, url).await?;
    let mut file = tokio::fs::File::create(dest).await.map_err(io(dest))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    while let Some(chunk) =
        response
            .chunk()
            .await
            .map_err(|source| EngineStoreError::Transport {
                url: url.to_string(),
                source,
            })?
    {
        received += chunk.len() as u64;
        if received > package.size {
            return Err(EngineStoreError::TooLarge {
                file: archive_file_name(package),
                expected: package.size,
            });
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(io(dest))?;
        progress(received, package.size);
    }
    file.flush().await.map_err(io(dest))?;
    check_digest(package, &hex::encode(hasher.finalize()))
}

fn check_digest(package: &Package, actual: &str) -> Result<(), EngineStoreError> {
    if actual != package.sha256 {
        return Err(EngineStoreError::DigestMismatch {
            file: archive_file_name(package),
            expected: package.sha256.clone(),
            actual: actual.to_owned(),
        });
    }
    Ok(())
}

fn hash_file(path: &Path, limit: u64) -> Result<(String, u64), EngineStoreError> {
    let mut file = std::fs::File::open(path).map_err(io(path))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = file.read(&mut buf).map_err(io(path))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > limit {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Extract an archive whose digest already matched, then record it. An
/// existing install of the same tag and variant is replaced.
fn install_verified_archive(
    store: &EngineStore,
    package: &Package,
    archive_path: &Path,
    source: Source,
) -> Result<InstalledPackage, EngineStoreError> {
    let kind = archive_kind(&package.url).ok_or_else(|| EngineStoreError::ManifestInvalid {
        reason: format!("unsupported archive {:?}", package.url),
    })?;
    let final_dir = package_dir(&store.root, package.engine, &package.tag, package.variant);
    let partial = final_dir.with_file_name(format!(".partial-{}", package.variant.as_str()));
    if partial.exists() {
        std::fs::remove_dir_all(&partial).map_err(io(&partial))?;
    }
    archive::extract(archive_path, kind, &partial)?;
    let record = InstalledPackage {
        engine: package.engine,
        tag: package.tag.clone(),
        variant: package.variant,
        platform: package.platform.clone(),
        sha256: package.sha256.clone(),
        source,
        released: package.released.clone(),
        path: final_dir.clone(),
    };
    let record_bytes =
        serde_json::to_vec_pretty(&record).map_err(|source| EngineStoreError::StoreFile {
            path: partial.join(RECORD_FILE),
            source,
        })?;
    let record_path = partial.join(RECORD_FILE);
    if record_path.symlink_metadata().is_ok() {
        std::fs::remove_file(&record_path).map_err(io(&record_path))?;
    }
    write_atomic(&record_path, &record_bytes)?;
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir).map_err(io(&final_dir))?;
    }
    std::fs::rename(&partial, &final_dir).map_err(io(&final_dir))?;
    tracing::event!(
        tracing::Level::INFO,
        engine = package.engine.as_str(),
        tag = %package.tag,
        variant = package.variant.as_str(),
        source = ?source,
        "engine_store.package.installed"
    );
    Ok(record)
}

/// Import engines from a directory holding `engines.json`, its signature and
/// archives: the same verification as a download (signature, rollback, then
/// digest) and the same layout. Every package of the manifest for this
/// platform whose archive sits in the directory is installed.
///
/// # Errors
///
/// [`EngineStoreError::MissingSignature`] when the signature is absent, every
/// verification failure of a download, and
/// [`EngineStoreError::PackageNotFound`] when no archive of this platform is in
/// the directory.
pub fn import_from_dir(
    store: &EngineStore,
    dir: &Path,
) -> Result<Vec<InstalledPackage>, EngineStoreError> {
    import_from_dir_with_progress(store, dir, || {})
}

/// [`import_from_dir`], calling `verified()` once the manifest signature held
/// and before any archive of the directory is read.
///
/// # Errors
///
/// Those of [`import_from_dir`].
pub fn import_from_dir_with_progress(
    store: &EngineStore,
    dir: &Path,
    verified: impl FnOnce(),
) -> Result<Vec<InstalledPackage>, EngineStoreError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let signature_path = dir.join(SIGNATURE_FILE);
    let bytes = read_capped(&manifest_path)?;
    if !signature_path.is_file() {
        return Err(EngineStoreError::MissingSignature {
            path: signature_path,
        });
    }
    let signature = String::from_utf8_lossy(&read_capped(&signature_path)?).into_owned();
    // The signature is checked before the channel is read from the manifest.
    apollia_signing::distribution::verify_minisign(&bytes, &signature, &store.trusted_key)
        .map_err(|source| EngineStoreError::Signature { source })?;
    let manifest = Manifest::parse(&bytes)?;
    accept_sequence(&store.root, &manifest)?;
    verified();
    let mut installed = Vec::new();
    for package in manifest
        .packages
        .iter()
        .filter(|p| p.platform == store.platform)
    {
        let archive_path = dir.join(archive_file_name(package));
        if !archive_path.is_file() {
            continue;
        }
        let (actual, size) = hash_file(&archive_path, package.size)?;
        if size > package.size {
            return Err(EngineStoreError::TooLarge {
                file: archive_file_name(package),
                expected: package.size,
            });
        }
        check_digest(package, &actual)?;
        installed.push(install_verified_archive(
            store,
            package,
            &archive_path,
            Source::Imported,
        )?);
    }
    if installed.is_empty() {
        return Err(EngineStoreError::PackageNotFound {
            engine: "any",
            variant: "any",
            platform: store.platform.clone(),
        });
    }
    Ok(installed)
}

fn read_capped(path: &PathBuf) -> Result<Vec<u8>, EngineStoreError> {
    let file = std::fs::File::open(path).map_err(io(path))?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io(path))?;
    if bytes.len() as u64 > MANIFEST_MAX_BYTES {
        return Err(EngineStoreError::ManifestInvalid {
            reason: format!("{} is over {MANIFEST_MAX_BYTES} bytes", path.display()),
        });
    }
    Ok(bytes)
}
