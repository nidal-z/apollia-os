//! Which `llama-server` variant suits this machine, and why the others do not.
//!
//! Each variant is rated `recommended`, `compatible` or `incompatible` from the
//! cards the hardware probe found ([`apollia_llm::hardware::GpuProfile`]), the
//! platform, and the variants the verified manifest offers for it. One variant
//! at most is recommended: the best one this machine can run, the processor
//! build when nothing else qualifies.
//!
//! The thresholds follow what each upstream build was compiled with:
//!
//! - CUDA 13 starts at compute capability 7.5 and wants a driver of the 580
//!   family or later; it is recommended from R615, the driver its toolkit asks
//!   for, and compatible between the two.
//! - CUDA 12 is CUDA 12.8 under Linux (driver 570.26 or later) and CUDA 12.4
//!   under Windows (551.61 or later). Below that, down to the floor of the CUDA
//!   12 family (525 under Linux, 527.41 under Windows), a card with native code
//!   in the build is still compatible; a card served by PTX is not, since PTX
//!   needs the driver of the toolkit. Windows CUDA 12.4 has no native code for
//!   cards past compute capability 9.0, and is at best compatible on them.
//! - ROCm needs the gfx target of the card in the list of the build, and, under
//!   Linux, `/dev/kfd` and the host's ROCm runtime. Under Windows the gfx target
//!   is not probed, and the build is compatible, to be confirmed after
//!   installation.
//! - Vulkan needs a loader. It is recommended for AMD, Intel and other cards,
//!   compatible beside an NVIDIA card.
//! - The processor build is always compatible.
//!
//! After installation, a GPU build whose `--list-devices` lists nothing is
//! incompatible whatever the rest says.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use apollia_llm::hardware::{CardProbe, GpuCard, GpuProfile, GpuVendor};
use serde::Serialize;

pub(crate) use listings::RATING_DEADLINE;
use listings::{Listings, Probes, LISTINGS};

use crate::engine_store::{
    cached_manifest, compare_tags, current_platform, find_binary, installed_packages, Engine,
    EngineStoreSettings, InstalledPackage, Manifest, Variant,
};

/// Driver of the CUDA 13 family floor.
const CUDA13_FLOOR: &str = "580";
/// Driver the CUDA 13.4 toolkit asks for.
const CUDA13_TOOLKIT: &str = "615";
/// Driver the Linux CUDA 12.8 toolkit asks for.
const CUDA12_LINUX_TOOLKIT: &str = "570.26";
/// Driver the Windows CUDA 12.4 toolkit asks for.
const CUDA12_WINDOWS_TOOLKIT: &str = "551.61";
/// Floor of the CUDA 12 family under Linux.
const CUDA12_LINUX_FLOOR: &str = "525";
/// Floor of the CUDA 12 family under Windows.
const CUDA12_WINDOWS_FLOOR: &str = "527.41";

/// gfx targets of the upstream Linux ROCm build (`ubuntu-24-rocm`).
const ROCM_LINUX_GFX: &[&str] = &[
    "gfx908", "gfx90a", "gfx942", "gfx950", "gfx1010", "gfx1011", "gfx1012", "gfx1030", "gfx1031",
    "gfx1032", "gfx1033", "gfx1034", "gfx1035", "gfx1036", "gfx1100", "gfx1101", "gfx1102",
    "gfx1103", "gfx1150", "gfx1151", "gfx1152", "gfx1200", "gfx1201",
];

/// gfx targets of the upstream Windows ROCm build (`windows-rocm`).
const ROCM_WINDOWS_GFX: &[&str] = &[
    "gfx1010", "gfx1011", "gfx1012", "gfx1030", "gfx1031", "gfx1032", "gfx1033", "gfx1034",
    "gfx1035", "gfx1036", "gfx1100", "gfx1101", "gfx1102", "gfx1103", "gfx1150", "gfx1151",
    "gfx1152", "gfx1153", "gfx1200", "gfx1201",
];

/// Order in which variants are preferred, the most capable first: the order
/// the automatic choice takes them in.
const PREFERENCE: [Variant; 6] = crate::engine_store::AUTOMATIC_ORDER;

/// How well a variant suits this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = EngineVerdict)]
pub enum Verdict {
    /// The one variant to install.
    Recommended,
    /// Runs here, with a better choice or a caveat.
    Compatible,
    /// Does not run here.
    Incompatible,
}

