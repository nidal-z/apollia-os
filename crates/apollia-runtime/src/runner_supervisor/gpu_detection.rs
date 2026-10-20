//! Cross-platform GPU detection at daemon boot.
//!
//! At startup, the `RunnerSupervisor` queries [`detect_gpu`] to get the local
//! GPU's characteristics, then applies [`boot_backend`] to produce the final
//! `RunnerBackend`, degraded onto a runner this build ships. No setting
//! overrides the choice.
//!
//! The decision hierarchy is: Apple Silicon, NVIDIA + CUDA >= 12.0, AMD + ROCm,
//! Vulkan fallback, CPU fallback.
//!
//! The module is strictly local-first: no telemetry, no network calls. All
//! probes are subprocesses (`nvidia-smi`, `rocm-smi`, `system_profiler`,
//! `vulkaninfo`) or `/sys` reads.

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
    /// NVIDIA CUDA (driver >= 12.0).
    Cuda,
    /// AMD ROCm (Linux + Windows Radeon Pro / Instinct).
    Rocm,
    /// Vulkan compute (cross-vendor fallback).
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

/// Detect the GPU on Linux: NVIDIA via `nvidia-smi`, AMD via `rocm-smi`,
/// then scan `/sys/class/drm` for other vendors.
#[cfg(target_os = "linux")]
pub fn detect_gpu() -> GpuInfo {
    if let Some(info) = probe_nvidia_linux() {
        log_detected(&info);
        return info;
    }
    if let Some(info) = probe_amd_rocm_linux() {
        log_detected(&info);
        return info;
    }
    if let Some(info) = probe_pci_drm() {
        log_detected(&info);
        return info;
    }
    let info = GpuInfo::cpu_fallback();
    log_detected(&info);
    info
}

/// Detect the GPU on Windows: WMI `Win32_VideoController` + probes
/// `nvidia-smi.exe`, HIP SDK, `vulkan-1.dll`.
#[cfg(target_os = "windows")]
pub fn detect_gpu() -> GpuInfo {
    let mut controllers = wmi_list_video_controllers().unwrap_or_default();
    if let Some(adapters) = registry_list_adapters() {
        for ctrl in &mut controllers {
            if let Some(bytes) = registry_memory_bytes(&ctrl.name, &ctrl.pnp_device_id, &adapters) {
                ctrl.adapter_ram = bytes;
            }
        }
    }

    if let Some(ctrl) = controllers.iter().find(|c| c.name.contains("NVIDIA")) {
        let info = build_video_controller_info(ctrl, Vendor::Nvidia, nvidia_backend_windows());
        log_detected(&info);
        return info;
    }

    if let Some(ctrl) = controllers
        .iter()
        .find(|c| c.name.contains("AMD") || c.name.contains("Radeon"))
    {
        let info = build_video_controller_info(ctrl, Vendor::Amd, amd_backend_windows(&ctrl.name));
        log_detected(&info);
        return info;
    }

    if let Some(ctrl) = controllers
        .iter()
        .find(|c| c.name.contains("Intel") && c.name.contains("Arc"))
    {
        let info = build_video_controller_info(ctrl, Vendor::Intel, intel_backend_windows());
        log_detected(&info);
        return info;
    }

    let info = GpuInfo::cpu_fallback();
    log_detected(&info);
    info
}

/// Select the recommended backend for an NVIDIA GPU on Windows.
#[cfg(target_os = "windows")]
fn nvidia_backend_windows() -> RunnerBackend {
    let cuda_version = probe_cuda_version_windows().unwrap_or((0, 0));
    if cuda_version.0 >= 12 {
        RunnerBackend::Cuda
    } else if vulkan_loader_present_windows() {
        RunnerBackend::Vulkan
    } else {
        RunnerBackend::Cpu
    }
}

/// Select the recommended backend for an AMD GPU on Windows.
#[cfg(target_os = "windows")]
fn amd_backend_windows(name: &str) -> RunnerBackend {
    if hip_sdk_present_windows() && rocm_supports_card(name) {
        RunnerBackend::Rocm
    } else if vulkan_loader_present_windows() {
        RunnerBackend::Vulkan
    } else {
        RunnerBackend::Cpu
    }
}

/// Select the recommended backend for an Intel Arc GPU on Windows.
#[cfg(target_os = "windows")]
fn intel_backend_windows() -> RunnerBackend {
    if vulkan_loader_present_windows() {
        RunnerBackend::Vulkan
    } else {
        RunnerBackend::Cpu
    }
}

