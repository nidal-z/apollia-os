/**
 * User preferences for the template gallery (US-SP42-058).
 *
 * Currently a single toggle: whether to surface community templates in the
 * gallery. The MVP ships with the infrastructure (URL/signature/trust level
 * on the backend registry entry) but the community registry fetcher is not
 * wired — so the toggle is effectively a stub that only affects local
 * filtering of `source = "community"` entries once they land.
 */
import { writable } from "svelte/store";

const STORAGE_KEY = "apollia.templates.show_community";

function initial(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(STORAGE_KEY) === "1";
}

export const showCommunityTemplates = writable<boolean>(initial());

if (typeof window !== "undefined") {
  showCommunityTemplates.subscribe((v) => {
    try {
      localStorage.setItem(STORAGE_KEY, v ? "1" : "0");
    } catch {
      // Storage unavailable (private mode) — ignore.
    }
  });
}
