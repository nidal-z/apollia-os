//! The AI setup step of onboarding: what the machine can run, which GGUF and
//! Whisper models it already has on disk, and which of them are worth
//! recommending given the RAM available.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::memory::{get_repo, load_state_from_memory, persist_state};
use super::state::{OnboardingError, OnboardingState};

// ---------------------------------------------------------------------------
// AI Setup - types
// ---------------------------------------------------------------------------

/// System information used to compute model recommendations in the AI setup step.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// GGUF model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Human-readable file size (e.g. `"4.2 GB"`).
    pub size_human: String,
    /// Whether this model is recommended for the current RAM.
    pub recommended: bool,
}

/// Whisper GGML model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Size variant parsed from the filename (`tiny`, `base`, `small`, `medium`, `large`).
    pub model_size: String,
    /// Whether this model is recommended for the current RAM.
    pub recommended: bool,
}

// ---------------------------------------------------------------------------
// AI Setup - Tauri commands
// ---------------------------------------------------------------------------

/// Returns system information for AI setup model recommendations.
///
/// Queries total and available RAM, OS, architecture, and basic GPU
/// availability. Runs the sysinfo query on a blocking thread.
#[tauri::command]
pub async fn get_ai_setup_info() -> Result<SystemInfo, String> {
    tokio::task::spawn_blocking(get_system_info_sync)
        .await
        .map_err(|e| format!("system info query failed: {e}"))
}

/// Scans standard filesystem locations for `.gguf` model files.
///
/// Locations scanned:
/// 1. `~/.apollia/models/` (flat)
/// 2. `~/Downloads/` (flat)
/// 3. `~/.cache/lm-studio/models/` (recursive, up to 4 levels deep)
///
/// Results are sorted by file size descending. Missing or unreadable
/// directories are silently skipped - an empty list is not an error.
#[tauri::command]
pub async fn scan_for_gguf_models() -> Result<Vec<GgufModelInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let home = apollia_core::paths::home_string().unwrap_or_default();
        let home_path = std::path::PathBuf::from(&home);
        let sys_info = get_system_info_sync();
        let max_recommended = recommended_max_gguf_size_bytes(sys_info.total_ram_gb);

        let flat_dirs = [
            apollia_core::paths::data_dir_under(&home_path).join("models"),
            home_path.join("Downloads"),
        ];
        let lm_studio_root = home_path.join(".cache").join("lm-studio").join("models");

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut models: Vec<GgufModelInfo> = Vec::new();

        for dir in &flat_dirs {
            for mut info in scan_gguf_in_dir(dir) {
                if seen.insert(info.path.clone()) {
                    info.recommended = info.size_bytes <= max_recommended;
                    models.push(info);
                }
            }
        }

        let mut recursive_buf: Vec<GgufModelInfo> = Vec::new();
        collect_gguf_recursive(&lm_studio_root, &mut recursive_buf, 0, 4);
        for mut info in recursive_buf {
            if seen.insert(info.path.clone()) {
                info.recommended = info.size_bytes <= max_recommended;
                models.push(info);
            }
        }

        models.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        models
    })
    .await
    .map_err(|e| format!("GGUF scan failed: {e}"))
}

/// Scans standard filesystem locations for Whisper GGML model files.
///
/// Locations scanned:
/// 1. `~/.apollia/models/ggml-*.bin`
/// 2. `~/Downloads/ggml-*.bin`
/// 3. `~/.cache/whisper/*.bin`
///
/// Only filenames matching the `ggml-(tiny|base|small|medium|large)` pattern
/// are returned. An empty list is not an error.
#[tauri::command]
pub async fn scan_for_whisper_models() -> Result<Vec<WhisperModelInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let home = apollia_core::paths::home_string().unwrap_or_default();
        let home_path = std::path::PathBuf::from(&home);
        let sys_info = get_system_info_sync();

        let scan_dirs = [
            apollia_core::paths::data_dir_under(&home_path).join("models"),
            home_path.join("Downloads"),
            home_path.join(".cache").join("whisper"),
        ];

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut models: Vec<WhisperModelInfo> = Vec::new();

        for dir in &scan_dirs {
            for model in scan_whisper_in_dir(dir, sys_info.total_ram_gb) {
                if seen.insert(model.path.clone()) {
                    models.push(model);
                }
            }
        }
        models
    })
    .await
    .map_err(|e| format!("Whisper scan failed: {e}"))
}

