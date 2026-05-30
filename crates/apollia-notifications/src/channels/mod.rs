// Concrete notification channels.
//
// - desktop:  native OS notifications via `notify-rust`.
// - webhook:  HTTP POST requests to a configured URL.
// - terminal: OSC sequences in the terminal (iTerm2, GNOME/VTE, bell).

pub mod desktop;
pub mod terminal;
pub mod webhook;

pub use desktop::DesktopChannel;
pub use terminal::TerminalChannel;
pub use webhook::WebhookChannel;