/// Build a [`GpuInfo`] from a WMI controller and the chosen backend.
#[cfg(target_os = "windows")]
fn build_video_controller_info(
    ctrl: &VideoController,
    vendor: Vendor,
    backend: RunnerBackend,
) -> GpuInfo {
    GpuInfo {
        vendor,
        model: ctrl.name.clone(),
        memory_total_mb: ctrl.adapter_ram_mb(),
        recommended_backend: backend,
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

// ─────────────────────────────────────────────
// Sondes Linux
// ─────────────────────────────────────────────

/// Parsed representation of a `nvidia-smi --query-gpu=name,memory.total,driver_version` line.
// Only the Linux probe parses `nvidia-smi` output; Windows detects GPUs through
// WMI. Keeping `windows` in this gate declared both items with no consumer
// there, which is a warning on that target.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NvidiaSmiLine {
    model: String,
    memory_total_mb: u32,
    driver_version: String,
}

/// Parses one `--format=csv,noheader,nounits` line in the format
/// `"GeForce RTX 4070, 12288, 550.78"`.
// Only the Linux probe parses `nvidia-smi` output; Windows detects GPUs through
// WMI. Keeping `windows` in this gate declared both items with no consumer
// there, which is a warning on that target.
#[cfg(any(target_os = "linux", test))]
fn parse_nvidia_smi_line(line: &str) -> Option<NvidiaSmiLine> {
    let mut parts = line.split(',').map(str::trim);
    let model = parts.next()?.to_string();
    let memory_total_mb: u32 = parts.next()?.parse().ok()?;
    let driver_version = parts.next()?.to_string();
    if model.is_empty() {
        return None;
    }
    Some(NvidiaSmiLine {
        model,
        memory_total_mb,
        driver_version,
    })
}

#[cfg(target_os = "linux")]
fn probe_nvidia_linux() -> Option<GpuInfo> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    let first_line = stdout.lines().next()?;
    let parsed = parse_nvidia_smi_line(first_line)?;

    let cuda_version = probe_cuda_version_linux().unwrap_or((0, 0));
    let backend = if cuda_version.0 >= 12 {
        RunnerBackend::Cuda
    } else {
        tracing::warn!(
            driver = %parsed.driver_version,
            cuda_major = cuda_version.0,
            cuda_minor = cuda_version.1,
            detail = "falling back to Vulkan if available", "gpu.cuda.driver.outdated",
        );
        if vulkan_loader_present_linux() {
            RunnerBackend::Vulkan
        } else {
            RunnerBackend::Cpu
        }
    };

    Some(GpuInfo {
        vendor: Vendor::Nvidia,
        model: parsed.model,
        memory_total_mb: parsed.memory_total_mb,
        recommended_backend: backend,
    })
}

#[cfg(target_os = "linux")]
fn probe_cuda_version_linux() -> Option<(u32, u32)> {
    let out = std::process::Command::new("nvidia-smi")
        .arg("-q")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    parse_cuda_version_from_smi_q(&s)
}

