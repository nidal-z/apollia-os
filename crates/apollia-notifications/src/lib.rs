//! Système de notification découplé pour Apollia OS.
//!
//! Cette crate centralise la logique de notification :
//! - [`NotificationChannel`] — trait à implémenter par chaque canal de livraison
//! - [`NotificationEngine`] — s'abonne à l'EventBus et dispatche les événements
//! - [`event_filter::map_event`] — transforme un [`RuntimeEvent`] en [`Notification`]
//!
//! Les canaux concrets (`DesktopChannel`, `WebhookChannel`) sont dans le module
//! [`channels`] et implémentés dans STORY-100 et STORY-101.

pub mod channels;
pub mod config;
pub mod engine;
pub mod event_filter;

pub use channels::webhook::WebhookChannelConfig;
pub use channels::{DesktopChannel, WebhookChannel};
pub use config::{
    build_channels, ChannelConfig, ChannelKind, NotifConfigError, NotificationConfig, Severity,
};
pub use engine::{
    NotifError, Notification, NotificationChannel, NotificationEngine, NotificationEngineHandle,
};
