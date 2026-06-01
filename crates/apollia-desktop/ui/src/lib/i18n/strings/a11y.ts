/**
 * Accessibility strings - icon-only aria-labels and screen-reader hints.
 *
 * Any `aria-label`, `title`, or `alt` attribute on an icon-only control must
 * pull from this catalog. Adding an
 * icon-only button? Add its key here first, then reference `$t(...)`.
 */
export const A11Y_KEYS = {
  skipToContent: "a11y.skip_to_content",
  mainLandmark: "a11y.main_landmark",
  keyboardShortcuts: "a11y.keyboard_shortcuts",
  keyboardShortcutsHint: "a11y.keyboard_shortcuts_hint",
  stepLabel: "a11y.step_label",
  shortcut: {
    navBack: "a11y.shortcut.nav_back",
    navForward: "a11y.shortcut.nav_forward",
    openHints: "a11y.shortcut.open_hints",
    closeModal: "a11y.shortcut.close_modal",
  },
  close: "a11y.close",
  dismiss: "a11y.dismiss",
  dismissWarning: "a11y.dismiss_warning",
  actionsMenu: "a11y.actions_menu",
  clearSearch: "a11y.clear_search",
  microphone: "a11y.microphone",
  valueRedacted: "a11y.value_redacted",
  costThresholdExceeded: "a11y.cost_threshold_exceeded",
  progressCompleted: "a11y.progress_completed",
  backShortcut: "a11y.back_shortcut",
  forwardShortcut: "a11y.forward_shortcut",
  logoAlt: "a11y.logo_alt",
} as const;
