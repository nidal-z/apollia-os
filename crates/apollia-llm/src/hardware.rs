//! Hardware detection for LLM model recommendations.
//!
//! Exposes [`HardwareProfile`] with RAM, CPU, and accelerator (Metal / CUDA / None).
//! Used by the Model Hub to display GGUF compatibility badges.

use serde::{Deserialize, Serialize};
use sysinfo::System;

/// Detected hardware profile of the local machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    /// Total RAM in GB.
    pub total_ram_gb: f64,
    /// Available RAM in GB.
    pub available_ram_gb: f64,
    /// CPU name/model (e.g. `"Apple M4 Max"`, `"Intel Core i9-13900K"`).
    pub cpu_model: String,
    /// Number of logical cores.
    pub cpu_cores: u32,
    /// Detected graphics accelerator.
    pub accelerator: AcceleratorProfile,
    /// Recommended memory budget for inference, in GB.
    ///
    /// - Apple Silicon: unified RAM x 0.75 (shared CPU/GPU)
    /// - CUDA: dedicated VRAM
    /// - CPU only: RAM x 0.60 (keep RAM for the OS and other processes)
    pub memory_budget_gb: f64,
}

/// Graphics accelerator available for inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceleratorProfile {
    /// No usable GPU, CPU-only inference.
    None,
    /// Apple Silicon with unified memory (Metal).
    AppleSilicon {
        /// Chip name (e.g. `"M4 Max"`, `"M3 Pro"`).
        chip: String,
        /// Generation (1 = M1, 2 = M2, 3 = M3, 4 = M4).
        generation: u8,
        /// Effective VRAM = total unified RAM (in GB).
        vram_gb: f64,
    },
    /// NVIDIA GPU (CUDA).
    Cuda {
        /// GPU name (e.g. `"NVIDIA GeForce RTX 4090"`).
        device_name: String,
        /// Dedicated VRAM in GB.
        vram_gb: f64,
        /// Compute capability as `(major, minor)`.
        compute_capability: (u8, u8),
    },
    /// Other GPU (AMD, Intel Arc, etc.) via generic acceleration.
    Generic {
        /// GPU name.
        device_name: String,
        /// VRAM in GB (if detectable).
        vram_gb: f64,
    },
}

/// Compatibility badge for a GGUF file against the local hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityBadge {
    /// The model fits comfortably in memory (<= 70% of the budget).
    Fits,
    /// The model might fit but will be tight (70 to 100% of the budget).
    MightFit,
    /// The model exceeds the available memory budget.
    TooLarge,
}

impl CompatibilityBadge {
    /// Compute the badge from the file size (in GB) and the hardware profile.
    pub fn compute(file_size_gb: f64, profile: &HardwareProfile) -> Self {
        let required = file_size_gb * 1.1; // ~10% llama.cpp overhead
        let budget = profile.memory_budget_gb;
        if required < budget * 0.70 {
            CompatibilityBadge::Fits
        } else if required < budget {
            CompatibilityBadge::MightFit
        } else {
            CompatibilityBadge::TooLarge
        }
    }
}

/// Detect the hardware profile of the local machine.
///
/// Blocking: call from `tokio::task::spawn_blocking` if needed.
pub fn detect() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let available_ram_gb = sys.available_memory() as f64 / 1_073_741_824.0;

    let cpu_model = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());
    let cpu_cores = sys.cpus().len() as u32;

    #[cfg(target_os = "macos")]
    let accelerator = detect_apple_silicon(total_ram_gb);

    #[cfg(not(target_os = "macos"))]
    let accelerator = detect_gpu_non_macos();

    let memory_budget_gb = compute_budget(&accelerator, total_ram_gb);

    HardwareProfile {
        total_ram_gb,
        available_ram_gb,
        cpu_model,
        cpu_cores,
        accelerator,
        memory_budget_gb,
    }
}

