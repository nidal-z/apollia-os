import { writable, get } from "svelte/store";

/** Available theme modes for the application. */
export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "apollia-theme";

function readStoredMode(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  return "system";
}

/** Reactive store holding the current theme mode preference. */
export const themeMode = writable<ThemeMode>(readStoredMode());

/** Apply the given theme mode to the document and persist the choice. */
export function applyTheme(mode: ThemeMode): void {
  localStorage.setItem(STORAGE_KEY, mode);
  const isDark =
    mode === "dark" ||
    (mode === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", isDark);
}

/** Initialize theme: apply stored preference and listen for OS changes. */
export function initTheme(): void {
  const mode = get(themeMode);
  applyTheme(mode);

  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  mediaQuery.addEventListener("change", (e) => {
    if (get(themeMode) === "system") {
      document.documentElement.classList.toggle("dark", e.matches);
    }
  });

  themeMode.subscribe((newMode) => {
    applyTheme(newMode);
  });
}
