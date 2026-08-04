//! System tray integration for Apollia OS desktop.
//!
//! Provides a persistent tray icon with a context menu that allows the user to
//! open the main window, see pending HITL approvals, and quit the application
//! gracefully. The tray tooltip and menu are updated dynamically via Tauri
//! events emitted by the frontend when SSE state changes.

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager};

/// Tray menu item identifier for "Open Apollia OS".
const MENU_OPEN: &str = "open";

/// Tray menu item identifier for pending approvals display.
const MENU_APPROVALS: &str = "approvals";

/// Tray menu item identifier for "Quit".
const MENU_QUIT: &str = "quit";

/// Application (macOS menu bar) identifier for the "Quit" item.
///
/// Distinct from [`MENU_QUIT`] (the tray menu) so both menus can be handled by
/// their own routers without collision. Handled at the `tauri::Builder` level
/// in `main.rs`, which routes it to [`initiate_quit`].
pub const MENU_QUIT_APP: &str = "quit-app";

/// Tauri event name emitted by the frontend to update the tray badge.
pub const EVENT_TRAY_UPDATE: &str = "tray-update";

/// Formats the tray tooltip based on the number of active agents and pending approvals.
pub fn format_tooltip(active_agents: usize, pending_approvals: usize) -> String {
    let mut parts = Vec::new();

    if active_agents > 0 {
        let suffix = if active_agents == 1 {
            "actif"
        } else {
            "actifs"
        };
        parts.push(format!(
            "{active_agents} agent{} {suffix}",
            plural_s(active_agents)
        ));
    }

    if pending_approvals > 0 {
        parts.push(format!(
            "{pending_approvals} approbation{} en attente",
            plural_s(pending_approvals)
        ));
    }

    if parts.is_empty() {
        "Apollia OS".to_string()
    } else {
        format!("Apollia OS - {}", parts.join(", "))
    }
}

/// Formats the menu item label for pending approvals.
///
/// Returns a human-readable string like "2 approbations en attente" or
/// "Aucune approbation en attente" when count is 0.
pub fn format_approvals_label(count: usize) -> String {
    if count == 0 {
        "Aucune approbation en attente".to_string()
    } else {
        format!("{count} approbation{} en attente", plural_s(count))
    }
}

/// Returns "s" for plural counts, empty string for singular.
fn plural_s(n: usize) -> &'static str {
    if n <= 1 {
        ""
    } else {
        "s"
    }
}

/// What the window close button does on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseAction {
    /// Hide the window and keep the runtime alive behind the tray icon.
    HideToTray,
    /// Quit the application, stopping the runtime and its child processes.
    Quit,
}

/// Resolves the close-button behaviour for the host platform.
///
/// macOS keeps the window open in spirit: closing the last window is expected
/// to leave the application running in the Dock, and every macOS quit surface
/// (Cmd+Q, the app menu) is a separate gesture.
///
/// Windows and Linux read the close button as "quit". Leaving the runtime alive
/// there strands the child processes with no visible owner, which is exactly
/// what a Windows test session reported: the window was gone and the backend
/// was still running.
/// `cfg!` rather than `#[cfg]`: both arms are then compiled on every platform,
/// so neither variant reads as dead code on the platform that does not pick it.
pub(crate) fn close_button_action() -> CloseAction {
    if cfg!(target_os = "macos") {
        CloseAction::HideToTray
    } else {
        CloseAction::Quit
    }
}

