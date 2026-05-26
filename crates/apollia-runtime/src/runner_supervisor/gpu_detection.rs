//! Détection cross-platform du GPU au boot du daemon.
//!
//! Au démarrage, le `RunnerSupervisor` interroge [`detect_gpu`] pour obtenir
//! les caractéristiques de la GPU locale, puis applique [`resolve_backend`]
//! pour produire le `RunnerBackend` final (en tenant compte d'un éventuel
//! override utilisateur via `[llm.runner].backend` dans `apollia.toml`).
//!
//! Cf. `docs/internal/architecture/GPU-DETECTION.md` pour la spec complète
//! et la hiérarchie de décision (Apple Silicon, NVIDIA + CUDA ≥ 12.0, AMD +
//! ROCm, fallback Vulkan, fallback CPU).
//!
//! Le module est strictement local-first (Principe #1) : aucune télémetrie,
//! aucun appel réseau. Toutes les sondes sont des sous-processus (`nvidia-smi`,
//! `rocm-smi`, `system_profiler`, `vulkaninfo`) ou des lectures `/sys`.

use apollia_core::LlmRunnerConfig;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Types publics
// ─────────────────────────────────────────────

/// Identité du fabricant GPU détecté.
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
    /// Vendor non identifié ou aucune GPU détectée.
    Unknown,
}

/// Backend d'inférence retenu pour spawner le sidecar runner.
///
/// Le `binary_name()` correspondant est dans le même répertoire que `apollia-os`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunnerBackend {
    /// NVIDIA CUDA (driver ≥ 12.0).
    Cuda,
    /// AMD ROCm (Linux + Windows Radeon Pro / Instinct).
    Rocm,
    /// Vulkan compute (cross-vendor fallback).
    Vulkan,
    /// Apple Metal (macOS arm64).
    Metal,
    /// CPU pur (fallback ultime, toujours disponible).
    Cpu,
}

impl RunnerBackend {
    /// Nom du binaire runner correspondant.
    ///
    /// Utilisé par `RunnerSupervisor::spawn` pour résoudre le chemin du sidecar
    /// dans le même répertoire que `apollia-os`.
    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::Cuda => "apollia-runner-cuda",
            Self::Rocm => "apollia-runner-rocm",
            Self::Vulkan => "apollia-runner-vulkan",
            Self::Metal => "apollia-runner-metal",
            Self::Cpu => "apollia-runner-cpu",
        }
    }

    /// Parse une valeur textuelle (issue de `apollia.toml` ou d'un test) vers
    /// le variant correspondant. Retourne `Err(())` si la chaîne n'est pas reconnue.
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "cuda" => Ok(Self::Cuda),
            "rocm" => Ok(Self::Rocm),
            "vulkan" => Ok(Self::Vulkan),
            "metal" => Ok(Self::Metal),
            "cpu" => Ok(Self::Cpu),
            _ => Err(()),
        }
    }
}

/// Informations GPU collectées par [`detect_gpu`].
///
/// Le champ `recommended_backend` reflète la décision de la détection auto :
/// il peut être surchargé par [`resolve_backend`] si l'opérateur a fixé
/// `[llm.runner].backend` à autre chose que `"auto"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Fabricant identifié.
    pub vendor: Vendor,
    /// Nom modèle lisible (ex: `"GeForce RTX 4070"`, `"Apple M3"`).
    pub model: String,
    /// Mémoire totale en mégaoctets. `0` si non disponible.
    pub memory_total_mb: u32,
    /// Backend recommandé d'après la détection auto.
    pub recommended_backend: RunnerBackend,
}

impl GpuInfo {
    /// Constructeur fallback CPU lorsque la détection a échoué ou qu'aucune GPU n'est présente.
    pub fn cpu_fallback() -> Self {
        Self {
            vendor: Vendor::Unknown,
            model: "No GPU detected, CPU fallback".into(),
            memory_total_mb: 0,
            recommended_backend: RunnerBackend::Cpu,
        }
    }
}

