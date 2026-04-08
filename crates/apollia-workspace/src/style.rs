//! Détection automatique des conventions de code via échantillonnage + LLM léger.
//!
//! [`StyleDetector`] identifie l'extension dominante du projet, échantillonne
//! un nombre configurable de fichiers source, puis interroge le backend LLM
//! par défaut pour en extraire les conventions de code en bullet points.
//!
//! L'appel LLM est borné par un timeout configurable (`style_detection_timeout_ms`).
//! Toute erreur (LLM indisponible, timeout, répertoire vide) produit `None`
//! sans propager d'erreur vers l'appelant.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_llm::{ChatMessage, CompletionRequest, LlmRouter};

use crate::config::StyleProviderConfig;

/// Extensions exclues de la détection du langage dominant.
///
/// On veut uniquement des fichiers de code source — pas de configuration,
/// documentation, ni métadonnées de build.
const EXCLUDED_EXTENSIONS: &[&str] = &[
    "md", "toml", "json", "lock", "yaml", "yml", "txt", "log", "xml", "csv",
];

/// Répertoires ignorés lors du parcours récursif.
const SKIPPED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    "__pycache__",
];

/// Détecte automatiquement les conventions de code du projet via LLM léger.
///
/// Non-bloquant : l'appel LLM est borné par `config.style_detection_timeout_ms`.
/// Retourne `None` si le répertoire est vide, si le LLM est indisponible,
/// ou si le timeout est dépassé.
pub struct StyleDetector;

impl StyleDetector {
    /// Détecte les conventions de code du projet via échantillonnage + LLM léger.
    ///
    /// Étapes :
    /// 1. Identifie l'extension dominante sur 2 niveaux de répertoires.
    /// 2. Échantillonne jusqu'à `config.style_sample_count` fichiers de cette extension.
    /// 3. Lit `config.style_sample_lines_per_file` lignes par fichier (head + tail).
    /// 4. Appelle le backend LLM par défaut avec timeout `config.style_detection_timeout_ms`.
    ///
    /// Retourne `None` si : répertoire sans fichiers source, LLM timeout, ou LLM indisponible.
    pub async fn detect(
        cwd: &Path,
        llm_router: &LlmRouter,
        config: &StyleProviderConfig,
    ) -> Option<String> {
        let ext = Self::dominant_extension(cwd).await?;

        let files = Self::sample_files(cwd, &ext, config.sample_count).await;
        if files.is_empty() {
            return None;
        }

        let samples = Self::collect_samples(&files, config.sample_lines_per_file).await;

        let prompt = format!(
            "Extract the coding style conventions from these {} code samples in 5 bullet points max. \
             Focus on: naming conventions, error handling patterns, comment style, struct organization.\n\n{}",
            ext, samples
        );

        let backend = match llm_router.route_fast() {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(error = %e, "route_fast unavailable for style detection");
                return None;
            }
        };
        let req = CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: Some(512),
            ..Default::default()
        };

        let result = tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            backend.complete(req),
        )
        .await;

        match result {
            Ok(Ok(resp)) => {
                let text = resp.content.trim().to_string();
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "style detection LLM error");
                None
            }
            Err(_elapsed) => {
                tracing::debug!("style detection timeout");
                None
            }
        }
    }

    /// Retourne l'extension de fichier la plus fréquente dans le répertoire (récursif, 2 niveaux).
    ///
    /// Exclut les extensions non-source (`.md`, `.toml`, `.json`, `.lock`, `.yaml`…)
    /// et les répertoires système (`target`, `node_modules`…).
    /// Retourne `None` si aucun fichier source n'est trouvé.
    pub async fn dominant_extension(cwd: &Path) -> Option<String> {
        let cwd = cwd.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut counts: HashMap<String, usize> = HashMap::new();
            collect_extensions_recursive(&cwd, 0, 2, &mut counts);
            counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(ext, _)| ext)
        })
        .await
        .ok()
        .flatten()
    }

    /// Retourne jusqu'à `max_count` fichiers de l'extension donnée avec distribution régulière.
    ///
    /// Si le nombre de fichiers trouvés est inférieur ou égal à `max_count`,
    /// tous les fichiers sont retournés. Sinon, une sélection espacée régulièrement
    /// garantit une représentation de l'ensemble du projet.
    async fn sample_files(cwd: &Path, ext: &str, max_count: usize) -> Vec<PathBuf> {
        let cwd = cwd.to_path_buf();
        let ext = ext.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut all: Vec<PathBuf> = Vec::new();
            collect_files_recursive(&cwd, &ext, 0, 2, &mut all);
            all.sort(); // ordre déterministe
            if all.len() <= max_count {
                return all;
            }
            let step = all.len() / max_count;
            (0..max_count).map(|i| all[i * step].clone()).collect()
        })
        .await
        .unwrap_or_default()
    }

    /// Lit jusqu'à `lines_per_file` lignes par fichier (head + tail 50/50).
    ///
    /// Chaque fichier est préfixé par son nom pour contextualiser les extraits.
    /// Si le fichier ne dépasse pas `lines_per_file` lignes, il est inclus en entier.
    /// Les fichiers illisibles sont silencieusement ignorés.
    async fn collect_samples(files: &[PathBuf], lines_per_file: usize) -> String {
        let half = (lines_per_file / 2).max(1);
        let mut parts = Vec::with_capacity(files.len());

        for path in files {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let lines: Vec<&str> = content.lines().collect();
                let sample = if lines.len() <= lines_per_file {
                    content.as_str().to_owned()
                } else {
                    let head = lines[..half].join("\n");
                    let tail = lines[lines.len() - half..].join("\n");
                    format!(
                        "{}\n... [{} lines omitted] ...\n{}",
                        head,
                        lines.len() - lines_per_file,
                        tail
                    )
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                parts.push(format!("=== {name} ===\n{sample}"));
            }
        }

        parts.join("\n\n")
    }
}

