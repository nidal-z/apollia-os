//! What the bus hears of a model being loaded.
//!
//! Launching an engine process on a model takes seconds, tens of them for a
//! large one, and until now nothing said so: the events a router publishes when
//! it reads its configuration ([`RuntimeEvent::LlmModelLoading`] then
//! [`RuntimeEvent::LlmModelReady`]) are over before any process starts. A
//! surface that wants to say "this model is loading" had no fact to read.
//!
//! The supervisor publishes the same three events around the launch itself,
//! under the wire name of the engine: loading when the process is started,
//! ready once it answers its health check, failed when it exits or never
//! answers. The model is named by its path when it starts loading and by the
//! stem of its file once ready, as the two events define their fields.

use std::path::Path;

use apollia_core::RuntimeEvent;

use super::{LlamaServerError, LlamaServerSupervisor};
use crate::engine_store::Engine;

impl LlamaServerSupervisor {
    /// Says a process is being started on `model_path`.
    pub(super) fn announce_loading(&self, model_path: &str) {
        self.announce(RuntimeEvent::LlmModelLoading {
            backend: Engine::LlamaServer.as_str().to_owned(),
            model_path: model_path.to_owned(),
        });
    }

    /// Says the process serving `model_path` answers.
    pub(super) fn announce_ready(&self, model_path: &str) {
        self.announce(RuntimeEvent::LlmModelReady {
            backend: Engine::LlamaServer.as_str().to_owned(),
            model_id: model_stem(model_path),
        });
    }

    /// Says the process started on `model_path` did not come up.
    pub(super) fn announce_failed(&self, model_path: &str, error: &LlamaServerError) {
        self.announce(RuntimeEvent::LlmModelFailed {
            backend: Engine::LlamaServer.as_str().to_owned(),
            reason: format!("{}: {error}", model_stem(model_path)),
        });
    }

    fn announce(&self, event: RuntimeEvent) {
        if let Some(events) = &self.events {
            // No subscriber is not a failure: the load goes on unheard.
            let _ = events.send(event);
        }
    }
}

/// The name of a model file without its extension, the form
/// [`RuntimeEvent::LlmModelReady`] names a local model by.
fn model_stem(model_path: &str) -> String {
    Path::new(model_path).file_stem().map_or_else(
        || model_path.to_owned(),
        |s| s.to_string_lossy().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use apollia_core::RuntimeEvent;
    use tokio::sync::broadcast;

    use super::model_stem;
    use crate::llama_server::load_settings::LoadSettings;
    use crate::llama_server::test_support::supervisor_with_ceiling;
    use crate::llama_server::LlamaServerConfig;

    #[tokio::test]
    async fn test_a_launch_that_fails_is_announced_as_loading_then_failed() {
        // GIVEN a supervisor whose binary does not exist, heard on a bus
        let (events, mut bus) = broadcast::channel(16);
        let mut supervisor = supervisor_with_ceiling(1);
        supervisor.events = Some(events);
        let config = LlamaServerConfig {
            model_path: "/models/Vision-30B-Q4_K_M/Vision-30B-Q4_K_M.gguf".to_owned(),
            ..LlamaServerConfig::default()
        };

        // WHEN a model is launched
        let launched = supervisor
            .spawn_configured(config, LoadSettings::default(), false, None)
            .await;

        // THEN the launch fails, and the bus heard that the model was loading,
        // then that it did not come up, named by the stem of its file
        assert!(launched.is_err());
        match bus.try_recv() {
            Ok(RuntimeEvent::LlmModelLoading {
                backend,
                model_path,
            }) => {
                assert_eq!(backend, "llama-server");
                assert_eq!(
                    model_path,
                    "/models/Vision-30B-Q4_K_M/Vision-30B-Q4_K_M.gguf"
                );
            }
            other => panic!("expected the loading event, got {other:?}"),
        }
        match bus.try_recv() {
            Ok(RuntimeEvent::LlmModelFailed { backend, reason }) => {
                assert_eq!(backend, "llama-server");
                assert!(reason.starts_with("Vision-30B-Q4_K_M: "), "{reason}");
            }
            other => panic!("expected the failed event, got {other:?}"),
        }
    }

    #[test]
    fn test_a_model_is_named_by_the_stem_of_its_file() {
        // GIVEN the path of a model in a folder of its own, and a bare name
        // WHEN each is named for the ready event
        // THEN the first loses its folders and its extension, the second is
        // kept as it is
        assert_eq!(
            model_stem("/models/Vision-30B-Q4_K_M/Vision-30B-Q4_K_M.gguf"),
            "Vision-30B-Q4_K_M"
        );
        assert_eq!(model_stem("remote-model"), "remote-model");
    }
}