/// Why a variant is not recommended, as a code the interface translates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
#[schema(as = EngineRatingReason)]
pub enum Reason {
    /// The NVIDIA driver is older than the build needs.
    DriverTooOld {
        /// The driver of the machine, as `nvidia-smi` prints it.
        have: String,
        /// The driver the build needs.
        need: String,
    },
    /// CUDA 13 dropped the architecture of the card (Maxwell, Pascal, Volta).
    ArchDroppedInCuda13 {
        /// The compute capability of the card, `6.1`.
        cc: String,
    },
    /// The gfx target of the card is not in the list of the build.
    GfxNotInBuild {
        /// The gfx target, `gfx906`.
        gfx: String,
    },
    /// No Vulkan loader is installed.
    NoVulkanLoader,
    /// An NVIDIA card is there, but no driver answers for it.
    NoNvidiaDriver,
    /// The channel offers no build of this variant for the platform.
    NotBuiltForPlatform,
    /// No card of the kind this build drives.
    NoGpu,
    /// The installed build lists no device with `--list-devices`.
    EngineSeesNoDevice,
    /// The host has no ROCm runtime (`libamdhip64`), which the Linux ROCm
    /// build loads from the machine.
    NoRocmRuntime,
}

/// The verdict on one variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[schema(as = EngineRating)]
pub struct Rating {
    /// Variant rated.
    pub variant: Variant,
    /// How well it suits this machine.
    pub verdict: Verdict,
    /// Why it is not recommended, or the caveat of a recommendation no build
    /// fully qualified for, when a code says it.
    pub reason: Option<Reason>,
}

/// The operating system a platform names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Os {
    Linux,
    Windows,
    Macos,
}

impl Os {
    fn of(platform: &str) -> Self {
        match platform.split('-').next() {
            Some("windows") => Self::Windows,
            Some("macos") => Self::Macos,
            _ => Self::Linux,
        }
    }
}

/// What one variant is worth before the one recommendation is picked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fit {
    /// Could be the recommended one.
    Best,
    /// Runs, not the best choice, with a reason when a code says it.
    Runs(Option<Reason>),
    /// Does not run.
    Fails(Reason),
}

/// The variants of `engine` the manifest offers for `platform`.
#[must_use]
pub fn offered_variants(manifest: &Manifest, engine: Engine, platform: &str) -> Vec<Variant> {
    Variant::ALL
        .into_iter()
        .filter(|v| {
            manifest
                .packages
                .iter()
                .any(|p| p.engine == engine && p.variant == *v && p.platform == platform)
        })
        .collect()
}

/// Rate every `llama-server` variant for this machine.
///
/// `offered` are the variants the verified manifest has for `platform`; any
/// other is `not_built_for_platform`. `sees_device` tells, for an installed
/// build, whether its `--list-devices` listed a device; `None` when it was not
/// asked.
#[must_use]
pub fn rate(
    profile: &GpuProfile,
    platform: &str,
    offered: &[Variant],
    sees_device: &dyn Fn(Variant) -> Option<bool>,
) -> Vec<Rating> {
    let os = Os::of(platform);
    let fits: Vec<(Variant, Fit)> = Variant::ALL
        .into_iter()
        .map(|variant| {
            let fit = if offered.contains(&variant) {
                match (variant, sees_device(variant)) {
                    (v, Some(false)) if v != Variant::Cpu => Fit::Fails(Reason::EngineSeesNoDevice),
                    _ => fit(variant, profile, os),
                }
            } else {
                Fit::Fails(Reason::NotBuiltForPlatform)
            };
            (variant, fit)
        })
        .collect();
    let pick = |wanted: fn(&Fit) -> bool| {
        PREFERENCE.into_iter().find(|v| {
            fits.iter()
                .any(|(variant, fit)| variant == v && wanted(fit))
        })
    };
    let recommended = pick(|f| *f == Fit::Best).or_else(|| pick(|f| matches!(f, Fit::Runs(_))));
    fits.into_iter()
        .map(|(variant, fit)| {
            let (verdict, reason) = match fit {
                // A recommendation taken among the builds that only run keeps
                // its caveat: the driver to update, for one.
                Fit::Runs(reason) if Some(variant) == recommended => (Verdict::Recommended, reason),
                _ if Some(variant) == recommended => (Verdict::Recommended, None),
                Fit::Best => (Verdict::Compatible, None),
                Fit::Runs(reason) => (Verdict::Compatible, reason),
                Fit::Fails(reason) => (Verdict::Incompatible, Some(reason)),
            };
            Rating {
                variant,
                verdict,
                reason,
            }
        })
        .collect()
}

