//! Which `llama-server` binary a launch starts.
//!
//! The binary is resolved again at every launch, from the `[engines] gguf`
//! choice the settings hold at that moment, so a change of engine applies
//! when a model is next loaded, without a restart of the runtime.

use std::path::{Path, PathBuf};

use apollia_core::config::PolicySettings;
use tokio::process::Child;

use super::locate::{locate_for_launch, locate_llama_server_binary, user_store_root};
use super::{LlamaServerError, LlamaServerSupervisor};
use crate::engine_store::EngineChoices;

/// Where the supervisor takes the binary of each launch from.
#[derive(Debug, Clone)]
pub(super) enum LaunchBinary {
    /// Resolved at every launch from the settings, against the engine store
    /// under `store_root`.
    FromSettings { store_root: Option<PathBuf> },
    /// This path at every launch, for the tests that bring their own engine.
    #[cfg(test)]
    Fixed(PathBuf),
}

impl LaunchBinary {
    /// Resolution from the settings, against the user's engine store, once
    /// a binary is found for the choice the settings hold now: an install
    /// with no engine at all is reported at construction, as before.
    ///
    /// # Errors
    ///
    /// [`LlamaServerError::BinaryNotFound`] when no binary is in reach.
    pub(super) fn located(
        config_file: Option<&std::path::Path>,
        imposed: &PolicySettings,
    ) -> Result<Self, LlamaServerError> {
        locate_llama_server_binary(EngineChoices::read(config_file, imposed).gguf)?;
        Ok(Self::FromSettings {
            store_root: user_store_root(),
        })
    }
}

impl LlamaServerSupervisor {
    /// The binary the next launch starts, the engine choice of the settings
    /// read now. A variant chosen and since removed falls back to the
    /// automatic choice, which the engine store reports.
    ///
    /// # Errors
    ///
    /// [`LlamaServerError::BinaryNotFound`] when no binary is in reach.
    pub(super) fn launch_binary(&self) -> Result<PathBuf, LlamaServerError> {
        match &self.binary {
            #[cfg(test)]
            LaunchBinary::Fixed(path) => Ok(path.clone()),
            LaunchBinary::FromSettings { store_root } => {
                let choices = EngineChoices::read(self.config_file.as_deref(), &self.imposed);
                locate_for_launch(store_root.as_deref(), choices.gguf)
            }
        }
    }

    /// Start `bin` with `args` and wait until it answers on `port`.
    pub(super) async fn start_and_wait(
        &self,
        bin: &Path,
        args: &[String],
        port: u16,
    ) -> Result<Child, LlamaServerError> {
        let mut child = self.spawn_child(bin, args)?;
        self.wait_until_healthy(&mut child, port).await?;
        Ok(child)
    }
}

#[cfg(test)]
mod tests;