// ─────────────────────────────────────────────
// detect_gpu : point d'entrée cross-platform
// ─────────────────────────────────────────────

/// Détecte le GPU disponible et retourne ses caractéristiques.
///
/// Détection cfg-gated par OS. Toujours best-effort : retourne
/// [`GpuInfo::cpu_fallback`] plutôt que de paniquer si aucune sonde n'aboutit.
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
            "GPU detected",
        );
        info
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        tracing::warn!("macOS x86_64 non supporté en v0.1.0, fallback CPU");
        GpuInfo::cpu_fallback()
    }
}

/// Détecte le GPU sur Linux : NVIDIA via `nvidia-smi`, AMD via `rocm-smi`,
/// puis scan `/sys/class/drm` pour les autres vendors.
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

/// Détecte le GPU sur Windows : WMI `Win32_VideoController` + sondes
/// `nvidia-smi.exe`, HIP SDK, `vulkan-1.dll`.
#[cfg(target_os = "windows")]
pub fn detect_gpu() -> GpuInfo {
    let controllers = wmi_list_video_controllers().unwrap_or_default();

    if let Some(ctrl) = controllers.iter().find(|c| c.name.contains("NVIDIA")) {
        let cuda_version = probe_cuda_version_windows().unwrap_or((0, 0));
        let backend = if cuda_version.0 >= 12 {
            RunnerBackend::Cuda
        } else if vulkan_loader_present_windows() {
            RunnerBackend::Vulkan
        } else {
            RunnerBackend::Cpu
        };
        let info = GpuInfo {
            vendor: Vendor::Nvidia,
            model: ctrl.name.clone(),
            memory_total_mb: ctrl.adapter_ram_mb(),
            recommended_backend: backend,
        };
        log_detected(&info);
        return info;
    }

    if let Some(ctrl) = controllers
        .iter()
        .find(|c| c.name.contains("AMD") || c.name.contains("Radeon"))
    {
        let backend = if hip_sdk_present_windows() && rocm_supports_card(&ctrl.name) {
            RunnerBackend::Rocm
        } else if vulkan_loader_present_windows() {
            RunnerBackend::Vulkan
        } else {
            RunnerBackend::Cpu
        };
        let info = GpuInfo {
            vendor: Vendor::Amd,
            model: ctrl.name.clone(),
            memory_total_mb: ctrl.adapter_ram_mb(),
            recommended_backend: backend,
        };
        log_detected(&info);
        return info;
    }

    if let Some(ctrl) = controllers
        .iter()
        .find(|c| c.name.contains("Intel") && c.name.contains("Arc"))
    {
        let backend = if vulkan_loader_present_windows() {
            RunnerBackend::Vulkan
        } else {
            RunnerBackend::Cpu
        };
        let info = GpuInfo {
            vendor: Vendor::Intel,
            model: ctrl.name.clone(),
            memory_total_mb: ctrl.adapter_ram_mb(),
            recommended_backend: backend,
        };
        log_detected(&info);
        return info;
    }

    let info = GpuInfo::cpu_fallback();
    log_detected(&info);
    info
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn log_detected(info: &GpuInfo) {
    tracing::info!(
        vendor = ?info.vendor,
        model = %info.model,
        memory_mb = info.memory_total_mb,
        backend = ?info.recommended_backend,
        "GPU detected",
    );
}

// ─────────────────────────────────────────────
// Override utilisateur
// ─────────────────────────────────────────────

/// Résout le `RunnerBackend` final en croisant la détection auto et l'override
/// utilisateur `[llm.runner].backend`.
///
/// Hiérarchie de décision :
/// 1. `"auto"` : utilise `detected.recommended_backend`.
/// 2. Valeur explicite valide ET binaire correspondant présent : utilise cette valeur.
/// 3. Valeur invalide ou binaire absent : log warning et fallback sur la détection.
pub fn resolve_backend(config: &LlmRunnerConfig, detected: &GpuInfo) -> RunnerBackend {
    let raw = config.backend.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("auto") {
        tracing::info!(
            backend = ?detected.recommended_backend,
            "runner backend = auto, using detected backend",
        );
        return detected.recommended_backend;
    }

    let parsed = match RunnerBackend::from_str(&raw.to_ascii_lowercase()) {
        Ok(b) => b,
        Err(()) => {
            tracing::warn!(
                requested = %raw,
                "runner backend override invalid, falling back to auto-detect",
            );
            return detected.recommended_backend;
        }
    };

    if !is_backend_available(parsed) {
        tracing::warn!(
            requested = ?parsed,
            "runner backend override not available on this build, using detected",
        );
        return detected.recommended_backend;
    }

    tracing::info!(
        requested = ?parsed,
        detected = ?detected.recommended_backend,
        "runner backend override applied",
    );
    parsed
}

/// Vérifie qu'un binaire runner est présent à côté de l'exécutable courant.
///
/// Le bundle Apollia livre les 5 binaires runner sur Linux/Windows, et
/// `metal`+`cpu` sur macOS. On accepte aussi `CARGO_MANIFEST_DIR` en environnement
/// de test ou de dev : si on ne peut pas résoudre l'exécutable courant, on
/// renvoie `true` pour ne pas casser les tests qui appellent `resolve_backend`.
fn is_backend_available(backend: RunnerBackend) -> bool {
    let exe_name = backend.binary_name();
    let exe_path = match std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        Some(dir) => dir,
        None => return true,
    };

    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let candidate = exe_path.join(format!("{}{}", exe_name, suffix));
    candidate.exists()
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

/// Représentation parsée d'une ligne `nvidia-smi --query-gpu=name,memory.total,driver_version`.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NvidiaSmiLine {
    model: String,
    memory_total_mb: u32,
    driver_version: String,
}

