//! Machines described for the tests of the engine choice, and their ratings.

use apollia_llm::hardware::{CardProbe, GpuCard, GpuProfile, GpuVendor};

use super::{rate, Rating};
use crate::engine_store::Variant;

/// Every `llama-server` variant a Linux x64 channel offers.
const LINUX_X64: [Variant; 5] = [
    Variant::Cuda12,
    Variant::Cuda13,
    Variant::Rocm,
    Variant::Vulkan,
    Variant::Cpu,
];

fn card(vendor: GpuVendor) -> GpuCard {
    GpuCard {
        vendor,
        name: "card".to_owned(),
        vram_gb: Some(16.0),
        driver_version: None,
        driver_major: None,
        compute_capability: None,
        gfx_target: None,
        kfd_accessible: None,
        probe: CardProbe::Complete,
    }
}

fn rated(profile: &GpuProfile) -> Vec<Rating> {
    rate(profile, "linux-x64", &LINUX_X64, &|_| None)
}

/// A Linux machine with one NVIDIA card of compute capability `cc` under the
/// driver `driver`, a Vulkan loader beside it.
pub(crate) fn nvidia_on(cc: (u8, u8), driver: &str) -> Vec<Rating> {
    let mut c = card(GpuVendor::Nvidia);
    c.compute_capability = Some(cc);
    c.driver_version = Some(driver.to_owned());
    c.driver_major = driver.split('.').next().and_then(|m| m.parse().ok());
    rated(&GpuProfile {
        cards: vec![c],
        vulkan_loader: true,
        rocm_runtime: false,
    })
}

/// An Ada Lovelace card (8.9) under driver 620: CUDA 13 recommended, CUDA 12
/// compatible.
pub(crate) fn nvidia() -> Vec<Rating> {
    nvidia_on((8, 9), "620.10")
}

/// A Linux machine with an AMD card the ROCm build does not target, and a
/// Vulkan loader: Vulkan recommended.
pub(crate) fn amd_with_vulkan() -> Vec<Rating> {
    let mut c = card(GpuVendor::Amd);
    c.gfx_target = Some("gfx906".to_owned());
    rated(&GpuProfile {
        cards: vec![c],
        vulkan_loader: true,
        rocm_runtime: true,
    })
}

/// A Linux machine without a graphics card: the processor build recommended.
pub(crate) fn no_card() -> Vec<Rating> {
    rated(&GpuProfile {
        cards: Vec::new(),
        vulkan_loader: false,
        rocm_runtime: false,
    })
}
