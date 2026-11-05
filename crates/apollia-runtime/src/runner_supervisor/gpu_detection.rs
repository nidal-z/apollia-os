//! Cross-platform GPU detection at daemon boot.
//!
//! At startup, the `RunnerSupervisor` queries [`detect_gpu`] to get the local
//! GPU's characteristics, then applies [`boot_backend`] to produce the final
//! `RunnerBackend`, degraded onto a runner this build ships. No setting
//! overrides the choice.
//!
//! The decision hierarchy is: Apple Silicon, NVIDIA with a driver of the CUDA
//! 12 family, AMD with ROCm, Vulkan when its loader is installed, CPU.
//!
//! Under Linux and Windows nothing is probed here: the cards, their drivers
//! and the Vulkan loader are those of the hardware probe of the language-model
//! crate ([`apollia_llm::hardware::gpu_profile`]), read once per process and
//! shared with the profile the model budget and the engine choice read, so
//! dictation and the models never describe two different machines. macOS
//! keeps its own `system_profiler` read. No telemetry, no network call.

#[cfg(any(target_os = "linux", target_os = "windows", test))]
use apollia_llm::hardware::{GpuCard, GpuProfile, GpuVendor};
use serde::{Deserialize, Serialize};

// Public types

/// Identity of the detected GPU vendor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    /// NVIDIA (PCI vendor ID `0x10de`).
    Nvidia,
    /// AMD / ATI (PCI vendor ID `0x1002`).
    Amd,
    /// Intel (PCI vendor ID `0x8086`).
    Intel,
    /// Apple Silicon (M1/M2/M3/M4+).
    Apple,
    /// Unidentified vendor or no GPU detected.
    Unknown,
}

/// Inference backend chosen to spawn the sidecar runner.
///
/// The matching `binary_name()` lives in the same directory as `apollia-os`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunnerBackend {
    /// NVIDIA CUDA (a driver of the CUDA 12 family, 525 or later).
    Cuda,
    /// AMD ROCm (Linux with `/dev/kfd` and a known gfx target; Windows).
    Rocm,
    /// Vulkan compute (cross-vendor fallback, when the loader is installed).
    Vulkan,
    /// Apple Metal (macOS arm64).
    Metal,
    /// Pure CPU (ultimate fallback, always available).
    Cpu,
}

impl RunnerBackend {
    /// Name of the matching runner binary.
    ///
    /// Used by `RunnerSupervisor::spawn` to resolve the sidecar path in the
    /// same directory as `apollia-os`.
    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::Cuda => "apollia-runner-cuda",
            Self::Rocm => "apollia-runner-rocm",
            Self::Vulkan => "apollia-runner-vulkan",
            Self::Metal => "apollia-runner-metal",
            Self::Cpu => "apollia-runner-cpu",
        }
    }
}

/// GPU information collected by [`detect_gpu`].
///
/// The `recommended_backend` field reflects the auto-detection decision, which
/// [`boot_backend`] degrades onto a runner this build ships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Identified vendor.
    pub vendor: Vendor,
    /// Human-readable model name (e.g. `"GeForce RTX 4070"`, `"Apple M3"`).
    pub model: String,
    /// Total memory in megabytes. `0` if unavailable.
    pub memory_total_mb: u32,
    /// Backend recommended by auto-detection.
    pub recommended_backend: RunnerBackend,
}

impl GpuInfo {
    /// CPU fallback constructor when detection failed or no GPU is present.
    pub fn cpu_fallback() -> Self {
        Self {
            vendor: Vendor::Unknown,
            model: "No GPU detected, CPU fallback".into(),
            memory_total_mb: 0,
            recommended_backend: RunnerBackend::Cpu,
        }
    }
}

// detect_gpu: cross-platform entry point

