// Canaux de notification concrets.
//
// - desktop : Notifications natives OS via `notify-rust` (STORY-100)
// - webhook : Requêtes HTTP POST vers une URL configurée (STORY-101)

pub mod desktop;

pub use desktop::DesktopChannel;