/// Shows and focuses the main window.
///
/// Silently ignores errors if the window is not found (defensive; should not
/// happen since the main window is always created).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Initiates graceful shutdown by emitting a shutdown event to the runtime,
/// then exits the Tauri process.
///
/// The shutdown is triggered via HTTP POST to the embedded runtime API so
/// the full graceful shutdown sequence (drain tasks, stop agents) runs before
/// process exit.
///
/// This is the single quit entry point shared by every surface: the tray
/// "Quitter", the macOS app menu / Cmd+Q, the in-app user menu, the command
/// palette, and the window close button on Windows and Linux. It relies on
/// [`AppHandle::exit`], which bypasses the window `CloseRequested` handler so
/// quitting is never swallowed by `prevent_close`.
pub(crate) fn initiate_quit(app: &AppHandle) {
    // Release global hotkeys before the process exits so the OS reclaims them
    // immediately and other applications can register the same shortcuts.
    if let Err(e) = crate::stt::hotkey::unregister_all(app) {
        tracing::warn!(error = %e, "failed to unregister hotkeys during shutdown");
    }

    let app_handle = app.clone();
    // Fire-and-forget: send POST /api/v1/shutdown then exit
    tauri::async_runtime::spawn(async move {
        // Best-effort shutdown request to the embedded runtime
        let _ =
            crate::commands::http_post_json(7771, "/api/v1/shutdown", &serde_json::json!({})).await;
        app_handle.exit(0);
    });
}

/// Initialises the system tray icon with menu and event handlers.
///
/// Called from `main.rs` inside the Tauri `setup()` hook. Creates:
/// - A tray icon using the app's default window icon
/// - A context menu with "Ouvrir Apollia OS", approvals counter, and "Quitter"
/// - Event handlers for menu clicks and tray icon left-click
/// - A listener for `tray-update` events from the frontend to update tooltip/menu
///
/// # Errors
///
/// Returns an error if menu or tray icon construction fails (missing icon, etc.).
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItemBuilder::new("Ouvrir Apollia OS")
        .id(MENU_OPEN)
        .build(app)?;

    let approvals_item = MenuItemBuilder::new(format_approvals_label(0))
        .id(MENU_APPROVALS)
        .enabled(false)
        .build(app)?;

    let quit_item = MenuItemBuilder::new("Quitter").id(MENU_QUIT).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .separator()
        .item(&approvals_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let icon = app
        .default_window_icon()
        .ok_or("no default window icon configured in tauri.conf.json")?
        .clone();

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Apollia OS")
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => show_main_window(app),
            MENU_QUIT => initiate_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    // Listen for tray-update events from the frontend to refresh tooltip and menu
    let approvals_item_clone = approvals_item.clone();
    app.listen(EVENT_TRAY_UPDATE, move |event| {
        if let Ok(payload) = serde_json::from_str::<TrayUpdatePayload>(event.payload()) {
            let tooltip = format_tooltip(payload.active_agents, payload.pending_approvals);
            // Tray icon tooltip update: Tauri v2 does not expose set_tooltip on TrayIcon
            // after construction in all platforms, so we update the menu item as primary
            // feedback channel.
            let label = format_approvals_label(payload.pending_approvals);
            let _ = approvals_item_clone.set_text(&label);
            let _ = approvals_item_clone.set_enabled(payload.pending_approvals > 0);

            // tooltip is computed but not dynamically settable on all platforms
            // in Tauri v2 after construction; the menu item is the primary
            // dynamic feedback visible to the user.
            let _ = tooltip;
        }
    });

    Ok(())
}

/// Builds and installs the macOS application menu bar.
///
/// Tauri auto-generates a default macOS menu whose "Quit" item invokes the
/// native `terminate:` selector. That selector asks each window to close,
/// which Apollia intercepts via `CloseRequested` + `prevent_close` (the
/// close-to-tray behaviour), so the native quit is silently cancelled and
/// Cmd+Q becomes a no-op. Replacing the menu with our own "Quit" item (id
/// [`MENU_QUIT_APP`], bound to Cmd+Q) routes the quit through
/// [`initiate_quit`] instead, which exits cleanly.
///
/// The Edit and Window submenus reinstate the standard editing accelerators
/// (Cmd+C / V / X / A, undo/redo) that a fully custom menu would otherwise
/// drop from the webview.
///
/// # Errors
///
/// Returns an error if menu construction or installation fails.
#[cfg(target_os = "macos")]
pub fn setup_app_menu(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::SubmenuBuilder;

    let quit_item = MenuItemBuilder::new("Quitter Apollia OS")
        .id(MENU_QUIT_APP)
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Apollia OS")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit_item)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Édition")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Fenêtre")
        .minimize()
        .maximize()
        .separator()
        .fullscreen()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&window_menu)
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}