/// Detect the available GPU and return its characteristics.
///
/// Detection is cfg-gated per OS. Always best-effort: returns
/// [`GpuInfo::cpu_fallback`] rather than panicking if no probe succeeds.
#[cfg(target_os = "macos")]
pub fn detect_gpu() -> GpuInfo {
    #[cfg(target_arch = "aarch64")]
    {
        let model = query_system_profiler_chip_name().unwrap_or_else(|| "Apple Silicon".into());
        let memory_total_mb = query_system_profiler_unified_memory_mb().unwrap_or(0);
        let info = GpuInfo {
            vendor: Vendor::Apple,
            model,
            memory_total_mb,
            recommended_backend: RunnerBackend::Metal,
        };
        tracing::info!(
            vendor = ?info.vendor,
            model = %info.model,
            memory_mb = info.memory_total_mb,
            backend = ?info.recommended_backend,
            "gpu.detected",
        );
        info
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        tracing::warn!(
            detail = "macOS x86_64, falling back to CPU",
            "gpu.platform.unsupported"
        );
        GpuInfo::cpu_fallback()
    }
}

/// Detect the GPU on Linux and Windows, from the hardware probe the whole
/// process shares.
///
/// Blocking on the first call of the process, for up to a few seconds when a
/// driver hangs; every later call reads the kept profile.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn detect_gpu() -> GpuInfo {
    let platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let info = info_from_profile(&apollia_llm::hardware::gpu_profile(), platform);
    log_detected(&info);
    info
}

/// The operating system a profile was read on: ROCm is decided differently
/// under each.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Linux,
    Windows,
}

/// The lowest driver major of the CUDA 12 family, the runner's CUDA build.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
const CUDA_12_MIN_DRIVER: u32 = 525;

/// The card the runner runs on, and the backend recommended for it.
///
/// The best card is NVIDIA before AMD before Intel before any other, then the
/// one with the most memory. A machine without a card falls back to the
/// processor.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn info_from_profile(profile: &GpuProfile, platform: Platform) -> GpuInfo {
    let rank = |vendor: GpuVendor| match vendor {
        GpuVendor::Nvidia => 0,
        GpuVendor::Amd => 1,
        GpuVendor::Intel => 2,
        GpuVendor::Other => 3,
    };
    let best = profile.cards.iter().min_by(|a, b| {
        rank(a.vendor).cmp(&rank(b.vendor)).then_with(|| {
            b.vram_gb
                .unwrap_or(0.0)
                .total_cmp(&a.vram_gb.unwrap_or(0.0))
        })
    });
    let Some(card) = best else {
        return GpuInfo::cpu_fallback();
    };
    let vendor = match card.vendor {
        GpuVendor::Nvidia => Vendor::Nvidia,
        GpuVendor::Amd => Vendor::Amd,
        GpuVendor::Intel => Vendor::Intel,
        GpuVendor::Other => Vendor::Unknown,
    };
    GpuInfo {
        vendor,
        model: card.name.clone(),
        memory_total_mb: card.vram_gb.map_or(0, |gb| {
            (gb * 1024.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
        }),
        recommended_backend: backend_for(card, profile.vulkan_loader, platform),
    }
}