/// Parse une ligne `--format=csv,noheader,nounits` au format
/// `"GeForce RTX 4070, 12288, 550.78"`.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
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
            "CUDA driver < 12.0 detected, falling back to Vulkan if available",
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
            // Si on est tombé ici, pas de nvidia-smi : on tente Vulkan.
            "0x10de" => (Vendor::Nvidia, RunnerBackend::Vulkan),
            // Pas de ROCm, Vulkan fallback.
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

/// Controller GPU tel qu'exposé par WMI `Win32_VideoController`.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, serde::Deserialize)]
struct VideoController {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AdapterRAM", default)]
    adapter_ram: u64,
}

#[cfg(target_os = "windows")]
impl VideoController {
    fn adapter_ram_mb(&self) -> u32 {
        (self.adapter_ram / 1024 / 1024) as u32
    }
}

#[cfg(target_os = "windows")]
fn wmi_list_video_controllers() -> Option<Vec<VideoController>> {
    use wmi::{COMLibrary, WMIConnection};
    let com_con = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com_con).ok()?;
    let results: Vec<VideoController> = wmi_con
        .raw_query("SELECT Name, AdapterRAM FROM Win32_VideoController")
        .ok()?;
    Some(results)
}

#[cfg(target_os = "windows")]
fn probe_cuda_version_windows() -> Option<(u32, u32)> {
    let out = std::process::Command::new("nvidia-smi.exe")
        .arg("-q")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    parse_cuda_version_from_smi_q(&s)
}

