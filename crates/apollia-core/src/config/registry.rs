use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// RegistryConfig
// ─────────────────────────────────────────────

/// Community pipeline registry configuration (`[registry]` section in `apollia.toml`).
///
/// Holds the URL of the public Git repository from which `apollia pipeline
/// install` downloads templates. GitHub URLs (`https://github.com/org/repo`)
/// are converted automatically to raw-content URLs by the `PipelineRegistry`.
/// Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Git repository URL of the community pipeline registry.
    ///
    /// GitHub format: `https://github.com/org/repo`.
    /// The `PipelineRegistry` converts this URL automatically to a raw-content
    /// URL (`raw.githubusercontent.com`).
    /// Default: `"https://github.com/apollia-os/pipelines"`.
    #[serde(default = "default_pipeline_registry_url")]
    pub pipeline_registry_url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            pipeline_registry_url: default_pipeline_registry_url(),
        }
    }
}

fn default_pipeline_registry_url() -> String {
    "https://github.com/apollia-os/pipelines".to_owned()
}