/// The backend recommended for `card`.
///
/// NVIDIA takes CUDA from a driver of the CUDA 12 family on, Vulkan below.
/// AMD under Linux takes ROCm when the kernel names its gfx target and
/// `/dev/kfd` opens; under Windows the build carries its HIP runtime and no
/// sourced table names the gfx target of a card, so ROCm is compatible, to be
/// confirmed, and recommended. Every other card, and a card ROCm or CUDA do
/// not reach, takes Vulkan when its loader is installed, the processor
/// otherwise.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn backend_for(card: &GpuCard, vulkan_loader: bool, platform: Platform) -> RunnerBackend {
    let vulkan_or_cpu = if vulkan_loader {
        RunnerBackend::Vulkan
    } else {
        RunnerBackend::Cpu
    };
    match card.vendor {
        GpuVendor::Nvidia => {
            if card
                .driver_major
                .is_some_and(|major| major >= CUDA_12_MIN_DRIVER)
            {
                RunnerBackend::Cuda
            } else {
                tracing::warn!(
                    driver = card.driver_version.as_deref().unwrap_or("unknown"),
                    detail = "falling back to Vulkan if available",
                    "gpu.cuda.driver.outdated",
                );
                vulkan_or_cpu
            }
        }
        GpuVendor::Amd => match platform {
            Platform::Windows => RunnerBackend::Rocm,
            Platform::Linux if card.gfx_target.is_some() && card.kfd_accessible == Some(true) => {
                RunnerBackend::Rocm
            }
            Platform::Linux => vulkan_or_cpu,
        },
        GpuVendor::Intel | GpuVendor::Other => vulkan_or_cpu,
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn log_detected(info: &GpuInfo) {
    tracing::info!(
        vendor = ?info.vendor,
        model = %info.model,
        memory_mb = info.memory_total_mb,
        backend = ?info.recommended_backend,
        "gpu.detected",
    );
}

// Backend choice

/// Returns a backend whose binary is actually in this build, starting from the
/// one that was chosen.
///
/// The runner carries the speech-to-text engine and only that: the language
/// model runs in llama-server, a separate binary chosen on its own. So the
/// name asked for here follows the graphics card, while packaging follows what
/// whisper can use: measured on the release of 2026-09-08, four desktop
/// bundles out of five ship the processor runner alone, macOS being the
/// exception. A machine with a Radeon therefore asked for
/// `apollia-runner-vulkan`, found nothing, and lost dictation AND the local
/// model at once, on a build that carried both.
///
/// Order: what was chosen, then the Vulkan runner when the choice was another
/// card backend, then the processor runner, then whatever is there. Vulkan
/// reaches the same NVIDIA and AMD cards CUDA and ROCm do, and the Windows
/// bundles ship it next to the processor runner since 2026-09-10 while no
/// bundle carries a CUDA or ROCm one: a card whose first choice is absent
/// keeps its dictation on the card rather than on the processor. The last
/// step matters for a bundle that ships a GPU runner alone, where the chosen
/// one can be the processor.
/// The backend to start with at boot, when no operator override is in reach.
///
/// Boot used to hand `detected.recommended_backend` straight to the supervisor.
/// Detection recommends Vulkan for every AMD and Intel card, no bundle has ever
/// shipped `apollia-runner-vulkan`, and the spawn path does not fall back on its
/// own, so the runner failed to start and took dictation and the local model
/// down with it. Measured on 2026-09-09, on Windows with a Radeon RX 6900 XT.
///
/// This is the one seam boot goes through: the operator override that used to
/// sit beside it, `[llm.runner] backend`, had test callers only and is
/// withdrawn.
pub(crate) fn boot_backend(detected: &GpuInfo) -> RunnerBackend {
    degrade_to_something_present(detected.recommended_backend)
}

fn degrade_to_something_present(chosen: RunnerBackend) -> RunnerBackend {
    if is_backend_available(chosen) {
        return chosen;
    }
    if matches!(chosen, RunnerBackend::Cuda | RunnerBackend::Rocm)
        && is_backend_available(RunnerBackend::Vulkan)
    {
        tracing::warn!(
            requested = ?chosen,
            using = ?RunnerBackend::Vulkan,
            detail = "dictation runs on the card through Vulkan; the language model keeps its own engine",
            "runner.backend.degraded",
        );
        return RunnerBackend::Vulkan;
    }
    if chosen != RunnerBackend::Cpu && is_backend_available(RunnerBackend::Cpu) {
        tracing::warn!(
            requested = ?chosen,
            using = ?RunnerBackend::Cpu,
            detail = "dictation runs on the processor; the language model keeps its own engine",
            "runner.backend.degraded",
        );
        return RunnerBackend::Cpu;
    }
    let others = [
        RunnerBackend::Metal,
        RunnerBackend::Cuda,
        RunnerBackend::Vulkan,
        RunnerBackend::Rocm,
    ];
    if let Some(found) = others
        .into_iter()
        .find(|b| *b != chosen && is_backend_available(*b))
    {
        tracing::warn!(
            requested = ?chosen,
            using = ?found,
            detail = "the only runner this build ships",
            "runner.backend.degraded",
        );
        return found;
    }
    tracing::warn!(
        requested = ?chosen,
        detail = "no runner binary ships with this build",
        "runner.backend.none_available",
    );
    chosen
}

/// Guards the directory the runner-lookup tests write into.
///
/// Those cases live in two modules and create the same file names next to the
/// test binary, so they take one lock rather than racing each other.
#[cfg(test)]
pub(super) fn runner_dir_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Check that a runner binary is present next to the current executable.
///
/// What a bundle ships is decided by packaging, not by this file: as of the
/// release of 2026-09-08 the macOS desktop carries `cpu` and `metal` while
/// every other desktop bundle carries `cpu` alone, and a CLI archive carries
/// the single runner of its preset. This function answers for one name; use
/// `degrade_to_something_present` to end up on one that exists.
///
/// A test or dev environment where the current executable cannot be resolved
/// answers `true`, so `boot_backend` keeps working there.
fn is_backend_available(backend: RunnerBackend) -> bool {
    // Delegate to the spawn-path resolver so detection and spawning agree on
    // every bundle layout (macOS .app `Contents/Resources/runners/`, the CLI
    // self-contained bundle, and the Linux packages). If the current executable
    // cannot be resolved (some test harnesses), assume available so
    // `boot_backend` keeps working.
    if std::env::current_exe().is_err() {
        return true;
    }
    super::lifecycle::locate_runner_binary(backend).is_ok()
}

// ─────────────────────────────────────────────
// Sondes macOS
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn query_system_profiler_chip_name() -> Option<String> {
    let out = std::process::Command::new("system_profiler")
        .args(["SPHardwareDataType", "-json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    json["SPHardwareDataType"][0]["chip_type"]
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(target_os = "macos")]
fn query_system_profiler_unified_memory_mb() -> Option<u32> {
    let out = std::process::Command::new("system_profiler")
        .args(["SPHardwareDataType", "-json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let s = json["SPHardwareDataType"][0]["physical_memory"].as_str()?;
    // Format "16 GB" -> 16 * 1024.
    let n: u32 = s.split_whitespace().next()?.parse().ok()?;
    Some(n * 1024)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn nvidia_detected() -> GpuInfo {
        GpuInfo {
            vendor: Vendor::Nvidia,
            model: "GeForce RTX 4070".to_string(),
            memory_total_mb: 12_288,
            recommended_backend: RunnerBackend::Cuda,
        }
    }

    #[test]
    fn cpu_fallback_when_no_gpu() {
        // GIVEN no probe succeeds
        // WHEN building the CPU fallback
        let info = GpuInfo::cpu_fallback();
        // THEN vendor=Unknown and backend=Cpu
        assert_eq!(info.vendor, Vendor::Unknown);
        assert_eq!(info.recommended_backend, RunnerBackend::Cpu);
        assert_eq!(info.memory_total_mb, 0);
    }

    fn write_runner(name: &str) -> std::path::PathBuf {
        let ext = if cfg!(windows) { ".exe" } else { "" };
        let path = std::env::current_exe()
            .expect("current_exe")
            .parent()
            .expect("parent")
            .join(format!("{name}{ext}"));
        std::fs::write(&path, b"probe").expect("write a runner");
        path
    }

    fn remove_every_runner() {
        let ext = if cfg!(windows) { ".exe" } else { "" };
        let dir = std::env::current_exe()
            .expect("current_exe")
            .parent()
            .expect("parent")
            .to_path_buf();
        for name in [
            "apollia-runner-cpu",
            "apollia-runner-cuda",
            "apollia-runner-rocm",
            "apollia-runner-vulkan",
            "apollia-runner-metal",
        ] {
            let _ = std::fs::remove_file(dir.join(format!("{name}{ext}")));
        }
    }

    #[test]
    fn a_detected_gpu_with_no_runner_degrades_to_the_processor_one() {
        // GIVEN a machine whose card resolves to Vulkan, on a build that ships
        // the processor runner alone: four desktop bundles out of five
        let _guard = runner_dir_lock();
        remove_every_runner();
        let cpu = write_runner("apollia-runner-cpu");

        // WHEN boot picks the backend to start
        let detected = GpuInfo {
            recommended_backend: RunnerBackend::Vulkan,
            ..GpuInfo::cpu_fallback()
        };
        let backend = boot_backend(&detected);

        // THEN it lands on the runner that exists rather than on one that does
        // not: asking for the absent name cost dictation and the local model
        // at once on a machine carrying both
        assert_eq!(backend, RunnerBackend::Cpu);
        let _ = std::fs::remove_file(&cpu);
    }

    #[test]
    fn boot_degrades_a_vulkan_recommendation_onto_the_runner_that_ships() {
        // GIVEN what every published Windows and Linux bundle carries, the
        // processor runner alone, on a machine whose card makes detection
        // recommend Vulkan: an AMD or an Intel GPU
        let _guard = runner_dir_lock();
        remove_every_runner();
        let cpu = write_runner("apollia-runner-cpu");
        let detected = GpuInfo {
            recommended_backend: RunnerBackend::Vulkan,
            ..GpuInfo::cpu_fallback()
        };

        // WHEN boot picks the backend to start
        let backend = boot_backend(&detected);

        // THEN it starts the runner that exists. Boot used to pass the
        // recommendation straight through, so the supervisor looked for
        // apollia-runner-vulkan, a binary no bundle has ever carried, failed,
        // and disabled dictation and the local model together.
        assert_eq!(backend, RunnerBackend::Cpu);
        let _ = std::fs::remove_file(&cpu);
    }

    #[test]
    fn an_nvidia_card_without_a_cuda_runner_lands_on_the_vulkan_one_that_ships() {
        // GIVEN what the Windows bundles ship since 2026-09-10, the processor
        // and the Vulkan runners, on a machine whose card makes detection
        // recommend CUDA
        let _guard = runner_dir_lock();
        remove_every_runner();
        let cpu = write_runner("apollia-runner-cpu");
        let vulkan = write_runner("apollia-runner-vulkan");
        let detected = nvidia_detected();

        // WHEN boot picks the backend to start
        let backend = boot_backend(&detected);

        // THEN dictation stays on the card: the Vulkan runner reaches it,
        // where the processor one would have been chosen before this test
        // existed
        assert_eq!(backend, RunnerBackend::Vulkan);

        // AND the control: without the Vulkan runner, the processor one is
        // still the answer, so the change is about preference, not presence
        let _ = std::fs::remove_file(&vulkan);
        assert_eq!(boot_backend(&detected), RunnerBackend::Cpu);
        let _ = std::fs::remove_file(&cpu);
    }

    #[test]
    fn a_build_shipping_one_gpu_runner_alone_is_used_by_a_machine_without_a_card() {
        // GIVEN a bundle carrying the Vulkan runner alone, which is what a
        // vulkan CLI preset ships, on a machine detected as processor-only
        let _guard = runner_dir_lock();
        remove_every_runner();
        let vulkan = write_runner("apollia-runner-vulkan");

        // WHEN boot picks the backend to start
        let detected = GpuInfo::cpu_fallback();
        let backend = boot_backend(&detected);

        // THEN the runner that ships is used, because refusing here disables
        // dictation on a build that carries a working one
        assert_eq!(backend, RunnerBackend::Vulkan);
        let _ = std::fs::remove_file(&vulkan);
    }

    #[test]
    fn a_build_with_no_runner_at_all_keeps_the_detected_backend() {
        // GIVEN a build shipping no runner, a dev tree for instance
        let _guard = runner_dir_lock();
        remove_every_runner();

        // WHEN boot picks the backend to start
        let detected = GpuInfo {
            recommended_backend: RunnerBackend::Cuda,
            ..GpuInfo::cpu_fallback()
        };
        let backend = boot_backend(&detected);

        // THEN the detected backend is kept and the spawn reports the missing
        // binary by name, which is the diagnosis a reader needs
        assert_eq!(backend, RunnerBackend::Cuda);
    }

    #[test]
    fn runner_backend_binary_names() {
        // GIVEN each backend of the enum
        // WHEN its binary name is asked for
        // THEN it is the runner binary the supervisor spawns for that backend
        assert_eq!(RunnerBackend::Cuda.binary_name(), "apollia-runner-cuda");
        assert_eq!(RunnerBackend::Rocm.binary_name(), "apollia-runner-rocm");
        assert_eq!(RunnerBackend::Vulkan.binary_name(), "apollia-runner-vulkan");
        assert_eq!(RunnerBackend::Metal.binary_name(), "apollia-runner-metal");
        assert_eq!(RunnerBackend::Cpu.binary_name(), "apollia-runner-cpu");
    }
}

#[cfg(test)]
mod profile_tests;
