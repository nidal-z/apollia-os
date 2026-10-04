//! The live sections of the configuration, applied by the chat actor between
//! two commands.

use super::*;

impl ChatSessionManager {
    /// Applies the sections that changed since the last command: `[chat]`
    /// for the next conversation, `[tools]` and `[mcp]` for the next
    /// exchange, `[hooks]` by rebuilding the hook registry.
    pub(in crate::chat::manager) fn apply_config_changes(&mut self) {
        let Some(feed) = self.config_feed.as_mut() else {
            return;
        };
        let tools = feed.tools.changed();
        let chat = feed.chat.changed();
        let mcp = feed.mcp.changed();
        let hooks = feed.hooks.changed();

        if let Some(chat) = &chat {
            self.plan_mode_default = chat.plan_mode_default;
            self.plan_mode_default_tx
                .send_replace(chat.plan_mode_default);
        }
        if let Some(mcp) = mcp {
            self.mcp_loading = LoadingMode::from(mcp.tool_loading);
            self.tool_search_limit = mcp.tool_search_limit;
        }
        if tools.is_some() || chat.is_some() {
            if let Some(current) = self.chat_tools_config.as_deref() {
                let mut next = current.clone();
                if let Some(tools) = tools {
                    next.tools_config = tools;
                }
                if let Some(chat) = &chat {
                    next.tool_turn_temperature = chat.tool_turn_temperature;
                }
                self.chat_tools_config = Some(Arc::new(next));
            }
        }
        if let Some(hooks) = hooks {
            let executor = Arc::new(HookExecutor::new(Arc::new(
                crate::hooks::HookRegistry::from_config(&hooks),
            )));
            self.hook_executor = Some(Arc::clone(&executor));
            self.hook_executor_tx.send_replace(Some(executor));
        }
    }
}
