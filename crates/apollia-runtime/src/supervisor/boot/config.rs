//! Phase 1c: the configuration in force, its live sections, and the reloader
//! that follows the file.

use crate::config_reload::{ConfigChannels, ConfigFeed, ConfigReloadHandle};
use crate::supervisor::*;

impl Supervisor {
    /// The live sections every consumer is handed, and the reloader when the
    /// runtime was started on a file.
    ///
    /// The channels are the caller's when it built consumers before the
    /// runtime, otherwise built here from the configuration the runtime starts
    /// on. Without a file nothing changes them: each [`Live`] keeps its value.
    ///
    /// [`Live`]: crate::config_reload::Live
    pub(super) fn start_config_reload(
        &mut self,
        event_sender: &EventBusSender,
        org_policy: &apollia_permissions::PolicyVerdict,
    ) -> (ConfigFeed, Option<ConfigReloadHandle>) {
        let channels = self
            .config
            .config_channels
            .take()
            .unwrap_or_else(|| ConfigChannels::new(&self.seed_config()));
        let feed = channels.feed();
        let Some(path) = self.config.config_path.clone() else {
            return (feed, None);
        };
        let handle = ConfigReloadHandle::spawn(
            path.clone(),
            channels,
            org_policy.imposed_settings(),
            event_sender,
        );
        handle.watch_file(path);
        info!("supervisor.config_reload.ready");
        (feed, Some(handle))
    }

    /// The configuration the runtime starts on, as one root: the file when it
    /// validates, with the sections this supervisor was given on top.
    fn seed_config(&self) -> apollia_core::config::ApolliaConfig {
        let mut seed = self
            .config
            .config_path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| apollia_core::config::validate(&text).ok())
            .unwrap_or_default();
        seed.hitl = self.config.hitl_config.clone();
        seed.tools = self.config.tools_config.clone();
        seed.hooks = self.config.hooks_config.clone();
        seed.observability = self.config.obs_config.clone();
        seed.mcp.tool_search_limit = self.config.tool_search_limit;
        seed.chat.plan_mode_default = self.config.plan_mode_default;
        seed.chat.tool_turn_temperature = self.config.chat_tool_turn_temperature;
        seed
    }
}
