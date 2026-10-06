//! What an agent created without code is granted when nobody chose for it.
//!
//! An agent installed from a package is granted what its package declares,
//! accepted at installation. An agent created in the interface, the Apollia
//! agent among them, is granted the default access: whole families of tools
//! active on the machine, native tools and MCP tools alike, then tools named
//! one by one ([`DefaultAccess`], the `[tools.new_agent]` setting). Out of the
//! box that is the reading family, every tool of [`ActionFamily::Read`], and
//! nothing else. Any other access is a choice made on its card, an override
//! of the access field, which then replaces the default whole.
//!
//! "Active" is what the tool registry holds: a native tool compiled in, an
//! MCP tool of a connected server. The tools switched off by the operator are
//! taken out by the caller, which knows the disabled list.

use apollia_core::config::NewAgentConfig;

use crate::agent_definition::{AgentOrigin, OverrideField, ResolvedAgent};
use crate::descriptor::ActionFamily;
use crate::registry::ToolRegistryHandle;

/// The five families a default may grant whole. A tool nobody classified and
/// the tool that acts on the conversation alone are not families to grant.
const GRANTABLE_FAMILIES: [ActionFamily; 5] = [
    ActionFamily::Read,
    ActionFamily::ReadPersonal,
    ActionFamily::WriteOrDelete,
    ActionFamily::SendExternal,
    ActionFamily::ExecuteCode,
];

/// The default access as the setting states it: families granted whole, then
/// tools granted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultAccess {
    /// Families whose every active tool is granted.
    pub families: Vec<ActionFamily>,
    /// Tools granted by name, when the machine holds them.
    pub tools: Vec<String>,
}

impl Default for DefaultAccess {
    /// The reading family and nothing else.
    fn default() -> Self {
        Self {
            families: vec![ActionFamily::Read],
            tools: Vec::new(),
        }
    }
}

impl DefaultAccess {
    /// Reads the `[tools.new_agent]` setting. A family name the product does
    /// not know grants nothing and is logged: a typo never widens a default.
    pub fn from_config(config: &NewAgentConfig) -> Self {
        let mut families = Vec::with_capacity(config.families.len());
        for name in &config.families {
            match GRANTABLE_FAMILIES.iter().find(|f| f.as_str() == name) {
                Some(family) if !families.contains(family) => families.push(*family),
                Some(_) => {}
                None => tracing::event!(
                    tracing::Level::WARN,
                    family = %name,
                    detail = "not a family of tools; it grants nothing",
                    "agent.access.default.unknown_family"
                ),
            }
        }
        Self {
            families,
            tools: config.access.clone(),
        }
    }
}

/// The tools an agent may call, as its resolved definition states them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAccess {
    /// The tools listed by its package or chosen on its card.
    Listed(Vec<String>),
    /// The default access, read against the machine when the run starts.
    Default,
}

impl AgentAccess {
    /// The access of `agent`: the default access when nobody chose for it
    /// ([`reads_by_default`]), its listed access otherwise.
    pub fn of(agent: &ResolvedAgent) -> Self {
        if reads_by_default(agent) {
            AgentAccess::Default
        } else {
            AgentAccess::Listed(agent.access.clone())
        }
    }

    /// The tool names this access stands for, the default access read from
    /// `registry` now as `default` states it.
    pub async fn tools(
        self,
        registry: &ToolRegistryHandle,
        default: &DefaultAccess,
    ) -> Vec<String> {
        match self {
            AgentAccess::Listed(tools) => tools,
            AgentAccess::Default => default_access(registry, default, &[]).await,
        }
    }
}

/// Whether `agent` is granted the default access: an agent created without
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
/// name: the default access out of the box.
pub async fn reading_family(registry: &ToolRegistryHandle, disabled: &[String]) -> Vec<String> {
    default_access(registry, &DefaultAccess::default(), disabled).await
}

/// The tools `default` grants among those `registry` holds, `disabled` left
/// out, by name: every tool of a granted family, then the tools named. A
/// registry that cannot be listed yields none: a default never grants what
/// could not be checked.
///
/// `code_context` is left out too: a checked code folder grants it, not the
/// access ([`crate::granted_with_code_folders`]).
pub async fn default_access(
    registry: &ToolRegistryHandle,
    default: &DefaultAccess,
    disabled: &[String],
) -> Vec<String> {
    let descriptors = match registry.list().await {
        Ok(descriptors) => descriptors,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "the default access is empty",
                "agent.access.default.unavailable"
            );
            return Vec::new();
        }
    };
    let mut names: Vec<String> = descriptors
        .into_iter()
        .filter(|d| default.families.contains(&d.family) || default.tools.contains(&d.name))
        .map(|d| d.name)
        .filter(|name| name != crate::CODE_CONTEXT_TOOL)
        .filter(|name| !disabled.contains(name))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `agent` with its default access written in when nobody chose for it, so a
/// card shows what the agent is granted. The `overridden` list is left as it
/// is: the access is still the default, not the user's.
pub async fn with_default_access(
    mut agent: ResolvedAgent,
    registry: &ToolRegistryHandle,
    default: &DefaultAccess,
) -> ResolvedAgent {
    if reads_by_default(&agent) {
        agent.access = default_access(registry, default, &[]).await;
    }
    agent
}

#[cfg(test)]
mod tests;