/// JSON payload for the `tray-update` event emitted by the frontend.
#[derive(Debug, serde::Deserialize)]
struct TrayUpdatePayload {
    /// Number of agents in an active state.
    active_agents: usize,
    /// Number of HITL approvals currently pending.
    pending_approvals: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tooltip_zero_agents_zero_approvals() {
        // GIVEN no agents and no approvals
        // WHEN formatting the tooltip
        let tooltip = format_tooltip(0, 0);
        // THEN it shows just the app name
        assert_eq!(tooltip, "Apollia OS");
    }

    #[test]
    fn test_tooltip_single_agent_no_approvals() {
        // GIVEN 1 active agent and no approvals
        let tooltip = format_tooltip(1, 0);
        // THEN it uses singular form
        assert_eq!(tooltip, "Apollia OS - 1 agent actif");
    }

    #[test]
    fn test_tooltip_multiple_agents_no_approvals() {
        // GIVEN 3 active agents and no approvals
        let tooltip = format_tooltip(3, 0);
        // THEN it uses plural form
        assert_eq!(tooltip, "Apollia OS - 3 agents actifs");
    }

    #[test]
    fn test_tooltip_agents_and_approvals() {
        // GIVEN 3 active agents and 2 pending approvals
        let tooltip = format_tooltip(3, 2);
        // THEN both are shown
        assert_eq!(
            tooltip,
            "Apollia OS - 3 agents actifs, 2 approbations en attente"
        );
    }

    #[test]
    fn test_tooltip_single_approval() {
        // GIVEN 1 agent and 1 approval
        let tooltip = format_tooltip(1, 1);
        // THEN singular forms are used
        assert_eq!(
            tooltip,
            "Apollia OS - 1 agent actif, 1 approbation en attente"
        );
    }

    #[test]
    fn test_tooltip_only_approvals() {
        // GIVEN no agents but 3 pending approvals
        let tooltip = format_tooltip(0, 3);
        // THEN only approvals are shown
        assert_eq!(tooltip, "Apollia OS - 3 approbations en attente");
    }

    #[test]
    fn test_approvals_label_zero() {
        // GIVEN 0 pending approvals
        let label = format_approvals_label(0);
        // THEN it shows "Aucune"
        assert_eq!(label, "Aucune approbation en attente");
    }

    #[test]
    fn test_approvals_label_one() {
        // GIVEN 1 pending approval
        let label = format_approvals_label(1);
        // THEN singular form
        assert_eq!(label, "1 approbation en attente");
    }

    #[test]
    fn test_approvals_label_multiple() {
        // GIVEN 5 pending approvals
        let label = format_approvals_label(5);
        // THEN plural form
        assert_eq!(label, "5 approbations en attente");
    }

    #[test]
    fn test_plural_s() {
        assert_eq!(plural_s(0), "");
        assert_eq!(plural_s(1), "");
        assert_eq!(plural_s(2), "s");
        assert_eq!(plural_s(100), "s");
    }

    #[test]
    fn test_close_button_quits_everywhere_but_macos() {
        // GIVEN the platform this test was compiled for
        // WHEN the close-button behaviour is resolved
        let action = close_button_action();

        // THEN macOS keeps the runtime alive behind the tray, and Windows and
        // Linux quit, where leaving it alive stranded the child processes
        #[cfg(target_os = "macos")]
        assert_eq!(action, CloseAction::HideToTray);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(action, CloseAction::Quit);
    }

    #[test]
    fn test_tray_update_payload_deserialize() {
        // GIVEN a valid JSON payload
        let json = r#"{"active_agents": 2, "pending_approvals": 1}"#;
        // WHEN deserialized
        let payload: TrayUpdatePayload = serde_json::from_str(json).expect("valid payload");
        // THEN fields are parsed correctly
        assert_eq!(payload.active_agents, 2);
        assert_eq!(payload.pending_approvals, 1);
    }
}