/// Parcourt `dir` récursivement jusqu'à `max_depth` niveaux et compte les extensions source.
fn collect_extensions_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    counts: &mut HashMap<String, usize>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect_extensions_recursive(&path, depth + 1, max_depth, counts);
            }
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if !EXCLUDED_EXTENSIONS.contains(&ext_lower.as_str()) {
                *counts.entry(ext_lower).or_insert(0) += 1;
            }
        }
    }
}

/// Parcourt `dir` récursivement jusqu'à `max_depth` niveaux et collecte les fichiers de `ext`.
fn collect_files_recursive(
    dir: &Path,
    ext: &str,
    depth: usize,
    max_depth: usize,
    files: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect_files_recursive(&path, ext, depth + 1, max_depth, files);
            }
        } else {
            let matches = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase() == ext)
                .unwrap_or(false);
            if matches {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dominant_extension_rust_project() {
        // GIVEN : répertoire avec *.rs (5 fichiers) et 1 README.md
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            tokio::fs::write(dir.path().join(format!("file{i}.rs")), "fn main() {}")
                .await
                .unwrap();
        }
        tokio::fs::write(dir.path().join("README.md"), "# readme")
            .await
            .unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert_eq!(ext, Some("rs".to_string()));
    }

    #[tokio::test]
    async fn test_dominant_extension_empty_dir() {
        // GIVEN : répertoire vide
        let dir = tempfile::tempdir().unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert!(ext.is_none());
    }

    #[tokio::test]
    async fn test_dominant_extension_excludes_non_source() {
        // GIVEN : répertoire avec uniquement des fichiers exclus
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("config.toml"), "[x]")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("data.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("README.md"), "# readme")
            .await
            .unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert!(ext.is_none());
    }

    #[tokio::test]
    async fn test_sample_files_respects_max_count() {
        // GIVEN : répertoire avec 10 fichiers .rs
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            tokio::fs::write(dir.path().join(format!("f{i}.rs")), "fn f() {}")
                .await
                .unwrap();
        }
        // WHEN : max_count = 3
        let files = StyleDetector::sample_files(dir.path(), "rs", 3).await;
        // THEN
        assert!(
            files.len() <= 3,
            "expected at most 3 files, got {}",
            files.len()
        );
    }

    #[tokio::test]
    async fn test_sample_files_fewer_than_max() {
        // GIVEN : répertoire avec 2 fichiers .rs, max_count = 7
        let dir = tempfile::tempdir().unwrap();
        for i in 0..2 {
            tokio::fs::write(dir.path().join(format!("f{i}.rs")), "fn f() {}")
                .await
                .unwrap();
        }
        // WHEN
        let files = StyleDetector::sample_files(dir.path(), "rs", 7).await;
        // THEN : tous les fichiers retournés
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_collect_samples_truncates_long_files() {
        // GIVEN : fichier de 500 lignes
        let dir = tempfile::tempdir().unwrap();
        let content: String = (0..500).map(|i| format!("line {i}\n")).collect();
        let path = dir.path().join("big.rs");
        tokio::fs::write(&path, &content).await.unwrap();
        // WHEN : lines_per_file = 20
        let result = StyleDetector::collect_samples(&[path], 20).await;
        // THEN : contient le marqueur de troncature
        assert!(
            result.contains("lines omitted"),
            "expected truncation marker"
        );
    }

    #[tokio::test]
    async fn test_collect_samples_short_file_not_truncated() {
        // GIVEN : fichier de 5 lignes
        let dir = tempfile::tempdir().unwrap();
        let content = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}";
        let path = dir.path().join("short.rs");
        tokio::fs::write(&path, content).await.unwrap();
        // WHEN
        let result = StyleDetector::collect_samples(&[path], 20).await;
        // THEN : pas de marqueur de troncature
        assert!(!result.contains("lines omitted"));
        assert!(result.contains("fn a()"));
    }
}