fn compute_budget(accel: &AcceleratorProfile, total_ram_gb: f64) -> f64 {
    match accel {
        // Apple Silicon: unified memory shared between CPU/GPU.
        // Reserve 25 % for OS + other processes.
        AcceleratorProfile::AppleSilicon { vram_gb, .. } => vram_gb * 0.75,
        // CUDA: use dedicated VRAM fully for inference.
        AcceleratorProfile::Cuda { vram_gb, .. } => *vram_gb,
        // A discrete card is sized by its own memory. An integrated one
        // reports no dedicated memory at all, and sizing a model on zero would
        // leave nothing loadable, so it falls back to the CPU-only rule: it
        // shares system RAM, which is exactly what that rule measures.
        AcceleratorProfile::Generic { vram_gb, .. } if *vram_gb > 0.0 => *vram_gb,
        AcceleratorProfile::Generic { .. } => total_ram_gb * 0.60,
        // CPU-only: 60 % of system RAM.
        AcceleratorProfile::None => total_ram_gb * 0.60,
    }
}

// ── macOS Apple Silicon detection ────────────────────────────────────────

#[cfg(target_os = "macos")]
fn detect_apple_silicon(total_ram_gb: f64) -> AcceleratorProfile {
    let output = std::process::Command::new("system_profiler")
        .args(["SPHardwareDataType", "-json"])
        .output();

    let Ok(output) = output else {
        return AcceleratorProfile::None;
    };

    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return AcceleratorProfile::None;
    };

    let chip_type = json["SPHardwareDataType"][0]["chip_type"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if chip_type.to_lowercase().contains('m') || chip_type.to_lowercase().starts_with("apple") {
        // Strip "Apple " prefix if present.
        let chip = chip_type
            .strip_prefix("Apple ")
            .unwrap_or(&chip_type)
            .to_string();

        let generation = parse_apple_silicon_generation(&chip);

        return AcceleratorProfile::AppleSilicon {
            chip,
            generation,
            // Unified memory = total RAM.
            vram_gb: total_ram_gb,
        };
    }

    AcceleratorProfile::None
}

#[cfg(target_os = "macos")]
fn parse_apple_silicon_generation(chip: &str) -> u8 {
    let lower = chip.to_lowercase();
    if lower.contains("m4") {
        4
    } else if lower.contains("m3") {
        3
    } else if lower.contains("m2") {
        2
    } else if lower.contains("m1") {
        1
    } else {
        0
    }
}

// ── Linux / Windows NVIDIA CUDA detection ────────────────────────────────

#[cfg(not(target_os = "macos"))]
/// The devices the bundled engine actually offers, as it prints them.
///
/// Every other probe on this path asks the operating system and infers: a
/// registry value under Windows, a sysfs node under Linux. Both answered wrong
/// on the same machine on 2026-09-09. The registry query returned nothing for a
/// Radeon RX 6900 XT, so the settings page read "no acceleration" and the model
/// budget fell back to sixty percent of system RAM, offering 38 GB models to a
/// 16 GB card. Meanwhile `llama-server`, sitting in the same bundle, printed
/// `Vulkan0: AMD Radeon RX 6900 XT (16368 MiB, 7313 MiB free)`.
///
/// It is also the only source that answers the question actually being asked.
/// A card the operating system reports is not a card the engine can use: what
/// decides the budget is what the engine will load onto, and that is precisely
/// what this list is.
///
/// Spawned once per process. Several API routes call `detect` on every page
/// load, and paying a subprocess each time would be worse than the guess it
/// replaces.
#[cfg(not(target_os = "macos"))]
static ENGINE_DEVICES: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

#[cfg(not(target_os = "macos"))]
fn engine_device_line() -> Option<String> {
    ENGINE_DEVICES.get_or_init(probe_engine_devices).clone()
}

#[cfg(not(target_os = "macos"))]
fn probe_engine_devices() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let file = format!("llama-server{ext}");
    let mut candidates = vec![dir.join(&file)];
    candidates.extend(
        apollia_core::paths::bundled_resource_dirs(dir, "runners")
            .into_iter()
            .map(|d| d.join(&file)),
    );
    let binary = candidates.into_iter().find(|c| c.exists())?;

    let mut cmd = std::process::Command::new(binary);
    cmd.arg("--list-devices");
    apollia_core::subprocess_window::hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| parse_engine_device_line(l).map(|_| l.to_string()))
}

/// One line of `llama-server --list-devices`, as name and VRAM in GB.
///
/// The shape is `  <backend><index>: <name> (<total> MiB, <free> MiB free)`.
/// The heading line carries no parenthesis and is refused by the same rule that
/// reads the others, so nothing special-cases it.
#[cfg(not(target_os = "macos"))]
pub(super) fn parse_engine_device_line(line: &str) -> Option<(String, f64)> {
    let (_, rest) = line.trim().split_once(':')?;
    let rest = rest.trim();
    let open = rest.rfind('(')?;
    let name = rest[..open].trim();
    if name.is_empty() {
        return None;
    }
    let mib: String = rest[open + 1..]
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let mib: f64 = mib.parse().ok()?;
    if mib <= 0.0 {
        return None;
    }
    Some((name.to_string(), mib / 1024.0))
}