// Shared: both the Linux probe and the Windows one read the CUDA version out of
// `nvidia-smi -q`.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn parse_cuda_version_from_smi_q(s: &str) -> Option<(u32, u32)> {
    let line = s.lines().find(|l| l.contains("CUDA Version"))?;
    let v = line.split(':').nth(1)?.trim();
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(target_os = "linux")]
fn probe_amd_rocm_linux() -> Option<GpuInfo> {
    let out = std::process::Command::new("rocm-smi")
        .args(["--showproductname", "--showmeminfo", "vram", "--json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let card = json.as_object()?.values().next()?;
    let model = card["Card series"].as_str()?.to_string();
    let mem_bytes: u64 = card["VRAM Total Memory (B)"].as_str()?.parse().ok()?;
    Some(GpuInfo {
        vendor: Vendor::Amd,
        model,
        memory_total_mb: (mem_bytes / 1024 / 1024) as u32,
        recommended_backend: RunnerBackend::Rocm,
    })
}

#[cfg(target_os = "linux")]
fn probe_pci_drm() -> Option<GpuInfo> {
    use std::fs;

    let entries = fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let vendor_path = path.join("device/vendor");
        let Ok(vendor_id) = fs::read_to_string(&vendor_path) else {
            continue;
        };
        let vendor_id = vendor_id.trim();
        let (vendor, fallback_backend) = match vendor_id {
            // If we got here, no nvidia-smi: try Vulkan.
            "0x10de" => (Vendor::Nvidia, RunnerBackend::Vulkan),
            // No ROCm, Vulkan fallback.
            "0x1002" => (Vendor::Amd, RunnerBackend::Vulkan),
            "0x8086" => (Vendor::Intel, RunnerBackend::Vulkan),
            _ => continue,
        };
        let model = read_pci_device_name(&path).unwrap_or_else(|| format!("{:?} GPU", vendor));
        let backend = if vulkan_loader_present_linux() {
            fallback_backend
        } else {
            RunnerBackend::Cpu
        };
        return Some(GpuInfo {
            vendor,
            model,
            memory_total_mb: 0,
            recommended_backend: backend,
        });
    }
    None
}

#[cfg(target_os = "linux")]
fn read_pci_device_name(card_path: &std::path::Path) -> Option<String> {
    use std::fs;
    let uevent = fs::read_to_string(card_path.join("device/uevent")).ok()?;
    for line in uevent.lines() {
        if let Some(rest) = line.strip_prefix("DRIVER=") {
            return Some(rest.to_string());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn vulkan_loader_present_linux() -> bool {
    std::process::Command::new("vulkaninfo")
        .arg("--summary")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

// ─────────────────────────────────────────────
// Sondes Windows
// ─────────────────────────────────────────────

/// GPU controller as exposed by WMI `Win32_VideoController`.
///
/// `adapter_ram` starts as WMI's `AdapterRAM`, a 32-bit field that saturates
/// at 4095 MiB: a 16 GB Radeon RX 6900 XT was logged `memory_mb=4095` and the
/// model was sized, and partly computed on the processor, for a 4 GB card.
/// [`detect_gpu`] replaces it with the 64-bit size of the registry when the
/// adapter is found there.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, serde::Deserialize)]
struct VideoController {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AdapterRAM", default)]
    adapter_ram: u64,
    #[serde(rename = "PNPDeviceID", default)]
    pnp_device_id: String,
}

#[cfg(target_os = "windows")]
impl VideoController {
    fn adapter_ram_mb(&self) -> u32 {
        u32::try_from(self.adapter_ram / 1024 / 1024).unwrap_or(u32::MAX)
    }
}

#[cfg(target_os = "windows")]
fn wmi_list_video_controllers() -> Option<Vec<VideoController>> {
    use wmi::{COMLibrary, WMIConnection};
    let com_con = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com_con).ok()?;
    let results: Vec<VideoController> = wmi_con
        .raw_query("SELECT Name, AdapterRAM, PNPDeviceID FROM Win32_VideoController")
        .ok()?;
    Some(results)
}

/// A display adapter as its driver recorded it under the display class key
/// of the registry.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryAdapter {
    /// `DriverDesc`, the name WMI reports as `Name`.
    driver_desc: String,
    /// `MatchingDeviceId`, lowercase, a prefix of WMI's `PNPDeviceID`.
    matching_device_id: String,
    /// `HardwareInformation.qwMemorySize`, in bytes.
    memory_bytes: u64,
}

/// Parses the `DriverDesc<tab>MatchingDeviceId<tab>bytes` lines of
/// [`registry_list_adapters`]. A line without a positive size is skipped.
#[cfg(any(target_os = "windows", test))]
fn parse_registry_adapters(stdout: &str) -> Vec<RegistryAdapter> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let driver_desc = fields.next()?.trim().to_string();
            let matching_device_id = fields.next()?.trim().to_ascii_lowercase();
            let memory_bytes: u64 = fields.next()?.trim().parse().ok()?;
            (memory_bytes > 0).then_some(RegistryAdapter {
                driver_desc,
                matching_device_id,
                memory_bytes,
            })
        })
        .collect()
}

/// The registry size of the controller named `name` with the plug-and-play
/// id `pnp_device_id`.
///
/// The device id decides first, the longest matching id winning, since two
/// cards may share a name and a driver may write a name of its own. The name
/// is the fallback, for a controller WMI listed without an id.
#[cfg(any(target_os = "windows", test))]
fn registry_memory_bytes(
    name: &str,
    pnp_device_id: &str,
    adapters: &[RegistryAdapter],
) -> Option<u64> {
    let pnp = pnp_device_id.trim().to_ascii_lowercase();
    let by_id = adapters
        .iter()
        .filter(|a| !a.matching_device_id.is_empty() && pnp.starts_with(&a.matching_device_id))
        .max_by_key(|a| a.matching_device_id.len());
    let name = name.trim();
    by_id
        .or_else(|| {
            adapters
                .iter()
                .find(|a| !name.is_empty() && a.driver_desc.eq_ignore_ascii_case(name))
        })
        .map(|a| a.memory_bytes)
}

