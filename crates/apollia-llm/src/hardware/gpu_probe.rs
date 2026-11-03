//! Every graphics card of the machine, its vendor, its driver and what that
//! driver allows.
//!
//! The choice of an engine (CUDA 12 or 13, ROCm, Vulkan, processor) depends on
//! facts the accelerator profile of [`super::detect`] does not carry: the
//! vendor of each card, the driver version of an NVIDIA card, the gfx target of
//! an AMD one, whether the ROCm device node can be opened, whether a Vulkan
//! loader is installed. This module gathers them, with no new dependency:
//! `nvidia-smi`, sysfs under Linux, one PowerShell query under Windows, and a
//! few file checks for Vulkan. Every external probe is bounded by
//! [`super::bounded::PROBE_LIMIT`].

mod linux;
mod nvidia;
mod vulkan;
mod windows;

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::bounded::PROBE_LIMIT;

/// The maker of a card, read from its PCI vendor identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    /// PCI vendor `0x10de`.
    Nvidia,
    /// PCI vendor `0x1002`.
    Amd,
    /// PCI vendor `0x8086`.
    Intel,
    /// Any other vendor.
    Other,
}

impl GpuVendor {
    /// The vendor a PCI vendor identifier names.
    #[must_use]
    pub fn from_pci_id(id: u16) -> Self {
        match id {
            0x10de => Self::Nvidia,
            0x1002 => Self::Amd,
            0x8086 => Self::Intel,
            _ => Self::Other,
        }
    }
}

/// Whether what is known of a card is everything its probe can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardProbe {
    /// The probe answered.
    Complete,
    /// The probe did not answer within its limit and was killed: the card is
    /// there, what it is and what its driver allows are not known.
    TimedOut,
}

/// One graphics card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuCard {
    /// Its maker.
    pub vendor: GpuVendor,
    /// Its name as the driver or the operating system gives it.
    pub name: String,
    /// Its dedicated memory in GiB, when the probe reports it.
    pub vram_gb: Option<f64>,
    /// The driver version, as `nvidia-smi` prints it (`580.65.06`). NVIDIA only.
    pub driver_version: Option<String>,
    /// The major part of the driver version (`580`). NVIDIA only.
    pub driver_major: Option<u32>,
    /// The compute capability as `(major, minor)`. NVIDIA only, and unknown
    /// when the driver refuses the `compute_cap` field.
    pub compute_capability: Option<(u8, u8)>,
    /// The gfx target of an AMD card (`gfx1100`), when the kernel publishes it.
    pub gfx_target: Option<String>,
    /// Whether `/dev/kfd`, the device node ROCm runs through, can be opened.
    /// AMD under Linux only.
    pub kfd_accessible: Option<bool>,
    /// Whether the probe of this card answered.
    pub probe: CardProbe,
}

impl GpuCard {
    /// A card of `vendor` named `name`, with nothing else known yet.
    fn named(vendor: GpuVendor, name: String) -> Self {
        Self {
            vendor,
            name,
            vram_gb: None,
            driver_version: None,
            driver_major: None,
            compute_capability: None,
            gfx_target: None,
            kfd_accessible: None,
            probe: CardProbe::Complete,
        }
    }
}

/// What the machine offers an inference engine.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GpuProfile {
    /// Every card found, NVIDIA ones first.
    pub cards: Vec<GpuCard>,
    /// Whether a Vulkan loader is installed. Says nothing of the driver behind
    /// it: only the engine's own device listing proves that.
    pub vulkan_loader: bool,
}

/// Where the Vulkan loader is looked for.
#[derive(Debug, Clone)]
pub(crate) enum VulkanSource {
    /// Not looked for.
    None,
    /// Library and ICD manifests under this root (`/` on a real machine).
    Linux(PathBuf),
    /// `System32\vulkan-1.dll` under this system root.
    Windows(PathBuf),
}

