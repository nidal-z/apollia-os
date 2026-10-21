//! The tools a turn offers the model, and the names it may call.

use super::*;
use helpers::build_tool_specs;

/// What a turn offers: the specs the model sees, and every name a call may
/// carry without being refused as unknown.
pub(in crate::chat::builtin_agent) struct Catalog {
    /// Tool specs advertised to the model.
    pub(in crate::chat::builtin_agent) specs: Vec<ToolSpec>,
    /// Names a call may carry.
    pub(in crate::chat::builtin_agent) callable: HashSet<String>,
}

impl BuiltInChatAgent {
    /// The catalog of a turn: the agent's granted accesses, and only them,
    /// plus the tools the loop itself needs.
    ///
    /// `granted` is the access list of the resolved agent. Native tools, MCP
    /// tools (`mcp:<server>/<tool>`) and package tools (`pkg:<package>/<tool>`)
    /// reach the model only when listed there. The deferred MCP index the
    /// agent was built with is expected to hold granted tools only, since
    /// `tool_search` searches it.
    ///
    /// The tools of the loop, offered whatever the access list says:
    ///
    /// - `tool_search`, in deferred MCP mode: searches the granted MCP tools
    ///   and reads nothing else;
    /// - `todo_write`, when the conversation keeps a task list;
    /// - `connection_lookup`, for Apollia in a conversation: searches the
    ///   connections the user can add, on this machine;
    /// - the `plan_*` tools, while the conversation is in plan mode.
    ///
    /// An agent or a workflow is offered only when granted (`agent:<id>`,
    /// `workflow:<id>`), described by the agent's mission or the workflow's
    /// description, and never the agent itself: no other agent, no A2A skill,
    /// reaches the model on its own.
    ///
    /// A pre-authorization or an allow rule only spares a granted tool its
    /// approval: it never makes a tool callable.
    pub(in crate::chat::builtin_agent) async fn catalog(&self, granted: &[String]) -> Catalog {
        let (calls, tools): (Vec<String>, Vec<String>) = granted
            .iter()
            .cloned()
            .partition(|access| crate::composition::is_call_access(access));
        let mut specs = build_tool_specs(
            &tools,
            &self.tool_registry,
            self.mcp_index.as_deref(),
            self.tool_search_limit,
        )
        .await;
        if let Some(composer) = &self.composer {
            specs.extend(
                crate::composition::tool_specs(composer.as_ref(), &calls, &self.agent_id).await,
            );
        }
        if self.todo.is_some() {
            specs.push(todo_write_spec());
        }
        if self.session.connection_lookup {
            specs.push(ToolSpec {
                name: crate::chat::connection_lookup::CONNECTION_LOOKUP_NAME.to_owned(),
                description: crate::chat::connection_lookup::CONNECTION_LOOKUP_DESCRIPTION
                    .to_owned(),
                parameters: crate::chat::connection_lookup::connection_lookup_input_schema(),
            });
        }
        // Phase-aware: once a plan is approved the proposal tools are withheld
        // so the agent executes the approved plan instead of re-proposing it.
        if self.plan_mode_active() {
            specs.extend(plan_tool::plan_tool_specs_for_phase(
                self.session_plan_phase,
            ));
        }
        // Every advertised spec, every granted name, and every indexed MCP tool
        // the model can discover through `tool_search`. A call to any other
        // name is refused with a corrective tool result rather than handed to
        // the invoker.
        let mut callable: HashSet<String> = specs.iter().map(|s| s.name.clone()).collect();
        callable.extend(tools);
        if let Some(index) = self.mcp_index.as_deref() {
            callable.extend(
                index
                    .iter()
                    .map(|e| format!("mcp:{}/{}", e.server_name, e.tool_name)),
            );
        }
        Catalog { specs, callable }
    }
}

#[cfg(test)]
mod tests;