fn fit(variant: Variant, profile: &GpuProfile, os: Os) -> Fit {
    match variant {
        Variant::Metal => {
            if os == Os::Macos {
                Fit::Best
            } else {
                Fit::Fails(Reason::NotBuiltForPlatform)
            }
        }
        Variant::Cuda13 => nvidia_fit(profile, cuda13_fit),
        Variant::Cuda12 => nvidia_fit(profile, |card, driver| cuda12_fit(card, driver, os)),
        Variant::Rocm => rocm_fit(profile, os),
        Variant::Vulkan => vulkan_fit(profile),
        Variant::Cpu => Fit::Runs(None),
    }
}

/// The NVIDIA card the CUDA builds are rated for: the highest compute
/// capability, then the most memory.
fn best_nvidia(profile: &GpuProfile) -> Option<&GpuCard> {
    profile
        .cards
        .iter()
        .filter(|c| c.vendor == GpuVendor::Nvidia)
        .max_by(|a, b| {
            a.compute_capability.cmp(&b.compute_capability).then(
                a.vram_gb
                    .unwrap_or_default()
                    .total_cmp(&b.vram_gb.unwrap_or_default()),
            )
        })
}

/// A CUDA build rated by `rule` on the best NVIDIA card and its driver.
fn nvidia_fit(profile: &GpuProfile, rule: impl Fn(&GpuCard, &str) -> Fit) -> Fit {
    let Some(card) = best_nvidia(profile) else {
        return Fit::Fails(Reason::NoGpu);
    };
    match (&card.driver_version, card.probe) {
        (Some(driver), _) => rule(card, driver),
        // The driver hung on its query: the card is there, nothing proves the
        // build runs, nothing proves it does not.
        (None, CardProbe::TimedOut) => Fit::Runs(None),
        (None, CardProbe::Complete) => Fit::Fails(Reason::NoNvidiaDriver),
    }
}

fn too_old(have: &str, need: &str) -> Reason {
    Reason::DriverTooOld {
        have: have.to_owned(),
        need: need.to_owned(),
    }
}

fn cuda13_fit(card: &GpuCard, driver: &str) -> Fit {
    if let Some((major, minor)) = card.compute_capability {
        if (major, minor) < (7, 5) {
            return Fit::Fails(Reason::ArchDroppedInCuda13 {
                cc: format!("{major}.{minor}"),
            });
        }
    }
    if driver_below(driver, CUDA13_FLOOR) {
        return Fit::Fails(too_old(driver, CUDA13_FLOOR));
    }
    if driver_below(driver, CUDA13_TOOLKIT) {
        return Fit::Runs(Some(too_old(driver, CUDA13_TOOLKIT)));
    }
    if card.compute_capability.is_none() {
        // The driver refused the `compute_cap` field: the card may predate
        // Turing, which the device listing after installation tells.
        return Fit::Runs(None);
    }
    Fit::Best
}

fn cuda12_fit(card: &GpuCard, driver: &str, os: Os) -> Fit {
    let (toolkit, floor) = if os == Os::Windows {
        (CUDA12_WINDOWS_TOOLKIT, CUDA12_WINDOWS_FLOOR)
    } else {
        (CUDA12_LINUX_TOOLKIT, CUDA12_LINUX_FLOOR)
    };
    if driver_below(driver, floor) {
        return Fit::Fails(too_old(driver, floor));
    }
    let native = card
        .compute_capability
        .is_some_and(|cc| has_native_code(cc, os));
    if driver_below(driver, toolkit) {
        return if native {
            Fit::Runs(Some(too_old(driver, toolkit)))
        } else {
            Fit::Fails(too_old(driver, toolkit))
        };
    }
    let past_the_build = card
        .compute_capability
        .is_some_and(|(major, _)| major >= 10);
    if past_the_build && !native {
        return Fit::Runs(None);
    }
    Fit::Best
}

/// Whether the CUDA 12 build of `os` carries native code (`-real`) for `cc`:
/// `86-real` and `89-real` serve compute capability 8.6 to 8.9, and the Linux
/// 12.8 build adds `120a-real`. Every other card runs on PTX.
fn has_native_code(cc: (u8, u8), os: Os) -> bool {
    let (major, minor) = cc;
    (major == 8 && minor >= 6) || (os == Os::Linux && cc == (12, 0))
}

/// Whether `have` is an older driver than `need`, field by field.
fn driver_below(have: &str, need: &str) -> bool {
    let fields = |v: &str| -> Vec<u32> {
        v.split('.')
            .map(|f| f.trim().parse().unwrap_or_default())
            .collect()
    };
    let (have, need) = (fields(have), fields(need));
    for at in 0..have.len().max(need.len()) {
        let (h, n) = (
            have.get(at).copied().unwrap_or_default(),
            need.get(at).copied().unwrap_or_default(),
        );
        if h != n {
            return h < n;
        }
    }
    false
}

