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
        AcceleratorProfile::Generic { vram_gb, .. } => *vram_gb,
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
fn detect_gpu_non_macos() -> AcceleratorProfile {
    detect_nvidia_cuda().unwrap_or(AcceleratorProfile::None)
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
