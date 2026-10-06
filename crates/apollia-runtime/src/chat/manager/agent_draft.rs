//! What the conversation that creates an agent may propose.

use apollia_tools::ToolRegistryHandle;

use super::ChatToolsConfig;
use crate::chat::agent_draft_tool::{DraftOffer, OfferedSource};

/// The offer of a draft, read when a message is sent: the tools of
/// `registry` without the `disabled` ones, the default access of `cfg`, the
/// sources of the registry of `cfg`, and `models`. A registry of sources that is absent or cannot be
/// listed leaves the profile and the memory, which every agent may be given.
pub(in crate::chat::manager) async fn offer(
    registry: &ToolRegistryHandle,
    disabled: &[String],
    cfg: Option<&ChatToolsConfig>,
    models: Vec<String>,
) -> DraftOffer {
    let sources = match cfg.and_then(|cfg| cfg.source_registry.as_ref()) {
        Some(sources) => match sources.list().await {
            Ok(listed) => listed
                .into_iter()
                .map(|source| OfferedSource {
                    id: source.id,
                    label: source.label,
                })
                .collect(),
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "the draft proposes the profile and the memory only",
                    "chat.agent_draft.sources.unavailable"
                );
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    DraftOffer::read(
        registry,
        disabled,
        &super::default_access_of(cfg),
        sources,
        models,
    )
    .await
}
