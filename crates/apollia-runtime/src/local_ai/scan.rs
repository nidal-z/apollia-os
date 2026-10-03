//! What the machine can run, and which GGUF and Whisper models it already has
//! on disk, each graded by the estimator for this machine.
//!
//! These reads belong to the machine the runtime runs on: the models folder is
//! the runtime's own, and the model the user picks is loaded by the runtime.

use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AI Setup - types
// ---------------------------------------------------------------------------

/// System information used to compute model recommendations in the AI setup step.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SystemInfo {
    /// Total RAM in gigabytes.
    pub total_ram_gb: f64,
    /// Available RAM in gigabytes.
    pub available_ram_gb: f64,
    /// Operating system identifier (e.g. `"macos"`, `"linux"`).
    pub os: String,
    /// CPU architecture identifier (e.g. `"aarch64"`, `"x86_64"`).
    pub arch: String,
    /// Whether a GPU is available (basic heuristic).
    pub gpu_available: bool,
    /// The accelerator's dedicated memory in gigabytes, when the engine's
    /// device list names one. `None` on a processor-only machine and on
    /// platforms where the heuristic above answers without a device list.
    pub gpu_vram_gb: Option<f64>,
}

/// GGUF model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GgufModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Human-readable file size (e.g. `"4.2 GB"`).
    pub size_human: String,
    /// Whether the memory estimator says this model fits this machine.
    pub recommended: bool,
}

/// Whisper GGML model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WhisperModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Size variant parsed from the filename (`tiny`, `base`, `small`, `medium`, `large`).
    pub model_size: String,
    /// Whether the dictation verdict says this file fits this machine.
    pub recommended: bool,
}

// ---------------------------------------------------------------------------
// AI Setup - scans
// ---------------------------------------------------------------------------

/// Scans standard filesystem locations for `.gguf` model files.
///
/// Locations scanned:
/// 1. the models folder in force (`[models] dir`, else `<data_dir>/models/`), flat
/// 2. `<home>/Downloads/` (flat)
/// 3. `<home>/.cache/lm-studio/models/` (recursive, up to 4 levels deep)
///
/// Results are sorted by file size descending. Missing or unreadable
/// directories are silently skipped - an empty list is not an error.
/// Blocking: reads the disk and probes each header.
pub fn scan_gguf_models(home: &Path, data_dir: &Path) -> Vec<GgufModelInfo> {
    let hardware = apollia_llm::hardware::detect();
    let flat_dirs = [
        apollia_core::config::models_dir(data_dir),
        home.join("Downloads"),
    ];
    let lm_studio_root = home.join(".cache").join("lm-studio").join("models");

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut models: Vec<GgufModelInfo> = Vec::new();

    for dir in &flat_dirs {
        for mut info in scan_gguf_in_dir(dir) {
            if seen.insert(info.path.clone()) {
                info.recommended = fits_this_machine(&info, &hardware);
                models.push(info);
            }
        }
    }

    let mut recursive_buf: Vec<GgufModelInfo> = Vec::new();
    collect_gguf_recursive(&lm_studio_root, &mut recursive_buf, 0, 4);
    for mut info in recursive_buf {
        if seen.insert(info.path.clone()) {
            info.recommended = fits_this_machine(&info, &hardware);
            models.push(info);
        }
    }

    models.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
    models
}

/// Scans standard filesystem locations for Whisper GGML model files.
///
/// Locations scanned:
/// 1. `<data_dir>/models/ggml-*.bin`
/// 2. `<home>/Downloads/ggml-*.bin`
/// 3. `<home>/.cache/whisper/*.bin`
///
/// Only filenames matching the `ggml-(tiny|base|small|medium|large)` pattern
/// are returned. An empty list is not an error. Blocking: reads the disk.
pub fn scan_whisper_models(home: &Path, data_dir: &Path) -> Vec<WhisperModelInfo> {
    let hardware = apollia_llm::hardware::detect();
    // Dictation models stay in the default folder whatever `[models] dir` says.
    let scan_dirs = [
        apollia_core::config::default_models_dir(data_dir),
        home.join("Downloads"),
        home.join(".cache").join("whisper"),
    ];

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut models: Vec<WhisperModelInfo> = Vec::new();

    for dir in &scan_dirs {
        for model in scan_whisper_in_dir(dir, &hardware) {
            if seen.insert(model.path.clone()) {
                models.push(model);
            }
        }
    }
    models
}

// ---------------------------------------------------------------------------
// AI Setup - helper functions
// ---------------------------------------------------------------------------

/// Returns current system information synchronously.
///
/// Memory is read from the hardware profile every estimator reads (the total
/// computed once, the available memory on each call); OS and architecture are
/// compile-time constants. Blocking: call it from `spawn_blocking`.
pub fn system_info() -> SystemInfo {
    let hardware = apollia_llm::hardware::detect();
    SystemInfo {
        total_ram_gb: hardware.total_ram_gb,
        available_ram_gb: hardware.available_ram_gb,
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        gpu_available: detect_gpu_basic(),
        gpu_vram_gb: vram_gb_of(&hardware.accelerator),
    }
}

/// The accelerator's memory as the engine's device list reports it, whatever
/// the accelerator's kind: an operator choosing a model wants the number, not
/// the word. `None` when the profile names no accelerator.
fn vram_gb_of(accelerator: &apollia_llm::AcceleratorProfile) -> Option<f64> {
    use apollia_llm::AcceleratorProfile;
    match accelerator {
        AcceleratorProfile::None => None,
        AcceleratorProfile::AppleSilicon { vram_gb, .. }
        | AcceleratorProfile::Cuda { vram_gb, .. }
        | AcceleratorProfile::Generic { vram_gb, .. } => Some(*vram_gb),
    }
}