/// Every display adapter of the registry with its 64-bit memory size, read
/// through PowerShell as the language-model crate already reads it, so no
/// registry crate is added.
#[cfg(target_os = "windows")]
fn registry_list_adapters() -> Option<Vec<RegistryAdapter>> {
    // A driver may store the size as an 8-byte binary rather than a QWORD;
    // both are turned into a decimal count of bytes.
    const SCRIPT: &str = concat!(
        r"$k = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\",
        r"{4d36e968-e325-11ce-bfc1-08002be10318}\*'; ",
        "Get-ItemProperty $k -ErrorAction SilentlyContinue | ForEach-Object { ",
        "$m = $_.'HardwareInformation.qwMemorySize'; ",
        "if ($m -is [byte[]]) { $m = [BitConverter]::ToUInt64($m, 0) }; ",
        "if ($m) { $_.DriverDesc + [char]9 + $_.MatchingDeviceId + [char]9 + $m } }",
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_registry_adapters(&String::from_utf8_lossy(
        &out.stdout,
    )))
}

#[cfg(target_os = "windows")]
fn probe_cuda_version_windows() -> Option<(u32, u32)> {
    // Runs on every boot, so without this the desktop app flashes a console
    // window before its own window is even up.
    let mut smi = std::process::Command::new("nvidia-smi.exe");
    apollia_core::subprocess_window::hide_console(&mut smi);
    let out = smi.arg("-q").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    parse_cuda_version_from_smi_q(&s)
}

