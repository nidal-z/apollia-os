/**
 * What a run resets before its first step.
 *
 * A gestural run must start from the same interface twice, on any machine.
 * Two things break that on their own: preferences a previous run persisted in
 * this browser profile, and a language the application infers from the system
 * when nothing was stored. Both live here rather than in the runner, which
 * plays steps.
 */
import { locale, waitLocale } from "svelte-i18n";
import { LOCALE_STORAGE_KEY } from "$lib/i18n";

/**
 * Pins the interface language for the run.
 *
 * The application resolves its language from a stored preference, and failing
 * that from the machine: the same book therefore reads English on one system
 * and French on another, and every assertion on a rendered sentence turns on
 * the locale of whoever plays it. Measured on 2026-09-08: two journal
 * assertions passed on macOS and failed under Ubuntu, on an identical build.
 * The books assert English, so the run states English rather than inheriting
 * it. A book that drives the language buttons still does, this only fixes
 * where it starts.
 */
export async function pinRunLocale(): Promise<void> {
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, "en");
  } catch {
    // A profile that refuses storage still gets the locale set below.
  }
  locale.set("en");
  await waitLocale("en");
}

export function resetDeterministicUiState(): void {
  if (typeof localStorage === "undefined") return;
  const keys = [
    "apollia.quickpicker.expanded",
    "apollia.delete_automation.skip",
    "apollia.next_steps.dismissed",
    "apollia.next_steps.feedback",
    "apollia.ui.sidebar",
    // layout.ts persists the drawer state whenever the viewport is sm, which
    // a resizeWindow block reaches.
    "apollia.ui.sidebarState_sm",
    // McpDisclaimerDialog.svelte / WizardStepDisclaimer.svelte.
    "apollia-mcp-disclaimer-accepted",
    "apollia-mcp-disclaimer-version",
    // companion.ts: a keyboard nudge of the panel would leak its position.
    "companionGeometry",
    // agentInstallPrefs.ts: settings-det toggles it, the install deps step reads it.
    "apollia.agent-install-prefs",
  ];
  for (const key of keys) {
    try {
      localStorage.removeItem(key);
    } catch {
      // Private mode / quota - ignore.
    }
  }
}