#[cfg(not(target_os = "macos"))]
fn detect_engine_gpu() -> Option<AcceleratorProfile> {
    let line = engine_device_line()?;
    let (device_name, vram_gb) = parse_engine_device_line(&line)?;
    Some(AcceleratorProfile::Generic {
        device_name,
        vram_gb,
    })
}

#[cfg(not(target_os = "macos"))]
fn detect_gpu_non_macos() -> AcceleratorProfile {
    // NVIDIA first, because `nvidia-smi` gives the compute capability nothing
    // else does. Every other card then gets a `Generic` profile rather than
    // the `None` it used to receive by omission: an AMD or Intel machine was
    // told it had no accelerator at all, so the memory budget was computed on
    // system RAM and the model recommendation was sized for it. Measured on
    // 2026-09-08: a 16 GB Radeon under Windows was offered models built for
    // 38 GB, and the settings page read "hardware acceleration: none" while
    // the runtime had detected the very same card and started its Vulkan
    // engine.
    // NVIDIA stays first: its profile carries the compute capability, which no
    // device listing exposes. Everyone else is asked of the engine before the
    // operating system, and the OS probes remain as the last resort for a
    // bundle whose engine is absent or refuses to run.
    detect_nvidia_cuda()
        .or_else(detect_engine_gpu)
        .or_else(detect_generic_gpu)
        .unwrap_or(AcceleratorProfile::None)
}

/// Bytes reported for a GPU by the Windows registry, parsed to gibibytes.
///
/// `Win32_VideoController.AdapterRAM`, the obvious WMI source, is a 32-bit
/// field Microsoft never widened: every card of 4 GB or more reports 4095 MiB.
/// The registry keeps the real size in `HardwareInformation.qwMemorySize`, a
/// 64-bit value. Measured on 2026-09-08 on a 16 GB Radeon that WMI called 4 GB.
#[cfg(not(target_os = "macos"))]
pub(super) fn vram_gb_from_bytes(raw: &str) -> Option<f64> {
    let bytes: f64 = raw.trim().parse().ok()?;
    if bytes <= 0.0 {
        return None;
    }
    Some(bytes / 1024.0 / 1024.0 / 1024.0)
}

/// Parse the `name<tab>bytes` line the platform probes emit.
#[cfg(not(target_os = "macos"))]
pub(super) fn parse_generic_gpu(line: &str) -> Option<(String, f64)> {
    let (name, bytes) = line.split_once('\t')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), vram_gb_from_bytes(bytes).unwrap_or(0.0)))
}

/// A non-NVIDIA GPU and its memory, asked of the platform.
///
/// Windows answers from the registry, Linux from sysfs; both are read through
/// a short probe rather than a new dependency, which is how this module
/// already reaches `nvidia-smi` and `system_profiler`.
#[cfg(not(target_os = "macos"))]
fn detect_generic_gpu() -> Option<AcceleratorProfile> {
    let line = probe_generic_gpu()?;
    let (device_name, vram_gb) = parse_generic_gpu(&line)?;
    Some(AcceleratorProfile::Generic {
        device_name,
        vram_gb,
    })
}

