//! [`ScriptProvider`] — provider de contexte via script shell ou binaire.
//!
//! Le script est exécuté dans `cwd` et doit écrire son résultat JSON sur stdout.
//! Format attendu :
//! ```json
//! { "sections": [{"title": "...", "content": "..."}], "errors": [] }
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};
use serde::Deserialize;

/// Provider de contexte qui exécute un script et parse son output JSON.
///
/// Fail-silent : si le script est absent, échoue ou produit un JSON invalide,
/// retourne un [`WorkspaceSlice::with_error`] sans panic.
pub struct ScriptProvider {
    /// Nom unique du provider.
    name: String,
    /// Chemin vers le script ou binaire à exécuter.
    path: PathBuf,
    /// Timeout d'exécution en millisecondes.
    timeout_ms: u64,
    /// Priorité d'affichage dans le system prompt.
    priority: u8,
}

/// Output JSON attendu du script.
#[derive(Deserialize)]
struct ScriptOutput {
    #[serde(default)]
    sections: Vec<ScriptSection>,
    #[serde(default)]
    errors: Vec<String>,
}

/// Section JSON produite par un script.
#[derive(Deserialize)]
struct ScriptSection {
    title: String,
    content: String,
}

impl ScriptProvider {
    /// Construit un provider script avec les paramètres fournis.
    ///
    /// `timeout_ms` — timeout d'exécution du script. `0` désactive le timeout.
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        timeout_ms: u64,
        priority: u8,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            timeout_ms,
            priority,
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for ScriptProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn description(&self) -> &str {
        "Script shell ou binaire produisant du JSON sur stdout"
    }

    /// Exécute le script et parse son output JSON.
    ///
    /// Retourne [`WorkspaceSlice::with_error`] si le script échoue,
    /// dépasse le timeout, ou produit un JSON invalide.
    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let source = self.name.clone();
        let timeout = if self.timeout_ms > 0 {
            Duration::from_millis(self.timeout_ms)
        } else {
            Duration::from_secs(30)
        };

        let output_result = tokio::time::timeout(
            timeout,
            tokio::process::Command::new(&self.path)
                .current_dir(cwd)
                .output(),
        )
        .await;

        let output = match output_result {
            Err(_elapsed) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!(
                        "script '{}' timed out after {}ms",
                        self.path.display(),
                        self.timeout_ms
                    ),
                );
            }
            Ok(Err(e)) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!("script '{}' failed to start: {}", self.path.display(), e),
                );
            }
            Ok(Ok(o)) => o,
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return WorkspaceSlice::with_error(
                &source,
                format!(
                    "script '{}' exited with {}: {}",
                    self.path.display(),
                    output.status,
                    stderr.trim()
                ),
            );
        }

        let stdout = match String::from_utf8(output.stdout) {
            Ok(s) => s,
            Err(e) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!(
                        "script '{}' output is not valid UTF-8: {}",
                        self.path.display(),
                        e
                    ),
                );
            }
        };

        let parsed: ScriptOutput = match serde_json::from_str(&stdout) {
            Ok(p) => p,
            Err(e) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!("script '{}' JSON parse error: {}", self.path.display(), e),
                );
            }
        };

        let sections: Vec<WorkspaceSection> = parsed
            .sections
            .into_iter()
            .map(|s| WorkspaceSection {
                title: s.title,
                content: s.content,
                source: source.clone(),
            })
            .collect();

        WorkspaceSlice {
            source: source.clone(),
            sections,
            errors: parsed.errors,
            collected_at: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn script_provider_missing_binary_returns_error_slice() {
        // GIVEN un provider avec un chemin de script inexistant
        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = ScriptProvider::new("test", "/nonexistent/script.sh", 500, 50);
        // WHEN
        let slice = provider.collect(tmp.path()).await;
        // THEN — fail-silent : pas de panic, erreur dans slice.errors
        assert!(slice.is_empty());
        assert!(!slice.errors.is_empty());
    }
}
