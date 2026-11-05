//! The machine as the engine list sees it: its cards, the packages the channel
//! offers for its platform, and the rating of each `llama-server` variant.
//!
//! Probing is blocking and may hang on a driver in a bad state. Each probe is
//! bounded by its own limit; on top of that the route waits at most
//! [`ROUTE_DEADLINE`] for all of them, and answers without the hardware when
//! they are not done. A probe left running keeps its result for the next read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_llm::hardware::bounded::PROBE_LIMIT;
use apollia_llm::hardware::{CardProbe, GpuProfile, GpuVendor};
use serde::Serialize;

use crate::engine_store::compat::{self, Rating};
use crate::engine_store::{self, Engine, InstalledPackage, Manifest, Variant};

/// What the route waits, at most, for the hardware probes.
pub(crate) const ROUTE_DEADLINE: Duration = PROBE_LIMIT.saturating_add(Duration::from_secs(1));

/// Where the route reads the hardware: the probe of the machine, or a fake.
#[derive(Clone, Copy)]
pub(crate) struct HardwareSource {
    /// The kept profile, probed on first use. Blocking.
    pub(crate) profile: fn() -> GpuProfile,
    /// Probe again and keep the result. Blocking.
    pub(crate) refresh: fn() -> GpuProfile,
    /// Whether the build at a path lists a device. Blocking.
    pub(crate) sees_a_device: fn(&Path) -> Option<bool>,
}

/// The probes of this machine.
pub(crate) const SYSTEM: HardwareSource = HardwareSource {
    profile: apollia_llm::hardware::gpu_profile,
    refresh: apollia_llm::hardware::refresh_gpu_profile,
    sees_a_device: compat::sees_a_device,
};

/// The maker of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = GpuCardVendor)]
pub enum CardVendor {
    /// NVIDIA.
    Nvidia,
    /// AMD.
    Amd,
    /// Intel.
    Intel,
    /// Any other maker.
    Other,
}

/// One graphics card of the machine.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct HardwareCard {
    /// Its maker.
    pub vendor: CardVendor,
    /// Its name.
    pub name: String,
    /// Its dedicated memory in GiB, when known.
    pub vram_gb: Option<f64>,
    /// The NVIDIA driver version, `580.65.06`.
    pub driver_version: Option<String>,
    /// The NVIDIA compute capability, `8.9`.
    pub compute_capability: Option<String>,
    /// The gfx target of an AMD card, `gfx1100`.
    pub gfx_target: Option<String>,
    /// Whether `/dev/kfd` opens. AMD under Linux only.
    pub kfd_accessible: Option<bool>,
    /// True when the driver did not answer within the probe limit.
    pub timed_out: bool,
}

/// The cards of the machine and what is installed around them.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct HardwareView {
    /// Every card, NVIDIA ones first.
    pub cards: Vec<HardwareCard>,
    /// Whether a Vulkan loader is installed.
    pub vulkan_loader: bool,
    /// Whether the host's ROCm runtime is installed. Linux only.
    pub rocm_runtime: bool,
}

impl From<&GpuProfile> for HardwareView {
    fn from(profile: &GpuProfile) -> Self {
        let cards = profile
            .cards
            .iter()
            .map(|card| HardwareCard {
                vendor: match card.vendor {
                    GpuVendor::Nvidia => CardVendor::Nvidia,
                    GpuVendor::Amd => CardVendor::Amd,
                    GpuVendor::Intel => CardVendor::Intel,
                    GpuVendor::Other => CardVendor::Other,
                },
                name: card.name.clone(),
                vram_gb: card.vram_gb,
                driver_version: card.driver_version.clone(),
                compute_capability: card
                    .compute_capability
                    .map(|(major, minor)| format!("{major}.{minor}")),
                gfx_target: card.gfx_target.clone(),
                kfd_accessible: card.kfd_accessible,
                timed_out: card.probe == CardProbe::TimedOut,
            })
            .collect();
        Self {
            cards,
            vulkan_loader: profile.vulkan_loader,
            rocm_runtime: profile.rocm_runtime,
        }
    }
}

/// One package the channel offers for this platform.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AvailablePackage {
    /// Engine.
    pub engine: Engine,
    /// Upstream tag.
    pub tag: String,
    /// Variant.
    pub variant: Variant,
    /// Archive size in bytes.
    pub size: u64,
    /// Day the build was published, when the manifest states it.
    pub released: Option<String>,
    /// Whether this exact build is installed here.
    pub installed: bool,
}

