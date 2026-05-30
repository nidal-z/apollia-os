//! Decoupled notification system for Apollia OS.
//!
//! This crate centralizes the notification logic:
//! - [`NotificationChannel`]: trait implemented by each delivery channel
//! - [`NotificationEngine`]: subscribes to the EventBus and dispatches events
//! - [`event_filter::map_event`]: turns a [`RuntimeEvent`] into a [`Notification`]
//! - [`InactivityWatcher`]: watches for inactivity and publishes an OS + terminal notification
//!
//! The concrete channels (`DesktopChannel`, `WebhookChannel`, `TerminalChannel`)
//! live in the [`channels`] module.

pub mod channels;
pub mod config;
pub mod engine;
pub mod event_filter;
pub mod inactivity_watcher;
pub mod repository;
pub mod validation;

pub use channels::terminal::TerminalChannel;
pub use channels::webhook::WebhookChannelConfig;
pub use channels::{DesktopChannel, WebhookChannel};
pub use config::{
    build_channels, ChannelConfig, ChannelKind, NotifConfigError, NotificationConfig, Severity,
};
pub use engine::{
    NotifError, Notification, NotificationChannel, NotificationEngine, NotificationEngineHandle,
};
pub use inactivity_watcher::InactivityWatcher;
pub use repository::{NotificationChannelRow, NotificationConfigRepository, NotificationLogRow};
pub use validation::{NotificationConfigError, KNOWN_EVENTS};
