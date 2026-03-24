// Canaux de notification concrets.
//
// - desktop : Notifications natives OS via `notify-rust`.
// - webhook : Requêtes HTTP POST vers une URL configurée.

pub mod desktop;
pub mod webhook;

pub use desktop::DesktopChannel;
pub use webhook::WebhookChannel;
