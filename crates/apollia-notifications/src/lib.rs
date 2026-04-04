//! Système de notification découplé pour Apollia OS.
//!
//! Cette crate centralise la logique de notification :
//! - [`NotificationChannel`] — trait à implémenter par chaque canal de livraison
//! - [`NotificationEngine`] — s'abonne à l'EventBus et dispatche les événements
//! - [`event_filter::map_event`] — transforme un [`RuntimeEvent`] en [`Notification`]
//! - [`InactivityWatcher`] — surveille l'inactivité et publie une notification OS + terminal
//!
//! Les canaux concrets (`DesktopChannel`, `WebhookChannel`, `TerminalChannel`) sont dans
//! le module [`channels`].

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
