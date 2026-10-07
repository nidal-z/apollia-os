//! The engine manifest: its wire format, its parsing and its verification.
//!
//! See the module documentation of [`crate::engine_store`] for the format.

use serde::{Deserialize, Serialize};

use crate::engine_store::EngineStoreError;

/// The only manifest format this build reads.
pub const MANIFEST_FORMAT: u32 = 1;

/// Largest manifest or signature accepted, in bytes. A manifest lists a few
/// dozen packages; anything near this size is not one.
pub const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;

/// An inference engine the store can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = EngineName)]
pub enum Engine {
    /// The llama.cpp server.
    #[serde(rename = "llama-server")]
    LlamaServer,
    /// The MLX engine, a Python environment for Apple Silicon.
    #[serde(rename = "vllm-mlx")]
    VllmMlx,
    /// The Node.js runtime, a pinned LTS release, for the tool servers that
    /// run on it. Published under the `cpu` variant.
    #[serde(rename = "nodejs")]
    NodeJs,
}

impl Engine {
    /// Wire name, also the directory name under the store root.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Engine::LlamaServer => "llama-server",
            Engine::VllmMlx => "vllm-mlx",
            Engine::NodeJs => "nodejs",
        }
    }

    /// Every engine, in a stable order.
    pub const ALL: [Engine; 3] = [Engine::LlamaServer, Engine::VllmMlx, Engine::NodeJs];
}

/// A build of an engine for one kind of hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = EngineVariant)]
pub enum Variant {
    /// NVIDIA cards.
    Cuda,
    /// Any Vulkan card.
    Vulkan,
    /// No accelerator.
    Cpu,
    /// Apple Silicon.
    Metal,
}

impl Variant {
    /// Wire name, also the directory name under a tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Variant::Cuda => "cuda",
            Variant::Vulkan => "vulkan",
            Variant::Cpu => "cpu",
            Variant::Metal => "metal",
        }
    }

    /// Every variant, in a stable order.
    pub const ALL: [Variant; 4] = [Variant::Cuda, Variant::Vulkan, Variant::Cpu, Variant::Metal];
}

/// An update channel.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[schema(as = EngineChannel)]
pub enum Channel {
    /// Released engines. The default.
    #[default]
    Stable,
    /// Engines ahead of the stable channel.
    Preview,
}

impl Channel {
    /// Wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Preview => "preview",
        }
    }
}

/// One downloadable engine archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// Which engine the archive carries.
    pub engine: Engine,
    /// Upstream tag, `b6000` for llama.cpp, `v22.11.0` for Node.js.
    pub tag: String,
    /// Hardware the build targets.
    pub variant: Variant,
    /// Platform the build runs on, as [`current_platform`] spells it.
    pub platform: String,
    /// Archive size in bytes.
    pub size: u64,
    /// Archive address, relative to the manifest.
    pub url: String,
    /// Lowercase hex SHA-256 of the archive.
    pub sha256: String,
    /// Day the build was published, `YYYY-MM-DD`. Optional: a manifest that
    /// does not state it is read all the same, and the tag then names the
    /// build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
}

/// A manifest of one channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format version, [`MANIFEST_FORMAT`].
    pub format: u32,
    /// The channel this manifest belongs to.
    pub channel: Channel,
    /// Strictly increasing per channel; a lower one is a rollback.
    pub sequence: u64,
    /// The archives of the channel.
    pub packages: Vec<Package>,
}

impl Manifest {
    /// Parse and check a manifest's bytes. Does not verify the signature: call
    /// [`verify_manifest`] for bytes that come from outside.
    ///
    /// # Errors
    ///
    /// [`EngineStoreError::ManifestInvalid`] when the bytes are not a manifest
    /// of this format, or a package is malformed.
    pub fn parse(bytes: &[u8]) -> Result<Self, EngineStoreError> {
        let manifest: Manifest =
            serde_json::from_slice(bytes).map_err(|e| EngineStoreError::ManifestInvalid {
                reason: e.to_string(),
            })?;
        if manifest.format != MANIFEST_FORMAT {
            return Err(EngineStoreError::ManifestInvalid {
                reason: format!("unsupported format {}", manifest.format),
            });
        }
        for package in &manifest.packages {
            check_package(package)?;
        }
        Ok(manifest)
    }