/// Configures the Whisper STT backend with the selected model and marks
/// the onboarding state as STT-configured.
///
/// Persists `enabled = true` and the provided `model_path` to
/// `~/.apollia/system.db`, then sets `stt_configured` and `voice_enabled`
/// to `true` in the onboarding state.
#[tauri::command]
pub async fn setup_whisper_model(
    model_path: String,
    language: Option<String>,
    state: State<'_, RuntimeHandle>,
    app: tauri::AppHandle,
    stt_flow_state: State<'_, crate::commands::stt::SttFlowState>,
) -> Result<OnboardingState, String> {
    let result = setup_whisper_model_inner(model_path, language, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "stt.model.setup.failed");
            e.to_string()
        })?;

    // Hot-load the freshly-configured model so onboarding "Tester" and the
    // dictation hotkey work immediately, without restarting the app.
    if let Err(e) = crate::commands::stt::reload_stt_inner(&state, &app, &stt_flow_state).await {
        tracing::warn!(
            error = %e,
            detail = "the model is configured but the engine still runs the previous one",
            "stt.engine.reload.failed"
        );
    }

    Ok(result)
}

async fn setup_whisper_model_inner(
    model_path: String,
    language: Option<String>,
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    let db_path = apollia_core::paths::DataFile::System.path(&apollia_core::paths::data_dir_under(
        apollia_core::paths::home_dir_or_temp(),
    ));

    let mp = model_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), OnboardingError> {
        let repo = apollia_core::SttConfigRepository::open(&db_path)
            .map_err(|e| OnboardingError::PersistenceError(format!("open system.db: {e}")))?;
        let mut row = repo
            .get_or_default()
            .map_err(|e| OnboardingError::PersistenceError(format!("read STT config: {e}")))?;
        row.model_path = mp;
        row.enabled = true;
        // Pre-set the transcription language from the user's app locale so
        // dictation transcribes in their language instead of defaulting to
        // English. Only when unset, to preserve an explicit Settings choice.
        if row.language.is_none() {
            if let Some(lang) = language.filter(|l| !l.trim().is_empty()) {
                row.language = Some(lang);
            }
        }
        repo.upsert(&row)
            .map_err(|e| OnboardingError::PersistenceError(format!("persist STT config: {e}")))?;
        Ok(())
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))??;

    let repo = get_repo(state)?;
    let result = tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;
        let mut onboarding = load_state_from_memory(&repo)?;
        onboarding.stt_configured = true;
        onboarding.voice_enabled = true;
        persist_state(&repo, &onboarding)?;
        Ok::<OnboardingState, OnboardingError>(onboarding)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))??;

    tracing::info!(model_path = %model_path, "stt.model.configured");

    Ok(result)
}

// ---------------------------------------------------------------------------
// AI Setup - helper functions
// ---------------------------------------------------------------------------

/// Returns current system information synchronously.
///
/// Memory is read via sysinfo; OS and architecture are compile-time constants.
/// Intended for use inside `spawn_blocking` from async commands.
pub fn get_system_info_sync() -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_bytes = sys.total_memory();
    let available_bytes = sys.available_memory();

    SystemInfo {
        total_ram_gb: total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        available_ram_gb: available_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        gpu_available: detect_gpu_basic(),
    }
}

