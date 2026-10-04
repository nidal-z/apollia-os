//! The configuration in force, and how a new `apollia.toml` reaches the actors
//! without a restart.
//!
//! The runtime holds the last valid configuration in one actor, the
//! [`ConfigReloader`](actor). Each section the running engine can apply
//! (`[hitl]`, `[tools]`, `[mcp]`, `[hooks]`, `[chat]`, `[observability]`,
//! `[desktop]`) is broadcast on a `tokio::sync::watch`
//! channel of its own; a consumer holds the receiving end as a [`Live`] value
//! and reads it at its next use, or rebuilds what it derives from it between
//! two messages. Nothing is shared under a lock (principle 5): the reloader
//! sends, the consumers read their own copy.
//!
//! A new text reaches the reloader two ways: the save route hands it the text
//! it has just written, and [`ApolliaConfigWatcher`] says when the file
//! changed on disk. A text the reloader already applied is not applied twice,
//! which is how a save of the application is applied once although the
//! watcher sees it too. A valid text is compared with the configuration in
//! force section by section: the changed sections whose keys are
//! `Reloadable` in the key registry are sent, and the changed keys of the
//! other sections are marked as waiting for a restart, with their line. An
//! invalid text changes nothing: the last valid configuration stays in force
//! and `ConfigInvalid` says why. Each application ends with `ConfigReloaded`.
//!
//! [`ApolliaConfigWatcher`]: apollia_triggers::handlers::apollia_config_watch::ApolliaConfigWatcher

mod actor;
mod diff;
mod live;

#[cfg(test)]
pub(crate) mod tests;

use apollia_core::config::{
    ApolliaConfig, ChatConfig, DesktopConfig, HitlConfig, HooksConfig, McpConfig, ToolsConfig,
};
use apollia_core::ObservabilityConfig;
use tokio::sync::watch;

pub use actor::{ConfigReloadHandle, PendingRestartKey, ReloadStatus};
pub use live::Live;

/// The sections the running engine applies without a restart, each on a
/// channel of its own; the test of the registry holds their keys to it.
#[cfg(test)]
pub(crate) const LIVE_SECTIONS: [&str; 7] = [
    "hitl",
    "tools",
    "mcp",
    "hooks",
    "chat",
    "observability",
    "desktop",
];

/// The receiving ends of the section channels, one [`Live`] per section.
///
/// Cloned into every consumer; each clone reads the value in force.
#[derive(Debug, Clone)]
pub struct ConfigFeed {
    /// `[hitl]`, read by the watcher of the paused tasks at each scan.
    pub hitl: Live<HitlConfig>,
    /// `[tools]`, read when a tool dispatcher is built for a run.
    pub tools: Live<ToolsConfig>,
    /// `[mcp]`, read at the next chat exchange.
    pub mcp: Live<McpConfig>,
    /// `[hooks]`, from which the chat rebuilds its hook registry.
    pub hooks: Live<HooksConfig>,
    /// `[chat]`, read at the next conversation and the next exchange.
    pub chat: Live<ChatConfig>,
    /// `[observability]`, read when a run or an event is persisted.
    pub observability: Live<ObservabilityConfig>,
    /// `[desktop]`, for the desktop settings that follow the runtime.
    pub desktop: Live<DesktopConfig>,
}

impl ConfigFeed {
    /// A feed that never changes, for a runtime started without a file.
    pub fn fixed(config: &ApolliaConfig) -> Self {
        ConfigChannels::new(config).feed()
    }
}

/// The sending ends of the section channels.
///
/// Created by whoever starts the runtime, so the consumers built before the
/// supervisor can already hold their [`Live`] values; handed to the
/// supervisor, which gives it to the reloader.
#[derive(Debug)]
pub struct ConfigChannels {
    hitl: watch::Sender<HitlConfig>,
    tools: watch::Sender<ToolsConfig>,
    mcp: watch::Sender<McpConfig>,
    hooks: watch::Sender<HooksConfig>,
    chat: watch::Sender<ChatConfig>,
    observability: watch::Sender<ObservabilityConfig>,
    desktop: watch::Sender<DesktopConfig>,
}

impl ConfigChannels {
    /// One channel per live section, each holding the section of `config`.
    pub fn new(config: &ApolliaConfig) -> Self {
        Self {
            hitl: watch::Sender::new(config.hitl.clone()),
            tools: watch::Sender::new(config.tools.clone()),
            mcp: watch::Sender::new(config.mcp.clone()),
            hooks: watch::Sender::new(config.hooks.clone()),
            chat: watch::Sender::new(config.chat.clone()),
            observability: watch::Sender::new(config.observability.clone()),
            desktop: watch::Sender::new(config.desktop.clone()),
        }
    }

    /// A new set of receivers, each reading the value in force.
    pub fn feed(&self) -> ConfigFeed {
        ConfigFeed {
            hitl: Live::from_receiver(self.hitl.subscribe()),
            tools: Live::from_receiver(self.tools.subscribe()),
            mcp: Live::from_receiver(self.mcp.subscribe()),
            hooks: Live::from_receiver(self.hooks.subscribe()),
            chat: Live::from_receiver(self.chat.subscribe()),
            observability: Live::from_receiver(self.observability.subscribe()),
            desktop: Live::from_receiver(self.desktop.subscribe()),
        }
    }

    /// Sends the sections of `config` named in `sections`; a section without
    /// a channel of its own is skipped.
    fn publish(&self, config: &ApolliaConfig, sections: &[String]) {
        for section in sections {
            match section.as_str() {
                "hitl" => {
                    self.hitl.send_replace(config.hitl.clone());
                }
                "tools" => {
                    self.tools.send_replace(config.tools.clone());
                }
                "mcp" => {
                    self.mcp.send_replace(config.mcp.clone());
                }
                "hooks" => {
                    self.hooks.send_replace(config.hooks.clone());
                }
                "chat" => {
                    self.chat.send_replace(config.chat.clone());
                }
                "observability" => {
                    self.observability
                        .send_replace(config.observability.clone());
                }
                "desktop" => {
                    self.desktop.send_replace(config.desktop.clone());
                }
                _ => {}
            }
        }
    }
}
