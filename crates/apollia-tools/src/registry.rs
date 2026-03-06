use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tracing::instrument;

use crate::descriptor::{ToolDescriptor, ToolDescriptorError};

/// Internal messages processed by the [`ToolRegistry`] actor.
enum ToolRegistryMessage {
    Register {
        descriptor: ToolDescriptor,
        reply: oneshot::Sender<Result<(), ToolRegistryError>>,
    },
    Get {
        name: String,
        reply: oneshot::Sender<Option<ToolDescriptor>>,
    },
    List {
        reply: oneshot::Sender<Vec<ToolDescriptor>>,
    },
    Shutdown,
}

/// Internal actor that owns the tool catalogue.
///
/// Never exposed directly — callers interact through [`ToolRegistryHandle`].
struct ToolRegistry {
    catalogue: HashMap<String, ToolDescriptor>,
    receiver: mpsc::Receiver<ToolRegistryMessage>,
}

impl ToolRegistry {
    fn new(receiver: mpsc::Receiver<ToolRegistryMessage>) -> Self {
        Self {
            catalogue: HashMap::new(),
            receiver,
        }
    }

    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                ToolRegistryMessage::Register { descriptor, reply } => {
                    let result = self.handle_register(descriptor);
                    let _ = reply.send(result);
                }
                ToolRegistryMessage::Get { name, reply } => {
                    let result = self.catalogue.get(&name).cloned();
                    let _ = reply.send(result);
                }
                ToolRegistryMessage::List { reply } => {
                    let list: Vec<ToolDescriptor> = self.catalogue.values().cloned().collect();
                    let _ = reply.send(list);
                }
                ToolRegistryMessage::Shutdown => {
                    tracing::info!("ToolRegistry shutting down");
                    break;
                }
            }
        }
    }

    fn handle_register(&mut self, descriptor: ToolDescriptor) -> Result<(), ToolRegistryError> {
        descriptor
            .validate()
            .map_err(ToolRegistryError::InvalidDescriptor)?;

        if self.catalogue.contains_key(&descriptor.name) {
            return Err(ToolRegistryError::AlreadyRegistered(descriptor.name));
        }

        tracing::info!(tool = %descriptor.name, version = %descriptor.version, "tool registered");
        self.catalogue.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }
}

/// Clonable, thread-safe handle to the [`ToolRegistry`] actor.
///
/// This is the only type exposed outside this module. Obtain one via [`ToolRegistryHandle::start`].
#[derive(Clone)]
pub struct ToolRegistryHandle {
    sender: mpsc::Sender<ToolRegistryMessage>,
}

impl ToolRegistryHandle {
    /// Spawns the [`ToolRegistry`] actor and returns its handle.
    ///
    /// The actor runs as a background Tokio task until [`shutdown`](Self::shutdown) is called
    /// or all handles are dropped and the channel closes.
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = ToolRegistry::new(receiver);
        tokio::spawn(actor.run());
        Self { sender }
    }

    /// Registers a tool in the catalogue.
    ///
    /// Validates the descriptor before insertion. Returns an error if the descriptor
    /// is invalid or if a tool with the same name is already registered.
    ///
    /// # Errors
    ///
    /// - [`ToolRegistryError::InvalidDescriptor`] — descriptor fails validation.
    /// - [`ToolRegistryError::AlreadyRegistered`] — a tool with the same name exists.
    /// - [`ToolRegistryError::ActorGone`] — the actor task has terminated.
    #[instrument(skip(self, descriptor), fields(tool = %descriptor.name))]
    pub async fn register(&self, descriptor: ToolDescriptor) -> Result<(), ToolRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ToolRegistryMessage::Register {
                descriptor,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ToolRegistryError::ActorGone)?;
        reply_rx.await.map_err(|_| ToolRegistryError::ActorGone)?
    }

    /// Returns the descriptor for the named tool, or `None` if it is not registered.
    ///
    /// # Errors
    ///
    /// - [`ToolRegistryError::ActorGone`] — the actor task has terminated.
    pub async fn get(&self, name: &str) -> Result<Option<ToolDescriptor>, ToolRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ToolRegistryMessage::Get {
                name: name.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| ToolRegistryError::ActorGone)?;
        reply_rx.await.map_err(|_| ToolRegistryError::ActorGone)
    }

    /// Returns all registered tool descriptors.
    ///
    /// Order of the returned slice is unspecified.
    ///
    /// # Errors
    ///
    /// - [`ToolRegistryError::ActorGone`] — the actor task has terminated.
    pub async fn list(&self) -> Result<Vec<ToolDescriptor>, ToolRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ToolRegistryMessage::List { reply: reply_tx })
            .await
            .map_err(|_| ToolRegistryError::ActorGone)?;
        reply_rx.await.map_err(|_| ToolRegistryError::ActorGone)
    }

    /// Sends the shutdown signal and waits for the actor to stop.
    pub async fn shutdown(self) {
        let _ = self.sender.send(ToolRegistryMessage::Shutdown).await;
        // Drop sender — actor loop exits when the channel drains and closes.
    }
}

/// Errors returned by [`ToolRegistryHandle`] operations.
#[derive(Debug, thiserror::Error)]
pub enum ToolRegistryError {
    /// A tool with this name is already present in the catalogue.
    #[error("tool already registered: {0}")]
    AlreadyRegistered(String),
    /// The descriptor failed internal validation.
    #[error("invalid tool descriptor: {0}")]
    InvalidDescriptor(#[from] ToolDescriptorError),
    /// The registry actor has terminated unexpectedly.
    #[error("tool registry actor is gone")]
    ActorGone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SandboxProfile;

    fn bash_executor_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands in sandbox".to_string(),
            kind: crate::descriptor::ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
        }
    }

    #[tokio::test]
    async fn test_ac1_register_and_get_tool() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let descriptor = bash_executor_descriptor();
        // WHEN
        registry
            .register(descriptor.clone())
            .await
            .expect("register failed");
        let result = registry.get("bash_executor").await.expect("get failed");
        // THEN
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "bash_executor");
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac2_invalid_descriptor_rejected() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let mut descriptor = bash_executor_descriptor();
        descriptor.name = "".to_string();
        // WHEN
        let result = registry.register(descriptor).await;
        // THEN
        assert!(matches!(
            result,
            Err(ToolRegistryError::InvalidDescriptor(_))
        ));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac3_duplicate_rejected() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        registry
            .register(bash_executor_descriptor())
            .await
            .expect("first register failed");
        // WHEN
        let result = registry.register(bash_executor_descriptor()).await;
        // THEN
        assert!(matches!(
            result,
            Err(ToolRegistryError::AlreadyRegistered(_))
        ));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac4_list_returns_all_tools() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        registry.register(bash_executor_descriptor()).await.unwrap();
        // WHEN
        let list = registry.list().await.expect("list failed");
        // THEN
        assert_eq!(list.len(), 1);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac5_get_unknown_tool_returns_none() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        // WHEN
        let result = registry.get("inexistant").await.expect("get failed");
        // THEN
        assert!(result.is_none());
        registry.shutdown().await;
    }
}