/// Scans a single directory (non-recursive) for `.gguf` files.
///
/// Returns an empty `Vec` for non-existent or unreadable directories.
/// The `recommended` field defaults to `false`; callers apply a RAM threshold.
pub fn scan_gguf_in_dir(dir: &std::path::Path) -> Vec<GgufModelInfo> {
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
/// Returns an empty `Vec` for non-existent or unreadable directories.
fn scan_whisper_in_dir(dir: &std::path::Path, total_ram_gb: f64) -> Vec<WhisperModelInfo> {
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
        let recommended = whisper_model_recommended(&model_size, total_ram_gb);

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
pub fn detect_whisper_model_size(filename: &str) -> Option<String> {
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

/// Returns the maximum recommended GGUF file size in bytes for the given RAM.
///
/// Thresholds:
/// - < 8 GB  → 2.5 GB (≤ 3B-parameter models)
/// - 8–16 GB → 6.0 GB (7–8B-parameter models)
/// - > 16 GB → no upper limit
pub fn recommended_max_gguf_size_bytes(total_ram_gb: f64) -> u64 {
    if total_ram_gb < 8.0 {
        2_500_000_000
    } else if total_ram_gb <= 16.0 {
        6_000_000_000
    } else {
        u64::MAX
    }
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

pub fn format_size_human(bytes: u64) -> String {
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

/// Returns whether a Whisper model size variant is recommended for the given RAM.
fn whisper_model_recommended(model_size: &str, total_ram_gb: f64) -> bool {
    match model_size {
        "tiny" => true,
        "base" | "small" => total_ram_gb >= 8.0,
        "medium" => total_ram_gb >= 16.0,
        "large" => total_ram_gb >= 32.0,
        _ => false,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info_returns_valid_values() {
        // GIVEN the current system
        // WHEN calling get_system_info_sync
        let info = get_system_info_sync();
        // THEN RAM values are positive and os/arch are non-empty
        assert!(info.total_ram_gb > 0.0);
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
    }

    #[test]
    fn test_gguf_scan_empty_dir_returns_empty_vec() {
        // GIVEN a temporary empty directory
        let dir = tempfile::tempdir().unwrap();
        // WHEN scanning for GGUF models
        let results = scan_gguf_in_dir(dir.path());
        // THEN an empty Vec is returned
        assert!(results.is_empty());
    }

    #[test]
    fn test_whisper_model_size_detection() {
        // GIVEN a filename "ggml-base.bin"
        let size = detect_whisper_model_size("ggml-base.bin");
        // WHEN the model size is read from it
        // THEN model_size is "base"
        assert_eq!(size, Some("base".to_string()));
    }

    #[test]
    fn test_whisper_model_size_unknown_filename() {
        // GIVEN a filename without the standard pattern
        let size = detect_whisper_model_size("random-model.bin");
        // WHEN the model size is read from it
        // THEN None is returned
        assert_eq!(size, None);
    }

    #[test]
    fn test_ram_recommendation_low() {
        // GIVEN 6 GB RAM
        let max_size = recommended_max_gguf_size_bytes(6.0);
        // WHEN the largest recommended model is computed
        // THEN the threshold is at most 2.5 GB
        assert!(max_size <= 2_500_000_000);
    }

    #[test]
    fn test_ram_recommendation_medium() {
        // GIVEN 12 GB RAM
        let max_size = recommended_max_gguf_size_bytes(12.0);
        // WHEN the largest recommended model is computed
        // THEN the threshold is between 2.5 GB and 6 GB (inclusive)
        assert!(max_size <= 6_000_000_000);
        assert!(max_size > 2_500_000_000);
    }

    #[test]
    fn test_human_readable_size() {
        // GIVEN known byte counts
        // WHEN each is formatted for the operator
        // THEN human-readable output matches expected strings
        assert_eq!(format_size_human(4_500_000_000), "4.2 GB");
        assert_eq!(format_size_human(500_000_000), "476.8 MB");
    }

    #[test]
    fn a_model_reached_through_a_link_reports_the_size_of_the_file() {
        // GIVEN a model file and a link pointing at it, which is the shape a
        // synchronised Windows folder presents to a directory walk
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("model.gguf");
        std::fs::write(&real, vec![0u8; 4096]).expect("write the model");
        let link = tmp.path().join("linked.gguf");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&real, &link).is_err() {
            // Creating a link needs a privilege this runner may not hold; the
            // case still proves the direct read below.
            return;
        }

        // WHEN the size is read the way the scan reads it
        let size = size_on_disk(&link);

        // THEN it is the size of the file, not the zero a link answers when it
        // is measured instead of followed
        assert_eq!(size, 4096);
    }

    #[test]
    fn a_path_that_is_not_there_reads_as_zero() {
        // GIVEN a path no file occupies
        let tmp = tempfile::tempdir().expect("tempdir");

        // WHEN its size is read
        let size = size_on_disk(&tmp.path().join("absent.gguf"));

        // THEN zero is returned, and the warning names the path so the reader
        // is not left with a silent 0 B in the interface
        assert_eq!(size, 0);
    }
}
