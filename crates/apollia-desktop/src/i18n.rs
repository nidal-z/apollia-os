//! Two-language catalogue for the native shell surfaces.
//!
//! The webview owns the real i18n (svelte-i18n over `en.json` / `fr.json`).
//! The shell still renders a handful of native surfaces outside the webview:
//! the tray menu, the macOS application menu, the STT notifications and the
//! dictation overlay window title. Those strings used to be hardcoded in the
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
    let base = tag.split(['-', '_']).next().unwrap_or("");
    let value = u8::from(base.eq_ignore_ascii_case("en"));
    LOCALE.store(value, Ordering::Relaxed);
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
        ];
        // WHEN comparing the two locales pairwise
        // THEN no label is the same string in both languages
        for (fr, en) in pairs {
            assert_ne!(fr, en, "label identical in both locales: {fr}");
        }
    }
}