fn rocm_fit(profile: &GpuProfile, os: Os) -> Fit {
    let Some(card) = profile.cards.iter().find(|c| c.vendor == GpuVendor::Amd) else {
        return Fit::Fails(Reason::NoGpu);
    };
    let build = if os == Os::Windows {
        ROCM_WINDOWS_GFX
    } else {
        ROCM_LINUX_GFX
    };
    if let Some(gfx) = card.gfx_target.as_deref() {
        if !build.contains(&gfx) {
            return Fit::Fails(Reason::GfxNotInBuild {
                gfx: gfx.to_owned(),
            });
        }
    }
    if os == Os::Windows {
        return Fit::Runs(None);
    }
    if !profile.rocm_runtime {
        return Fit::Fails(Reason::NoRocmRuntime);
    }
    if card.gfx_target.is_none() {
        return Fit::Runs(None);
    }
    if card.kfd_accessible == Some(true) {
        Fit::Best
    } else {
        Fit::Runs(None)
    }
}

fn vulkan_fit(profile: &GpuProfile) -> Fit {
    if profile.cards.is_empty() {
        return Fit::Fails(Reason::NoGpu);
    }
    if !profile.vulkan_loader {
        return Fit::Fails(Reason::NoVulkanLoader);
    }
    if profile.cards.iter().any(|c| c.vendor == GpuVendor::Nvidia) {
        Fit::Runs(None)
    } else {
        Fit::Best
    }
}

/// The newest installed build of each `llama-server` GPU variant, by binary.
pub(crate) fn installed_gpu_builds(installed: &[InstalledPackage]) -> Vec<(Variant, PathBuf)> {
    let name = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let mut newest: HashMap<Variant, &InstalledPackage> = HashMap::new();
    for package in installed
        .iter()
        .filter(|p| p.engine == Engine::LlamaServer && p.variant != Variant::Cpu)
    {
        let newer = newest
            .get(&package.variant)
            .is_none_or(|kept| compare_tags(&package.tag, &kept.tag) == Ordering::Greater);
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
                find_binary(&p.path, &name)?
            };
            Some((variant, binary))
        })
        .collect()
}

/// The ratings of this machine the launcher chooses by, the store being the
/// one under `root`: the cards of the kept hardware profile, against the
/// variants the last verified manifest of the followed channel offers for
/// this platform and those installed, each installed GPU build asked whether
/// it lists a device. The same ratings as the engine list.
///
/// Blocking, for [`RATING_DEADLINE`] at most: the profile and every build
/// are probed at once, a build that has not answered by then is taken as not
/// asked, and without a profile in time nothing is rated. Call from
/// `spawn_blocking`.
#[must_use]
pub fn machine_ratings(root: &Path) -> Vec<Rating> {
    machine_ratings_with(root, listings::SYSTEM, &LISTINGS, RATING_DEADLINE)
}

/// [`machine_ratings`] through `probes`, its answers kept in `cache`.
pub(crate) fn machine_ratings_with(
    root: &Path,
    probes: Probes,
    cache: &Listings,
    deadline: std::time::Duration,
) -> Vec<Rating> {
    let platform = current_platform();
    let installed = installed_packages(root);
    let mut offered: Vec<Variant> = installed
        .iter()
        .filter(|p| p.engine == Engine::LlamaServer)
        .map(|p| p.variant)
        .collect();
    let manifest = apollia_signing::distribution::distribution_public_key().and_then(|key| {
        let channel = EngineStoreSettings::load(root).ok()?.channel;
        cached_manifest(root, channel, key)
    });
    if let Some(manifest) = manifest {
        offered.extend(offered_variants(&manifest, Engine::LlamaServer, &platform));
    }
    let facts = listings::facts_within(installed_gpu_builds(&installed), probes, cache, deadline);
    let Some(profile) = facts.profile else {
        return Vec::new();
    };
    rate(&profile, &platform, &offered, &|v| {
        facts.listings.get(&v).copied()
    })
}

/// Whether the build at `binary` lists a device with `--list-devices`, asked
/// once per process: the answer only changes when the build or the driver
/// does, and is dropped by [`forget_device_listings`], which the channel
/// check calls. `None` when the build could not be run: a build that does not
/// start is not a build that sees nothing.
///
/// Blocking, up to the probe limit: call from `spawn_blocking`.
#[must_use]
pub fn sees_a_device(binary: &Path) -> Option<bool> {
    listings::kept_listing(&LISTINGS, binary)
}

/// Drop every kept device listing, so the next rating asks each build again.
pub fn forget_device_listings() {
    LISTINGS.forget();
}

mod listings;

#[cfg(test)]
pub(crate) mod machines;
#[cfg(test)]
mod tests;