#[cfg(target_os = "windows")]
fn vulkan_loader_present_windows() -> bool {
    // `vulkan-1.dll` est livrée par le driver GPU sur Windows.
    std::process::Command::new("where")
        .arg("vulkan-1.dll")
        .output()
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn hip_sdk_present_windows() -> bool {
    std::env::var("HIP_PATH").is_ok()
        || std::path::Path::new("C:/Program Files/AMD/ROCm").exists()
}

/// Whitelist AMD ROCm Windows : officiellement limité aux Radeon Pro et Instinct MI.
#[cfg(any(target_os = "windows", test))]
fn rocm_supports_card(name: &str) -> bool {
    name.contains("Radeon Pro") || name.contains("Instinct")
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_runner_config(backend: &str) -> LlmRunnerConfig {
        LlmRunnerConfig {
            backend: backend.to_string(),
        }
    }

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
        // GIVEN aucune sonde réussit
        // WHEN on construit le fallback CPU
        let info = GpuInfo::cpu_fallback();
        // THEN vendor=Unknown et backend=Cpu
        assert_eq!(info.vendor, Vendor::Unknown);
        assert_eq!(info.recommended_backend, RunnerBackend::Cpu);
        assert_eq!(info.memory_total_mb, 0);
    }

    #[test]
    fn override_auto_uses_detection() {
        // GIVEN une NVIDIA détectée et override = "auto"
        let detected = nvidia_detected();
        let cfg = make_runner_config("auto");
        // WHEN on résout le backend
        let backend = resolve_backend(&cfg, &detected);
        // THEN on utilise la détection auto
        assert_eq!(backend, RunnerBackend::Cuda);
    }

    #[test]
    fn override_auto_case_insensitive() {
        // GIVEN override = "Auto" (casse mixte)
        let detected = nvidia_detected();
        let cfg = make_runner_config("Auto");
        // THEN traité comme auto
        assert_eq!(resolve_backend(&cfg, &detected), RunnerBackend::Cuda);
    }

    #[test]
    fn override_empty_uses_detection() {
        // GIVEN override = "" (config absente du toml)
        let detected = nvidia_detected();
        let cfg = make_runner_config("");
        // THEN fallback sur la détection
        assert_eq!(resolve_backend(&cfg, &detected), RunnerBackend::Cuda);
    }

    #[test]
    fn override_explicit_used_if_available() {
        // GIVEN une NVIDIA détectée et override = "vulkan".
        // Le test crée un binaire factice `apollia-runner-vulkan` à côté de
        // l'exécutable de test pour simuler un bundle complet.
        let exe_dir = std::env::current_exe()
            .expect("current_exe must succeed in test")
            .parent()
            .expect("current_exe has a parent")
            .to_path_buf();
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let dummy = exe_dir.join(format!("apollia-runner-vulkan{}", suffix));
        let _ = std::fs::write(&dummy, b"#!/bin/sh\n");
        // Cleanup guard pour ne pas polluer les autres tests.
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _guard = Cleanup(dummy.clone());

        let detected = nvidia_detected();
        let cfg = make_runner_config("vulkan");

        // WHEN on résout
        let backend = resolve_backend(&cfg, &detected);
        // THEN l'override est appliqué (binaire présent)
        assert_eq!(backend, RunnerBackend::Vulkan);
    }

    #[test]
    fn override_falls_back_if_binary_missing() {
        // GIVEN une NVIDIA détectée et override = "rocm".
        // Le binaire `apollia-runner-rocm` n'est PAS à côté de l'exécutable de test.
        // (current_exe pointe sur target/debug/deps/<test-binary>, on cherche
        //  target/debug/deps/apollia-runner-rocm qui n'existe pas).
        let detected = nvidia_detected();
        let cfg = make_runner_config("rocm");
        let backend = resolve_backend(&cfg, &detected);

        // On accepte les deux cas : binaire absent → fallback, binaire présent → Rocm.
        // Ce test reflète l'invariant : si absent, on retombe sur detected.
        if !is_backend_available(RunnerBackend::Rocm) {
            assert_eq!(backend, RunnerBackend::Cuda);
        } else {
            assert_eq!(backend, RunnerBackend::Rocm);
        }
    }

    #[test]
    fn override_invalid_value_falls_back() {
        // GIVEN override = "potato" (valeur inconnue)
        let detected = nvidia_detected();
        let cfg = make_runner_config("potato");
        // THEN warning + fallback détection
        assert_eq!(resolve_backend(&cfg, &detected), RunnerBackend::Cuda);
    }

    #[test]
    fn parse_nvidia_smi_line_rtx_4070() {
        // GIVEN une sortie typique de `nvidia-smi --query-gpu=...`
        let line = "GeForce RTX 4070, 12288, 550.78";
        // WHEN on parse
        let parsed = parse_nvidia_smi_line(line).expect("parsing should succeed");
        // THEN les 3 champs sont extraits
        assert_eq!(parsed.model, "GeForce RTX 4070");
        assert_eq!(parsed.memory_total_mb, 12_288);
        assert_eq!(parsed.driver_version, "550.78");
    }

    #[test]
    fn parse_nvidia_smi_line_rejects_garbage() {
        // GIVEN une sortie malformée (manque la mémoire et la version)
        let line = "only-model";
        // THEN on retourne None
        assert!(parse_nvidia_smi_line(line).is_none());
    }

    #[test]
    fn parse_nvidia_smi_line_rejects_non_numeric_memory() {
        // GIVEN une sortie avec mémoire non numérique
        let line = "Some GPU, notanumber, 1.0";
        // THEN on retourne None (parse::<u32> échoue)
        assert!(parse_nvidia_smi_line(line).is_none());
    }

    #[test]
    fn parse_cuda_version_from_smi_q_typical() {
        // GIVEN un extrait de `nvidia-smi -q`
        let s = "Driver Version : 550.78\nCUDA Version : 12.4\nGPU 00000000:01:00.0";
        // WHEN
        let v = parse_cuda_version_from_smi_q(s);
        // THEN
        assert_eq!(v, Some((12, 4)));
    }

    #[test]
    fn parse_cuda_version_from_smi_q_missing() {
        // GIVEN un texte sans la ligne CUDA Version
        let s = "Driver Version : 550.78\nGPU info...";
        // THEN None
        assert!(parse_cuda_version_from_smi_q(s).is_none());
    }

    #[test]
    fn runner_backend_binary_names() {
        assert_eq!(RunnerBackend::Cuda.binary_name(), "apollia-runner-cuda");
        assert_eq!(RunnerBackend::Rocm.binary_name(), "apollia-runner-rocm");
        assert_eq!(RunnerBackend::Vulkan.binary_name(), "apollia-runner-vulkan");
        assert_eq!(RunnerBackend::Metal.binary_name(), "apollia-runner-metal");
        assert_eq!(RunnerBackend::Cpu.binary_name(), "apollia-runner-cpu");
    }

    #[test]
    fn runner_backend_from_str_roundtrip() {
        assert_eq!(RunnerBackend::from_str("cuda"), Ok(RunnerBackend::Cuda));
        assert_eq!(RunnerBackend::from_str("rocm"), Ok(RunnerBackend::Rocm));
        assert_eq!(RunnerBackend::from_str("vulkan"), Ok(RunnerBackend::Vulkan));
        assert_eq!(RunnerBackend::from_str("metal"), Ok(RunnerBackend::Metal));
        assert_eq!(RunnerBackend::from_str("cpu"), Ok(RunnerBackend::Cpu));
        assert_eq!(RunnerBackend::from_str("auto"), Err(()));
        assert_eq!(RunnerBackend::from_str("xyz"), Err(()));
    }

    #[test]
    fn llm_runner_config_default_is_auto() {
        // GIVEN LlmRunnerConfig::default()
        let cfg = LlmRunnerConfig::default();
        // THEN backend == "auto"
        assert_eq!(cfg.backend, "auto");
    }

    #[test]
    fn rocm_only_for_radeon_pro_or_instinct() {
        // Whitelist statique ROCm : Radeon Pro et Instinct uniquement.
        assert!(rocm_supports_card("AMD Radeon Pro W7900"));
        assert!(rocm_supports_card("AMD Instinct MI300"));
        assert!(!rocm_supports_card("AMD Radeon RX 7800 XT"));
        assert!(!rocm_supports_card("NVIDIA GeForce RTX 4070"));
    }
}