#[cfg(target_os = "windows")]
fn vulkan_loader_present_windows() -> bool {
    // `vulkan-1.dll` is shipped by the GPU driver on Windows.
    let mut where_cmd = std::process::Command::new("where");
    apollia_core::subprocess_window::hide_console(&mut where_cmd);
    where_cmd
        .arg("vulkan-1.dll")
        .output()
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn hip_sdk_present_windows() -> bool {
    std::env::var("HIP_PATH").is_ok() || std::path::Path::new("C:/Program Files/AMD/ROCm").exists()
}

/// AMD ROCm Windows allowlist: officially limited to Radeon Pro and Instinct MI.
#[cfg(any(target_os = "windows", test))]
fn rocm_supports_card(name: &str) -> bool {
    name.contains("Radeon Pro") || name.contains("Instinct")
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
    fn parse_nvidia_smi_line_rtx_4070() {
        // GIVEN a typical output of `nvidia-smi --query-gpu=...`
        let line = "GeForce RTX 4070, 12288, 550.78";
        // WHEN parsing
        let parsed = parse_nvidia_smi_line(line).expect("parsing should succeed");
        // THEN the 3 fields are extracted
        assert_eq!(parsed.model, "GeForce RTX 4070");
        assert_eq!(parsed.memory_total_mb, 12_288);
        assert_eq!(parsed.driver_version, "550.78");
    }

    #[test]
    fn parse_nvidia_smi_line_rejects_garbage() {
        // GIVEN a malformed output (missing memory and version)
        let line = "only-model";
        // WHEN the line is parsed
        // THEN None is returned
        assert!(parse_nvidia_smi_line(line).is_none());
    }

    #[test]
    fn parse_nvidia_smi_line_rejects_non_numeric_memory() {
        // GIVEN an output with non-numeric memory
        let line = "Some GPU, notanumber, 1.0";
        // WHEN the line is parsed
        // THEN None is returned (parse::<u32> fails)
        assert!(parse_nvidia_smi_line(line).is_none());
    }

    #[test]
    fn parse_cuda_version_from_smi_q_typical() {
        // GIVEN an excerpt of `nvidia-smi -q`
        let s = "Driver Version : 550.78\nCUDA Version : 12.4\nGPU 00000000:01:00.0";
        // WHEN
        let v = parse_cuda_version_from_smi_q(s);
        // THEN
        assert_eq!(v, Some((12, 4)));
    }

    #[test]
    fn parse_cuda_version_from_smi_q_missing() {
        // GIVEN a text without the CUDA Version line
        let s = "Driver Version : 550.78\nGPU info...";
        // WHEN the CUDA version is looked for
        // THEN None
        assert!(parse_cuda_version_from_smi_q(s).is_none());
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

    #[test]
    fn rocm_only_for_radeon_pro_or_instinct() {
        // Static ROCm allowlist: Radeon Pro and Instinct only.
        // GIVEN two professional AMD cards, one consumer AMD card and one NVIDIA card
        // WHEN the ROCm allowlist is asked about each
        // THEN only the professional cards are supported
        assert!(rocm_supports_card("AMD Radeon Pro W7900"));
        assert!(rocm_supports_card("AMD Instinct MI300"));
        assert!(!rocm_supports_card("AMD Radeon RX 7800 XT"));
        assert!(!rocm_supports_card("NVIDIA GeForce RTX 4070"));
    }

    /// Registry lines as the Windows probe prints them: a discrete Radeon of
    /// 16 GB, an integrated Radeon with no size of its own, and the Microsoft
    /// basic display adapter.
    const REGISTRY_LINES: &str = "AMD Radeon RX 6900 XT\tpci\\ven_1002&dev_73bf\t17163091968\r\n\
        AMD Radeon(TM) Graphics\tPCI\\VEN_1002&DEV_164E\t0\r\n\
        Microsoft Basic Display Adapter\tpci\\cc_0300\t\r\n";

    #[test]
    fn test_parse_registry_adapters_keeps_the_adapters_with_a_size() {
        // GIVEN the probe's output for three adapters, one with a size
        // WHEN it is parsed
        let adapters = parse_registry_adapters(REGISTRY_LINES);

        // THEN only the sized adapter remains, its id lowercased
        assert_eq!(
            adapters,
            vec![RegistryAdapter {
                driver_desc: "AMD Radeon RX 6900 XT".to_string(),
                matching_device_id: "pci\\ven_1002&dev_73bf".to_string(),
                memory_bytes: 17_163_091_968,
            }]
        );
    }

    #[test]
    fn test_registry_memory_bytes_matches_the_controller_by_device_id() {
        // GIVEN the registry adapters and a controller whose WMI name differs
        // from the driver's description but whose device id matches
        let adapters = parse_registry_adapters(REGISTRY_LINES);
        let pnp = "PCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C0\\6&1A2B3C4D&0&00000019";

        // WHEN the controller's memory is looked up
        let bytes = registry_memory_bytes("Radeon RX 6900 XT (vendor name)", pnp, &adapters);

        // THEN the registry's 16 GB is returned, above WMI's 4095 MiB ceiling
        assert_eq!(bytes, Some(17_163_091_968));
    }

    #[test]
    fn test_registry_memory_bytes_falls_back_to_the_driver_description() {
        // GIVEN a controller WMI listed with no device id
        let adapters = parse_registry_adapters(REGISTRY_LINES);

        // WHEN its memory is looked up by name
        let bytes = registry_memory_bytes("amd radeon rx 6900 xt", "", &adapters);

        // THEN the adapter of the same description answers
        assert_eq!(bytes, Some(17_163_091_968));
    }

    #[test]
    fn test_registry_memory_bytes_of_an_unknown_controller_is_none() {
        // GIVEN a controller absent from the registry listing
        let adapters = parse_registry_adapters(REGISTRY_LINES);

        // WHEN its memory is looked up
        let bytes = registry_memory_bytes(
            "Intel(R) Arc(TM) A770",
            "PCI\\VEN_8086&DEV_56A0\\4&1",
            &adapters,
        );

        // THEN nothing is found, and WMI's AdapterRAM is kept
        assert_eq!(bytes, None);
    }

    #[test]
    fn test_registry_memory_bytes_prefers_the_longest_device_id() {
        // GIVEN two entries whose ids both prefix the controller's, the
        // longer naming its subsystem
        let adapters = parse_registry_adapters(
            "Generic\tpci\\ven_1002&dev_73bf\t4294967296\n\
             Board\tpci\\ven_1002&dev_73bf&subsys_0e3a1002\t17163091968\n",
        );

        // WHEN the controller's memory is looked up
        let bytes = registry_memory_bytes(
            "AMD Radeon RX 6900 XT",
            "PCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C0\\6&1",
            &adapters,
        );

        // THEN the more specific entry answers
        assert_eq!(bytes, Some(17_163_091_968));
    }
}