#[cfg(target_os = "windows")]
fn probe_generic_gpu() -> Option<String> {
    // The class key of display adapters. `qwMemorySize` is the 64-bit size the
    // driver writes; `AdapterRAM` from WMI saturates at 4095 MiB. The largest
    // adapter wins, which on a laptop is the discrete card rather than the
    // integrated one.
    const SCRIPT: &str = concat!(
        r"$k = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\",
        r"{4d36e968-e325-11ce-bfc1-08002be10318}\*'; ",
        "Get-ItemProperty $k -ErrorAction SilentlyContinue | ",
        "Where-Object { $_.'HardwareInformation.qwMemorySize' -gt 0 } | ",
        "Sort-Object -Property 'HardwareInformation.qwMemorySize' -Descending | ",
        "Select-Object -First 1 | ",
        "ForEach-Object { $_.DriverDesc + [char]9 + $_.'HardwareInformation.qwMemorySize' }",
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!line.is_empty()).then_some(line)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn probe_generic_gpu() -> Option<String> {
    // amdgpu and i915 both publish the card's name and its VRAM under
    // /sys/class/drm. Nothing is spawned: this is two file reads.
    let cards = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in cards.flatten() {
        let device = entry.path().join("device");
        let Ok(bytes) = std::fs::read_to_string(device.join("mem_info_vram_total")) else {
            continue;
        };
        let name = std::fs::read_to_string(device.join("product_name"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "GPU".to_string());
        return Some(format!("{name}\t{}", bytes.trim()));
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn detect_nvidia_cuda() -> Option<AcceleratorProfile> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,compute_cap",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.splitn(3, ',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }

    let device_name = parts[0].to_string();
    let vram_mb: f64 = parts[1].parse().ok()?;
    let vram_gb = vram_mb / 1024.0;

    // compute_cap format: "8.9"
    let cc_parts: Vec<&str> = parts[2].split('.').collect();
    let major: u8 = cc_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u8 = cc_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    Some(AcceleratorProfile::Cuda {
        device_name,
        vram_gb,
        compute_capability: (major, minor),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a profile with a 24 GB budget
    // WHEN  CompatibilityBadge::compute with a 10 GB file
    // THEN  badge Fits (10*1.1=11 < 24*0.7=16.8)
    #[test]
    fn test_badge_fits() {
        let profile = HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 20.0,
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::AppleSilicon {
                chip: "M3 Max".to_string(),
                generation: 3,
                vram_gb: 32.0,
            },
            memory_budget_gb: 24.0,
        };
        assert_eq!(
            CompatibilityBadge::compute(10.0, &profile),
            CompatibilityBadge::Fits
        );
    }

    // GIVEN a profile with a 24 GB budget
    // WHEN  CompatibilityBadge::compute with a 19 GB file
    // THEN  badge MightFit (19*1.1=20.9 between 16.8 and 24)
    #[test]
    fn test_badge_might_fit() {
        let profile = HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 20.0,
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::AppleSilicon {
                chip: "M3 Max".to_string(),
                generation: 3,
                vram_gb: 32.0,
            },
            memory_budget_gb: 24.0,
        };
        assert_eq!(
            CompatibilityBadge::compute(19.0, &profile),
            CompatibilityBadge::MightFit
        );
    }

    // GIVEN a profile with a 24 GB budget
    // WHEN  CompatibilityBadge::compute with a 25 GB file
    // THEN  badge TooLarge (25*1.1=27.5 > 24)
    #[test]
    fn test_badge_too_large() {
        let profile = HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 20.0,
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::AppleSilicon {
                chip: "M3 Max".to_string(),
                generation: 3,
                vram_gb: 32.0,
            },
            memory_budget_gb: 24.0,
        };
        assert_eq!(
            CompatibilityBadge::compute(25.0, &profile),
            CompatibilityBadge::TooLarge
        );
    }

    // GIVEN detect() called on the test machine
    // WHEN  the basic fields are checked
    // THEN  RAM > 0 and cpu_cores > 0
    #[test]
    fn test_detect_returns_valid_profile() {
        let profile = detect();
        assert!(profile.total_ram_gb > 0.0, "RAM should be > 0");
        assert!(profile.cpu_cores > 0, "CPU cores should be > 0");
        assert!(profile.memory_budget_gb > 0.0, "Budget should be > 0");
    }

    // GIVEN chip "M4 Max"
    // WHEN  parse_apple_silicon_generation()
    // THEN  returns 4
    #[cfg(target_os = "macos")]
    #[test]
    fn test_parse_generation_m4() {
        assert_eq!(parse_apple_silicon_generation("M4 Max"), 4);
        assert_eq!(parse_apple_silicon_generation("M3 Pro"), 3);
        assert_eq!(parse_apple_silicon_generation("M1"), 1);
    }
}

// Platform-gated with the parsers they cover: the probes exist on Linux
// and Windows alone, and CI runs the suite on both.
#[cfg(all(test, not(target_os = "macos")))]
mod engine_device_tests {
    use super::parse_engine_device_line;

    #[test]
    fn a_vulkan_device_line_yields_its_name_and_its_real_memory() {
        // GIVEN the line the bundled engine printed on the machine where the
        // registry probe returned nothing at all
        let line = "  Vulkan0: AMD Radeon RX 6900 XT (16368 MiB, 7313 MiB free)";

        // WHEN it is read
        let parsed = parse_engine_device_line(line);

        // THEN the card is named and its memory is the card's, not the
        // saturated 4095 MiB that WMI reports for the same adapter
        let (name, vram) = parsed.expect("the line describes a device");
        assert_eq!(name, "AMD Radeon RX 6900 XT");
        assert!((vram - 15.984_375).abs() < 0.001, "vram was {vram}");
    }

    #[test]
    fn the_heading_of_the_listing_is_not_read_as_a_device() {
        // GIVEN the first line of the listing, which names no device
        let line = "Available devices:";

        // WHEN it is read by the same rule as the others
        let parsed = parse_engine_device_line(line);

        // THEN it yields nothing, so a build whose engine sees no card is not
        // credited with one called "Available devices"
        assert!(parsed.is_none());
    }

    #[test]
    fn a_device_reporting_no_memory_is_refused() {
        // GIVEN a listing entry whose memory reads zero
        let line = "  Vulkan0: Some Adapter (0 MiB, 0 MiB free)";

        // WHEN it is read
        let parsed = parse_engine_device_line(line);

        // THEN it is refused rather than accepted with a zero budget, which
        // would size every recommendation to nothing
        assert!(parsed.is_none());
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod generic_gpu_tests {
    use super::{parse_generic_gpu, vram_gb_from_bytes};

    #[test]
    fn a_sixteen_gigabyte_card_reads_as_sixteen_not_four() {
        // GIVEN the byte count a 16 GB Radeon writes to the registry, the
        // value WMI would have saturated at 4095 MiB
        let raw = "17163091968";

        // WHEN it is converted
        let gb = vram_gb_from_bytes(raw).expect("a positive size parses");

        // THEN the card is sized as itself, within a tenth of a gibibyte
        assert!((gb - 15.98).abs() < 0.1, "got {gb}");
    }

    #[test]
    fn a_probe_line_yields_the_card_and_its_memory() {
        // GIVEN the line the platform probes emit, name then size
        let line = "AMD Radeon RX 6900 XT\t17163091968";

        // WHEN it is parsed
        let (name, gb) = parse_generic_gpu(line).expect("a well-formed line parses");

        // THEN both halves come back
        assert_eq!(name, "AMD Radeon RX 6900 XT");
        assert!(gb > 15.0, "got {gb}");
    }

    #[test]
    fn a_card_whose_size_is_unknown_still_names_itself() {
        // GIVEN a probe that found the adapter but no usable size, which is
        // what an integrated GPU with shared memory reports
        let line = "Intel(R) UHD Graphics\t0";

        // WHEN it is parsed
        let (name, gb) = parse_generic_gpu(line).expect("the name alone is enough");

        // THEN the accelerator is still named, with a size of zero rather than
        // the machine being called CPU-only
        assert_eq!(name, "Intel(R) UHD Graphics");
        assert_eq!(gb, 0.0);
    }
}

#[cfg(test)]
mod generic_budget_tests {
    use super::{compute_budget, AcceleratorProfile};

    #[test]
    fn a_discrete_card_is_sized_by_its_own_memory() {
        // GIVEN a 16 GB discrete card on a machine with 64 GB of system RAM
        let accel = AcceleratorProfile::Generic {
            device_name: "AMD Radeon RX 6900 XT".to_string(),
            vram_gb: 16.0,
        };

        // WHEN the budget is computed
        let budget = compute_budget(&accel, 64.0);

        // THEN it is the card's memory, not a share of the system RAM, which
        // is what offered 38 GB models to a 16 GB card
        assert_eq!(budget, 16.0);
    }

    #[test]
    fn an_integrated_card_falls_back_to_system_memory() {
        // GIVEN an integrated GPU, which reports no dedicated memory
        let accel = AcceleratorProfile::Generic {
            device_name: "Intel(R) UHD Graphics".to_string(),
            vram_gb: 0.0,
        };

        // WHEN the budget is computed on a 32 GB machine
        let budget = compute_budget(&accel, 32.0);

        // THEN it shares system RAM rather than being sized on zero, which
        // would have made every model unloadable
        assert!((budget - 19.2).abs() < 0.01, "got {budget}");
    }
}
