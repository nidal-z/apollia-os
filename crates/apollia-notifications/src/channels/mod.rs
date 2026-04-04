// Canaux de notification concrets.
//
// - desktop  : Notifications natives OS via `notify-rust`.
// - webhook  : Requêtes HTTP POST vers une URL configurée.
// - terminal : Séquences OSC dans le terminal (iTerm2, GNOME/VTE, bell).

pub mod desktop;
pub mod terminal;
pub mod webhook;

pub use desktop::DesktopChannel;
pub use terminal::TerminalChannel;
pub use webhook::WebhookChannel;
