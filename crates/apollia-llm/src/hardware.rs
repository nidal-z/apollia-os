//! Détection hardware pour les recommandations de modèles LLM.
//!
//! Expose [`HardwareProfile`] avec RAM, CPU, et accélérateur (Metal / CUDA / None).
//! Utilisé par le Model Hub pour afficher les badges de compatibilité GGUF.

use serde::{Deserialize, Serialize};
use sysinfo::System;

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Profil matériel détecté de la machine locale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    /// RAM totale en GB.
    pub total_ram_gb: f64,
    /// RAM disponible en GB.
    pub available_ram_gb: f64,
    /// Nom/modèle du CPU (ex. `"Apple M4 Max"`, `"Intel Core i9-13900K"`).
    pub cpu_model: String,
    /// Nombre de cœurs logiques.
    pub cpu_cores: u32,
    /// Accélérateur graphique détecté.
    pub accelerator: AcceleratorProfile,
    /// Budget mémoire recommandé pour l'inférence en GB.
    ///
    /// - Apple Silicon : RAM unifiée × 0.75 (partagée CPU/GPU)
    /// - CUDA : VRAM dédiée
    /// - CPU seul : RAM × 0.60 (conserver de la RAM pour l'OS et les processus)
    pub memory_budget_gb: f64,
}

/// Accélérateur graphique disponible pour l'inférence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceleratorProfile {
    /// Aucun GPU utilisable — inférence CPU uniquement.
    None,
    /// Apple Silicon avec mémoire unifiée (Metal).
    AppleSilicon {
        /// Nom du chip (ex. `"M4 Max"`, `"M3 Pro"`).
        chip: String,
        /// Génération (1 = M1, 2 = M2, 3 = M3, 4 = M4).
        generation: u8,
        /// VRAM effectif = RAM unifiée totale (en GB).
        vram_gb: f64,
    },
    /// GPU NVIDIA (CUDA).
    Cuda {
        /// Nom du GPU (ex. `"NVIDIA GeForce RTX 4090"`).
        device_name: String,
        /// VRAM dédiée en GB.
        vram_gb: f64,
        /// Compute capability sous forme `(major, minor)`.
        compute_capability: (u8, u8),
    },
    /// GPU autre (AMD, Intel Arc, etc.) via accélération générique.
    Generic {
        /// Nom du GPU.
        device_name: String,
        /// VRAM en GB (si détectable).
        vram_gb: f64,
    },
}

/// Badge de compatibilité d'un fichier GGUF avec le hardware local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityBadge {
    /// Le modèle tient confortablement en mémoire (≤ 70 % du budget).
    Fits,
    /// Le modèle pourrait tenir mais sera serré (70 – 100 % du budget).
    MightFit,
    /// Le modèle dépasse le budget mémoire disponible.
    TooLarge,
}

impl CompatibilityBadge {
    /// Calcule le badge depuis la taille du fichier (en GB) et le profil hardware.
    pub fn compute(file_size_gb: f64, profile: &HardwareProfile) -> Self {
        let required = file_size_gb * 1.1; // overhead llama.cpp ~10 %
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

// ─────────────────────────────────────────────
// Detection
// ─────────────────────────────────────────────

/// Détecte le profil hardware de la machine locale.
///
/// Bloquant — appeler depuis `tokio::task::spawn_blocking` si nécessaire.
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

// ─────────────────────────────────────────────
// macOS — Apple Silicon detection
// ─────────────────────────────────────────────

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

// ─────────────────────────────────────────────
// Linux / Windows — NVIDIA CUDA detection
// ─────────────────────────────────────────────

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

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN un profil avec 24 GB de budget
    // WHEN  CompatibilityBadge::compute avec fichier de 10 GB
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
        assert_eq!(CompatibilityBadge::compute(10.0, &profile), CompatibilityBadge::Fits);
    }

    // GIVEN un profil avec 24 GB de budget
    // WHEN  CompatibilityBadge::compute avec fichier de 19 GB
    // THEN  badge MightFit (19*1.1=20.9 entre 16.8 et 24)
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
        assert_eq!(CompatibilityBadge::compute(19.0, &profile), CompatibilityBadge::MightFit);
    }

    // GIVEN un profil avec 24 GB de budget
    // WHEN  CompatibilityBadge::compute avec fichier de 25 GB
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
        assert_eq!(CompatibilityBadge::compute(25.0, &profile), CompatibilityBadge::TooLarge);
    }

    // GIVEN detect() appelé sur la machine de test
    // WHEN  on vérifie les champs basiques
    // THEN  RAM > 0 et cpu_cores > 0
    #[test]
    fn test_detect_returns_valid_profile() {
        let profile = detect();
        assert!(profile.total_ram_gb > 0.0, "RAM should be > 0");
        assert!(profile.cpu_cores > 0, "CPU cores should be > 0");
        assert!(profile.memory_budget_gb > 0.0, "Budget should be > 0");
    }

    // GIVEN chip "M4 Max"
    // WHEN  parse_apple_silicon_generation()
    // THEN  retourne 4
    #[cfg(target_os = "macos")]
    #[test]
    fn test_parse_generation_m4() {
        assert_eq!(parse_apple_silicon_generation("M4 Max"), 4);
        assert_eq!(parse_apple_silicon_generation("M3 Pro"), 3);
        assert_eq!(parse_apple_silicon_generation("M1"), 1);
    }
}
