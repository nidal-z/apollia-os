//! Two-language catalogue for the native shell surfaces.
//!
//! The webview owns the real i18n (svelte-i18n over `en.json` / `fr.json`).
//! The shell still renders a handful of native surfaces outside the webview:
//! the tray menu, the macOS application menu, the STT notifications, the
//! dictation overlay window title and the window of a refused launch. Those strings used to be hardcoded in the
//! language of their author of the day, which put seven English and four
//! French labels in the same menu.
//!
//! The frontend transmits the interface locale over the
//! [`EVENT_UI_LOCALE`] Tauri event (emitted by `main.ts` at startup and on
//! every locale switch). [`attach_listener`] stores it; every label helper
//! takes the locale as a parameter so it stays a pure, testable function.
//! Until the first event arrives the shell renders French, the same default
//! `svelte-i18n` resolves for a first launch.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::Listener;

use crate::startup::RefusalReason;

/// Tauri event name emitted by the frontend when the interface locale is
/// known or changes. Payload: `{"locale": "fr" | "en"}`.
pub const EVENT_UI_LOCALE: &str = "ui-locale-changed";

/// Interface languages the shell can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLocale {
    /// French, the default locale of the interface.
    Fr,
    /// English.
    En,
}

/// Stored locale: 0 = French (the interface default), 1 = English.
static LOCALE: AtomicU8 = AtomicU8::new(0);

/// Records the interface locale transmitted by the frontend.
///
/// Accepts BCP 47 tags (`en`, `en-US`, `fr-FR`); anything that is not
/// English resolves to French, mirroring the two-catalogue frontend.
pub fn set_locale(tag: &str) {
    let value = u8::from(UiLocale::from_tag(tag) == UiLocale::En);
    LOCALE.store(value, Ordering::Relaxed);
}

impl UiLocale {
    /// The interface locale a BCP 47 tag names (`en`, `en-US`, `fr-FR`).
    /// Anything that is not English is French, as in the two catalogues of
    /// the frontend.
    pub fn from_tag(tag: &str) -> Self {
        let base = tag.split(['-', '_']).next().unwrap_or("");
        if base.eq_ignore_ascii_case("en") {
            Self::En
        } else {
            Self::Fr
        }
    }
}

/// The interface locale last transmitted by the frontend.
pub fn locale() -> UiLocale {
    if LOCALE.load(Ordering::Relaxed) == 1 {
        UiLocale::En
    } else {
        UiLocale::Fr
    }
}

/// JSON payload of the [`EVENT_UI_LOCALE`] event.
#[derive(Debug, serde::Deserialize)]
struct UiLocalePayload {
    /// Interface locale, `"fr"` or `"en"`.
    locale: String,
}

/// Stores the locale carried by every [`EVENT_UI_LOCALE`] event.
///
/// Surfaces that render at call time (STT notifications, overlay title) read
/// [`locale`] and need nothing else; surfaces built once (tray menu, app
/// menu) attach their own listener to refresh their labels.
pub fn attach_listener(app: &tauri::App) {
    app.listen(EVENT_UI_LOCALE, |event| {
        if let Ok(payload) = serde_json::from_str::<UiLocalePayload>(event.payload()) {
            set_locale(&payload.locale);
        }
    });
}

/// Tray menu label for the "open the main window" item.
pub fn tray_open(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Ouvrir Apollia OS",
        UiLocale::En => "Open Apollia OS",
    }
}

/// Tray menu label for the quit item.
pub fn tray_quit(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Quitter",
        UiLocale::En => "Quit",
    }
}

/// macOS application menu label for the quit item.
pub fn app_quit(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Quitter Apollia OS",
        UiLocale::En => "Quit Apollia OS",
    }
}

/// macOS application menu title for the Edit submenu.
pub fn menu_edit(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Édition",
        UiLocale::En => "Edit",
    }
}

/// macOS application menu title for the Window submenu.
pub fn menu_window(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Fenêtre",
        UiLocale::En => "Window",
    }
}

/// Tray menu label for the pending-approvals counter.
pub fn approvals_label(l: UiLocale, count: usize) -> String {
    match (l, count) {
        (UiLocale::Fr, 0) => "Aucune approbation en attente".to_string(),
        (UiLocale::Fr, 1) => "1 approbation en attente".to_string(),
        (UiLocale::Fr, n) => format!("{n} approbations en attente"),
        (UiLocale::En, 0) => "No pending approvals".to_string(),
        (UiLocale::En, 1) => "1 approval pending".to_string(),
        (UiLocale::En, n) => format!("{n} approvals pending"),
    }
}