/// Where each probe reads. [`ProbeSources::system`] is the machine; tests point
/// every field at a fake.
#[derive(Debug, Clone)]
pub(crate) struct ProbeSources {
    /// The `nvidia-smi` program, or `None` to skip the NVIDIA probe.
    pub nvidia_smi: Option<PathBuf>,
    /// The root sysfs and `/dev` are read under, or `None` off Linux.
    pub sysfs_root: Option<PathBuf>,
    /// Whether to ask Windows for its display adapters.
    pub windows_adapters: bool,
    /// Where the Vulkan loader is looked for.
    pub vulkan: VulkanSource,
    /// The limit of each external probe.
    pub limit: Duration,
}

impl ProbeSources {
    /// The probes of the machine this process runs on.
    pub(crate) fn system() -> Self {
        let nvidia_smi = (!cfg!(target_os = "macos")).then(|| PathBuf::from("nvidia-smi"));
        let sysfs_root = cfg!(target_os = "linux").then(|| PathBuf::from("/"));
        let vulkan = if cfg!(target_os = "linux") {
            VulkanSource::Linux(PathBuf::from("/"))
        } else if cfg!(windows) {
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
            VulkanSource::Windows(PathBuf::from(root))
        } else {
            VulkanSource::None
        };
        Self {
            nvidia_smi,
            sysfs_root,
            windows_adapters: cfg!(windows),
            vulkan,
            limit: PROBE_LIMIT,
        }
    }
}

/// Probe every source once.
pub(crate) fn probe_with(sources: &ProbeSources) -> GpuProfile {
    let mut cards = sources
        .nvidia_smi
        .as_deref()
        .map(|program| nvidia::cards(program, sources.limit))
        .unwrap_or_default();
    // A machine whose `nvidia-smi` answered, or hung, has its NVIDIA cards
    // described by it; the operating system then only adds the others. When it
    // is absent, an NVIDIA card the system still sees is listed without driver.
    let skip_nvidia = !cards.is_empty();
    if let Some(root) = sources.sysfs_root.as_deref() {
        cards.extend(linux::cards(root, skip_nvidia));
    }
    if sources.windows_adapters {
        cards.extend(windows::cards(sources.limit, skip_nvidia));
    }
    let vulkan_loader = match &sources.vulkan {
        VulkanSource::None => false,
        VulkanSource::Linux(root) => vulkan::linux_loader(root),
        VulkanSource::Windows(system_root) => vulkan::windows_loader(system_root),
    };
    GpuProfile {
        cards,
        vulkan_loader,
    }
}

/// The profile, probed once and kept until refreshed.
pub(crate) struct ProfileCache {
    slot: RwLock<Option<GpuProfile>>,
}

impl ProfileCache {
    /// An empty cache.
    pub(crate) const fn new() -> Self {
        Self {
            slot: RwLock::new(None),
        }
    }

    /// The kept profile, probed from `sources` on first use.
    pub(crate) fn get_or_probe(&self, sources: &ProbeSources) -> GpuProfile {
        if let Some(profile) = self.slot.read().ok().and_then(|slot| slot.clone()) {
            return profile;
        }
        // Probed under the write lock: two first readers wait on one probe
        // rather than spawning `nvidia-smi` twice.
        let mut slot = self
            .slot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.get_or_insert_with(|| probe_with(sources)).clone()
    }

    /// Probe `sources` again and keep the new profile.
    pub(crate) fn refresh(&self, sources: &ProbeSources) -> GpuProfile {
        let profile = probe_with(sources);
        let mut slot = self
            .slot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = Some(profile.clone());
        profile
    }
}

static CACHE: ProfileCache = ProfileCache::new();

/// The cards of this machine, probed on first call and kept for the process.
///
/// Blocking, up to a few seconds on a first call whose driver hangs: call from
/// `tokio::task::spawn_blocking` on an async path.
#[must_use]
pub fn gpu_profile() -> GpuProfile {
    CACHE.get_or_probe(&ProbeSources::system())
}

/// Probe the cards of this machine again, after a driver was installed, and
/// keep the result for the next [`gpu_profile`]. Blocking like it.
#[must_use]
pub fn refresh_gpu_profile() -> GpuProfile {
    CACHE.refresh(&ProbeSources::system())
}

#[cfg(test)]
mod tests;
