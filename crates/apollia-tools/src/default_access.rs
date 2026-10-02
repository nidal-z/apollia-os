//! What an agent created without code is granted when nobody chose for it.
//!
//! An agent installed from a package is granted what its package declares,
//! accepted at installation. An agent created in the interface, the Apollia
//! agent among them, is granted the reading family: every tool of
//! [`ActionFamily::Read`] active on the machine, native tools and MCP tools
//! alike. Any other access is a choice made on its card, an override of the
//! access field, which then replaces the default whole.
//!
//! "Active" is what the tool registry holds: a native tool compiled in, an
//! MCP tool of a connected server. The tools switched off by the operator are
//! taken out by the caller, which knows the disabled list.

use crate::agent_definition::{AgentOrigin, OverrideField, ResolvedAgent};
use crate::descriptor::ActionFamily;
use crate::registry::ToolRegistryHandle;

/// The tools an agent may call, as its resolved definition states them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAccess {
    /// The tools listed by its package or chosen on its card.
    Listed(Vec<String>),
    /// The reading family active on the machine, read when the run starts.
    ReadingFamily,
}

impl AgentAccess {
    /// The access of `agent`: the reading family when it reads by default
    /// ([`reads_by_default`]), its listed access otherwise.
    pub fn of(agent: &ResolvedAgent) -> Self {
        if reads_by_default(agent) {
            AgentAccess::ReadingFamily
        } else {
            AgentAccess::Listed(agent.access.clone())
        }
    }

    /// The tool names this access stands for, the reading family read from
    /// `registry` now.
    pub async fn tools(self, registry: &ToolRegistryHandle) -> Vec<String> {
        match self {
            AgentAccess::Listed(tools) => tools,
            AgentAccess::ReadingFamily => reading_family(registry, &[]).await,
        }
    }
}

/// Whether `agent` is granted the reading family: an agent created without
/// code whose access neither its definition nor an override names.
pub fn reads_by_default(agent: &ResolvedAgent) -> bool {
    agent.origin == AgentOrigin::Interface
        && !agent.overridden.contains(&OverrideField::Access)
        && agent
            .definition
            .pointer(OverrideField::Access.pointer())
            .is_none()
}

/// The tools of the reading family `registry` holds, `disabled` left out, by
/// name. A registry that cannot be listed yields none: a default never grants
/// what could not be checked.
pub async fn reading_family(registry: &ToolRegistryHandle, disabled: &[String]) -> Vec<String> {
    let descriptors = match registry.list().await {
        Ok(descriptors) => descriptors,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "the default access is empty",
                "agent.access.reading_family.unavailable"
            );
            return Vec::new();
        }
    };
    let mut names: Vec<String> = descriptors
        .into_iter()
        .filter(|d| d.family == ActionFamily::Read)
        .map(|d| d.name)
        .filter(|name| !disabled.contains(name))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `agent` with its default access written in when it reads by default, so a
/// card shows what the agent is granted. The `overridden` list is left as it
/// is: the access is still the default, not the user's.
pub async fn with_default_access(
    mut agent: ResolvedAgent,
    registry: &ToolRegistryHandle,
) -> ResolvedAgent {
    if reads_by_default(&agent) {
        agent.access = reading_family(registry, &[]).await;
    }
    agent
}

#[cfg(test)]
mod tests;