/// Scans a single directory (non-recursive) for `.gguf` files.
///
/// Returns an empty `Vec` for non-existent or unreadable directories.
/// The `recommended` field defaults to `false`; callers grade it with the estimator.
pub(crate) fn scan_gguf_in_dir(dir: &std::path::Path) -> Vec<GgufModelInfo> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let size_bytes = size_on_disk(&path);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        results.push(GgufModelInfo {
            path: path.display().to_string(),
            filename,
            size_bytes,
            size_human: format_size_human(size_bytes),
            recommended: false,
        });
    }
    results
}

/// Collects `.gguf` files recursively from `dir` up to `max_depth` levels.
///
/// Fills `out` in place. Unreadable directories and symlink loops are
/// silently skipped via `file_type()` rather than `is_dir()`.
fn collect_gguf_recursive(
    dir: &std::path::Path,
    out: &mut Vec<GgufModelInfo>,
    depth: u8,
    max_depth: u8,
) {
    if depth >= max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            collect_gguf_recursive(&path, out, depth + 1, max_depth);
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            let size_bytes = size_on_disk(&path);
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(GgufModelInfo {
                path: path.display().to_string(),
                filename,
                size_bytes,
                size_human: format_size_human(size_bytes),
                recommended: false,
            });
        }
    }
}

/// Scans a single directory for Whisper GGML model files.
///
/// Only files matching `ggml-(tiny|base|small|medium|large)*.bin` are included.
/// Each is judged by the dictation verdict the catalogue gives its entries.
/// Returns an empty `Vec` for non-existent or unreadable directories.
fn scan_whisper_in_dir(
    dir: &std::path::Path,
    hardware: &apollia_llm::HardwareProfile,
) -> Vec<WhisperModelInfo> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let filename = match path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            Some(n) => n,
            None => continue,
        };

        if !filename.starts_with("ggml-") || !filename.ends_with(".bin") {
            continue;
        }

        let model_size = match detect_whisper_model_size(&filename) {
            Some(s) => s,
            None => continue,
        };

        let size_bytes = size_on_disk(&path);
        let recommended = apollia_llm::recommend::dictation_fits(size_bytes, hardware);

        results.push(WhisperModelInfo {
            path: path.display().to_string(),
            filename,
            size_bytes,
            model_size,
            recommended,
        });
    }
    results
}

/// Parses the Whisper size variant from a filename.
///
/// Matches `ggml-(tiny|base|small|medium|large)` in the lowercased filename.
/// Falls back to `"base"` for any other `ggml-*.bin` file (e.g. `ggml-model-q5_0.bin`)
/// so that generic Whisper GGML models are still detected.
pub(crate) fn detect_whisper_model_size(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    for variant in ["tiny", "base", "small", "medium", "large"] {
        let pattern = format!("ggml-{variant}");
        if lower.contains(pattern.as_str()) {
            return Some(variant.to_owned());
        }
    }
    // Accept any ggml-*.bin as a valid Whisper model with unknown size.
    if lower.starts_with("ggml-") && lower.ends_with(".bin") {
        return Some("base".to_owned());
    }
    None
}

/// Whether a model file on disk fits this machine, by the estimator every
/// other surface reads.
///
/// The header is probed from the file itself, so the cache is sized from the
/// model's own architecture; an unreadable header falls back to the
/// estimator's proportional cache rather than to a size threshold.
fn fits_this_machine(info: &GgufModelInfo, hardware: &apollia_llm::HardwareProfile) -> bool {
    let facts = apollia_llm::gguf_probe::probe_file(
        std::path::Path::new(&info.path),
        apollia_llm::gguf_probe::SCREEN_BYTES,
    )
    .unwrap_or_default();
    recommended_for(info.size_bytes, &facts, hardware)
}

/// The onboarding verdict for a file of `size_bytes` with header `facts`.
fn recommended_for(
    size_bytes: u64,
    facts: &apollia_llm::gguf_probe::GgufHeaderFacts,
    hardware: &apollia_llm::HardwareProfile,
) -> bool {
    apollia_llm::recommend::assess_fit(
        size_bytes,
        facts,
        hardware,
        &apollia_llm::recommend::RuntimeShape::default(),
    )
    .badge
        == apollia_llm::CompatibilityBadge::Fits
}

/// Formats a byte count as a human-readable string using binary divisions.
///
/// Examples: `4_500_000_000` → `"4.2 GB"`, `500_000_000` → `"476.8 MB"`.
/// Size of a model file, read through the path rather than the directory entry.
///
/// `DirEntry::metadata` does not follow a link, and on Windows a file inside a
/// synchronised folder is a reparse point: the call succeeds and answers zero
/// while the file is perfectly valid. The onboarding then listed a model, and
/// a dictation model beside it, at 0 B. Reported on 2026-09-08 on the packaged
/// Windows build. A failure is logged rather than swallowed, because a size
/// silently read as zero is what made this take a user report to notice.
fn size_on_disk(path: &std::path::Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "model.size.unreadable"
            );
            0
        }
    }
}

pub(crate) fn format_size_human(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Detects GPU availability using platform-specific heuristics.
fn detect_gpu_basic() -> bool {
    #[cfg(target_os = "macos")]
    {
        // All macOS machines expose Metal GPU support
        true
    }
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/dri/card0").exists()
            || std::path::Path::new("/proc/driver/nvidia").exists()
    }
    #[cfg(target_os = "windows")]
    {
        // Ask the engine, not the operating system. This arm answered `false`
        // unconditionally, so no Windows machine ever saw the chip, including
        // one whose engine was loading every layer onto a Radeon (measured
        // 2026-09-09). The profile is computed once per process, so this costs
        // nothing after the first read.
        apollia_llm::hardware::detect().accelerator.is_available()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

#[cfg(test)]
mod tests;