/// What the probes gave within the deadline.
#[derive(Debug, Default)]
pub(crate) struct HostFacts {
    /// The profile, `None` when the probe was not done in time.
    pub(crate) profile: Option<GpuProfile>,
    /// Per installed `llama-server` GPU variant, whether its build lists a
    /// device; absent when it was not asked or did not answer in time.
    pub(crate) listings: HashMap<Variant, bool>,
}

/// The newest installed build of each `llama-server` GPU variant, by binary.
fn installed_gpu_builds(installed: &[InstalledPackage]) -> Vec<(Variant, PathBuf)> {
    let name = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let mut newest: HashMap<Variant, &InstalledPackage> = HashMap::new();
    for package in installed
        .iter()
        .filter(|p| p.engine == Engine::LlamaServer && p.variant != Variant::Cpu)
    {
        let newer = newest.get(&package.variant).is_none_or(|kept| {
            engine_store::compare_tags(&package.tag, &kept.tag) == std::cmp::Ordering::Greater
        });
        if newer {
            newest.insert(package.variant, package);
        }
    }
    newest
        .into_iter()
        .filter_map(|(variant, p)| {
            let binary = if p.path.is_file() {
                p.path.clone()
            } else {
                engine_store::find_binary(&p.path, &name)?
            };
            Some((variant, binary))
        })
        .collect()
}

/// Probe the machine through `source`, `refresh` asking for a new profile,
/// and wait for the answers until [`ROUTE_DEADLINE`] at most.
pub(crate) async fn host_facts(
    source: HardwareSource,
    refresh: bool,
    installed: &[InstalledPackage],
) -> HostFacts {
    let deadline = tokio::time::Instant::now() + ROUTE_DEADLINE;
    let read = if refresh {
        source.refresh
    } else {
        source.profile
    };
    let profile = tokio::task::spawn_blocking(read);
    let listings: Vec<_> = installed_gpu_builds(installed)
        .into_iter()
        .map(|(variant, binary)| {
            let ask = source.sees_a_device;
            (variant, tokio::task::spawn_blocking(move || ask(&binary)))
        })
        .collect();
    let profile = match tokio::time::timeout_at(deadline, profile).await {
        Ok(Ok(profile)) => Some(profile),
        Ok(Err(error)) => {
            tracing::event!(tracing::Level::WARN, error = %error, "engines.hardware.probe_failed");
            None
        }
        Err(_) => {
            tracing::event!(
                tracing::Level::WARN,
                deadline_ms = ROUTE_DEADLINE.as_millis(),
                "engines.hardware.probe_late"
            );
            None
        }
    };
    let mut seen = HashMap::new();
    for (variant, job) in listings {
        if let Ok(Ok(Some(sees))) = tokio::time::timeout_at(deadline, job).await {
            seen.insert(variant, sees);
        }
    }
    HostFacts {
        profile,
        listings: seen,
    }
}

/// The packages `manifest` offers for `platform`, each marked installed or not.
pub(crate) fn available(
    manifest: &Manifest,
    platform: &str,
    installed: &[InstalledPackage],
) -> Vec<AvailablePackage> {
    manifest
        .packages
        .iter()
        .filter(|p| p.platform == platform)
        .map(|p| AvailablePackage {
            engine: p.engine,
            tag: p.tag.clone(),
            variant: p.variant,
            size: p.size,
            released: p.released.clone(),
            installed: installed
                .iter()
                .any(|i| i.engine == p.engine && i.tag == p.tag && i.variant == p.variant),
        })
        .collect()
}

/// The rating of each `llama-server` variant, empty without a profile or a
/// manifest: a verdict on unknown hardware or an unknown channel would be a
/// guess.
pub(crate) fn ratings(
    facts: &HostFacts,
    manifest: Option<&Manifest>,
    platform: &str,
) -> Vec<Rating> {
    let (Some(profile), Some(manifest)) = (&facts.profile, manifest) else {
        return Vec::new();
    };
    let offered = compat::offered_variants(manifest, Engine::LlamaServer, platform);
    compat::rate(profile, platform, &offered, &|v| {
        facts.listings.get(&v).copied()
    })
}

/// The name of the NVIDIA card the CUDA builds are rated for, when there is one.
pub(crate) fn nvidia_card(profile: Option<&GpuProfile>) -> Option<String> {
    profile?
        .cards
        .iter()
        .filter(|c| c.vendor == GpuVendor::Nvidia)
        .max_by(|a, b| a.compute_capability.cmp(&b.compute_capability))
        .map(|c| c.name.clone())
}