    /// The newest package of `engine` and `variant` for `platform`.
    #[must_use]
    pub fn newest(&self, engine: Engine, variant: Variant, platform: &str) -> Option<&Package> {
        self.packages
            .iter()
            .filter(|p| p.engine == engine && p.platform == platform && p.variant == variant)
            .max_by(|a, b| compare_tags(&a.tag, &b.tag))
    }
}

fn check_package(package: &Package) -> Result<(), EngineStoreError> {
    let invalid = |reason: String| EngineStoreError::ManifestInvalid { reason };
    if !is_safe_component(&package.tag) {
        return Err(invalid(format!("unsafe tag {:?}", package.tag)));
    }
    if package.sha256.len() != 64
        || !package
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(format!("malformed sha256 for {}", package.tag)));
    }
    if !is_relative_archive_url(&package.url) {
        return Err(invalid(format!(
            "archive address {:?} is not relative",
            package.url
        )));
    }
    if archive_kind(&package.url).is_none() {
        return Err(invalid(format!("unsupported archive {:?}", package.url)));
    }
    if let Some(day) = &package.released {
        if !is_calendar_day(day) {
            return Err(invalid(format!(
                "release day {day:?} of {} is not YYYY-MM-DD",
                package.tag
            )));
        }
    }
    Ok(())
}

/// True for a `YYYY-MM-DD` day with a month from 01 to 12 and a day from 01
/// to 31.
fn is_calendar_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<u32>().ok();
    let digits = bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    digits
        && number(5..7).is_some_and(|m| (1..=12).contains(&m))
        && number(8..10).is_some_and(|d| (1..=31).contains(&d))
}

/// A name that can be used as one directory component.
pub(crate) fn is_safe_component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// A relative address without scheme, host, query or parent step.
fn is_relative_archive_url(url: &str) -> bool {
    !url.is_empty()
        && !url.contains("://")
        && !url.starts_with('/')
        && !url.contains('\\')
        && !url.contains('?')
        && !url.contains('#')
        && !url.contains(':')
        && url
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// The archive kinds an engine package may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArchiveKind {
    /// `.zip`.
    Zip,
}

/// The archive kind an address ends with. Only zip is read: a `.tar.gz` is
/// refused rather than parsed.
#[must_use]
pub fn archive_kind(url: &str) -> Option<ArchiveKind> {
    url.ends_with(".zip").then_some(ArchiveKind::Zip)
}

/// Order two upstream tags: `b<number>` tags by their number, `v<x>.<y>.<z>`
/// release tags by their numbers in turn, anything else by its bytes; a
/// numbered tag sorts after the others.
#[must_use]
pub fn compare_tags(a: &str, b: &str) -> std::cmp::Ordering {
    match (tag_numbers(a), tag_numbers(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

/// The numbers of a `b<number>` or `v<x>.<y>.<z>` tag, or `None` for any
/// other spelling.
fn tag_numbers(tag: &str) -> Option<Vec<u64>> {
    if let Some(n) = tag.strip_prefix('b') {
        return n.parse::<u64>().ok().map(|n| vec![n]);
    }
    let parts: Option<Vec<u64>> = tag
        .strip_prefix('v')?
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect();
    parts.filter(|p| p.len() == 3)
}

/// Verify `signature` over `bytes` with `trusted_key`, then parse the manifest
/// and check it belongs to `channel`.
///
/// The signature is checked first, on the exact bytes received: nothing in an
/// unverified manifest is read.
///
/// # Errors
///
/// [`EngineStoreError::Signature`] when the signature does not hold for this
/// key, [`EngineStoreError::ManifestInvalid`] for a malformed manifest, and
/// [`EngineStoreError::ChannelMismatch`] for a manifest of another channel.
pub fn verify_manifest(
    bytes: &[u8],
    signature: &str,
    trusted_key: &str,
    channel: Channel,
) -> Result<Manifest, EngineStoreError> {
    apollia_signing::distribution::verify_minisign(bytes, signature, trusted_key)
        .map_err(|source| EngineStoreError::Signature { source })?;
    let manifest = Manifest::parse(bytes)?;
    if manifest.channel != channel {
        return Err(EngineStoreError::ChannelMismatch {
            expected: channel.as_str(),
            found: manifest.channel.as_str(),
        });
    }
    Ok(manifest)
}

/// The platform this build runs on, as manifests spell it.
#[must_use]
pub fn current_platform() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    format!("{os}-{arch}")
}