/// Notification title shown when the dictation hotkey fires without a model.
pub fn stt_no_model_title(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Aucun modèle de dictée chargé",
        UiLocale::En => "No speech model loaded",
    }
}

/// Notification body shown when the dictation hotkey fires without a model.
pub fn stt_no_model_body(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Activez la dictée et chargez un modèle dans les Réglages.",
        UiLocale::En => "Turn dictation on and load a model in Settings.",
    }
}

/// Notification title for a finished transcription.
pub fn stt_ready_title(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Transcription prête",
        UiLocale::En => "Transcription ready",
    }
}

/// Notification title shown when no input device is connected.
pub fn stt_no_mic_title(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Aucun microphone détecté",
        UiLocale::En => "No microphone detected",
    }
}

/// Notification body shown when no input device is connected.
pub fn stt_no_mic_body(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Branchez un microphone pour utiliser la dictée vocale.",
        UiLocale::En => "Plug in a microphone to use voice dictation.",
    }
}

/// Window title of the dictation overlay.
pub fn overlay_recording(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Enregistrement",
        UiLocale::En => "Recording",
    }
}

/// Window title of the hardware window.
pub fn hardware_window_title(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Matériel",
        UiLocale::En => "Hardware",
    }
}

/// Window title of the window that shows the logs of the local server.
pub fn server_logs_window_title(l: UiLocale) -> &'static str {
    match l {
        UiLocale::Fr => "Journaux du serveur",
        UiLocale::En => "Server logs",
    }
}

/// Error a command not routed yet answers while the runtime is a daemon.
pub(crate) fn attached_unavailable(locale: UiLocale, command: &str) -> String {
    match locale {
        UiLocale::Fr => format!(
            "« {command} » est indisponible quand Apollia tourne en service : cette action demande que le runtime tourne dans l'application."
        ),
        UiLocale::En => format!(
            "\"{command}\" is unavailable while Apollia runs as a service: this action needs the runtime to run inside the application."
        ),
    }
}

/// Title of the window a refused launch opens.
pub(crate) fn startup_refused_title(locale: UiLocale) -> &'static str {
    match locale {
        UiLocale::Fr => "Apollia OS ne démarre pas",
        UiLocale::En => "Apollia OS does not start",
    }
}

/// Text of the window a refused launch opens: what holds the port, the
/// gesture to make, and where the reason is logged.
pub(crate) fn startup_refused_message(
    locale: UiLocale,
    reason: &RefusalReason,
    log: &std::path::Path,
) -> String {
    let log = log.display();
    let what = match (locale, reason) {
        (UiLocale::Fr, RefusalReason::IncompatibleRuntime { port, version, ours }) => format!(
            "Le runtime Apollia {version} sert le port {port}, et cette application en version {ours} ne peut pas le piloter.\n\nArrêtez ce runtime avec « apollia-os stop » dans un terminal, puis relancez Apollia OS."
        ),
        (UiLocale::En, RefusalReason::IncompatibleRuntime { port, version, ours }) => format!(
            "The Apollia runtime {version} serves port {port}, and this application in version {ours} cannot drive it.\n\nStop that runtime with \"apollia-os stop\" in a terminal, then launch Apollia OS again."
        ),
        (UiLocale::Fr, RefusalReason::TokenRefused { port }) => format!(
            "Le programme qui sert le port {port} refuse le jeton de cette installation : c'est un runtime Apollia d'un autre dossier de données, ou un autre programme.\n\nArrêtez-le (« apollia-os stop » s'il s'agit d'Apollia), puis relancez Apollia OS."
        ),
        (UiLocale::En, RefusalReason::TokenRefused { port }) => format!(
            "The program serving port {port} refuses the token of this installation: it is an Apollia runtime of another data directory, or another program.\n\nStop it (\"apollia-os stop\" if it is Apollia), then launch Apollia OS again."
        ),
        (UiLocale::Fr, RefusalReason::ForeignListener { port }) => format!(
            "Le port {port} est occupé par un programme qui n'est pas Apollia.\n\nFermez ce programme (« lsof -i :{port} » le nomme sur macOS et Linux), puis relancez Apollia OS."
        ),
        (UiLocale::En, RefusalReason::ForeignListener { port }) => format!(
            "Port {port} is held by a program that is not Apollia.\n\nClose that program (\"lsof -i :{port}\" names it on macOS and Linux), then launch Apollia OS again."
        ),
        (UiLocale::Fr, RefusalReason::PortUnavailable { port, source }) => format!(
            "Le port {port} ne peut pas être ouvert : {source}.\n\nVérifiez qu'aucune règle du système n'interdit ce port, puis relancez Apollia OS."
        ),
        (UiLocale::En, RefusalReason::PortUnavailable { port, source }) => format!(
            "Port {port} cannot be opened: {source}.\n\nCheck that no system rule forbids this port, then launch Apollia OS again."
        ),
    };
    match locale {
        UiLocale::Fr => format!("{what}\n\nLa raison est écrite dans {log}."),
        UiLocale::En => format!("{what}\n\nThe reason is written to {log}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_locale_resolves_base_language() {
        // GIVEN BCP 47 tags of both languages, region-qualified or not
        // WHEN the shell stores then reads them back
        set_locale("en-US");
        assert_eq!(locale(), UiLocale::En);
        set_locale("fr-FR");
        assert_eq!(locale(), UiLocale::Fr);
        set_locale("EN");
        assert_eq!(locale(), UiLocale::En);
        // THEN an unknown tag falls back to French, the interface default
        set_locale("de");
        assert_eq!(locale(), UiLocale::Fr);
    }

    #[test]
    fn test_approvals_label_english_plural_forms() {
        // GIVEN the English locale
        // WHEN formatting 0, 1 and 5 pending approvals
        // THEN the empty state, singular and plural forms come out
        assert_eq!(approvals_label(UiLocale::En, 0), "No pending approvals");
        assert_eq!(approvals_label(UiLocale::En, 1), "1 approval pending");
        assert_eq!(approvals_label(UiLocale::En, 5), "5 approvals pending");
    }

    #[test]
    fn test_approvals_label_french_plural_forms() {
        // GIVEN the French locale
        // WHEN formatting 0, 1 and 5 pending approvals
        // THEN the empty state, singular and plural forms come out
        assert_eq!(
            approvals_label(UiLocale::Fr, 0),
            "Aucune approbation en attente"
        );
        assert_eq!(approvals_label(UiLocale::Fr, 1), "1 approbation en attente");
        assert_eq!(
            approvals_label(UiLocale::Fr, 5),
            "5 approbations en attente"
        );
    }

    #[test]
    fn test_every_label_differs_between_locales() {
        // GIVEN every static label helper of the catalogue
        let pairs = [
            (tray_open(UiLocale::Fr), tray_open(UiLocale::En)),
            (tray_quit(UiLocale::Fr), tray_quit(UiLocale::En)),
            (app_quit(UiLocale::Fr), app_quit(UiLocale::En)),
            (menu_edit(UiLocale::Fr), menu_edit(UiLocale::En)),
            (menu_window(UiLocale::Fr), menu_window(UiLocale::En)),
            (
                stt_no_model_title(UiLocale::Fr),
                stt_no_model_title(UiLocale::En),
            ),
            (
                stt_no_model_body(UiLocale::Fr),
                stt_no_model_body(UiLocale::En),
            ),
            (stt_ready_title(UiLocale::Fr), stt_ready_title(UiLocale::En)),
            (
                stt_no_mic_title(UiLocale::Fr),
                stt_no_mic_title(UiLocale::En),
            ),
            (stt_no_mic_body(UiLocale::Fr), stt_no_mic_body(UiLocale::En)),
            (
                overlay_recording(UiLocale::Fr),
                overlay_recording(UiLocale::En),
            ),
            (
                hardware_window_title(UiLocale::Fr),
                hardware_window_title(UiLocale::En),
            ),
            (
                server_logs_window_title(UiLocale::Fr),
                server_logs_window_title(UiLocale::En),
            ),
        ];
        // WHEN comparing the two locales pairwise
        // THEN no label is the same string in both languages
        for (fr, en) in pairs {
            assert_ne!(fr, en, "label identical in both locales: {fr}");
        }
    }
}
