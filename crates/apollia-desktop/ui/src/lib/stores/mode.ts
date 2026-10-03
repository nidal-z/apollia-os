/**
 * Former interface mode, kept as an adapter of the developer mode.
 *
 * Every screen that still reads "operator" or "builder" reads it from
 * `developerMode`: "builder" when the developer mode is on. Writing "builder"
 * turns it on, "operator" turns it off. Nothing is persisted here; the value
 * lives in `apollia.toml` (see `developerMode.ts`). The readers move to
 * `developerMode` one screen at a time, then this adapter goes.
 */
import { derived, get, type Writable } from "svelte/store";
import { developerMode, setDeveloperMode } from "$lib/stores/developerMode";

/** Display modes the readers of this adapter still name. */
export type UIMode = "operator" | "builder";

const asMode = derived(developerMode, (on): UIMode => (on ? "builder" : "operator"));

/** "builder" when the developer mode is on, "operator" otherwise. */
export const uiMode: Writable<UIMode> = {
  subscribe: asMode.subscribe,
  set(mode: UIMode) {
    void setDeveloperMode(mode === "builder");
  },
  update(fn: (mode: UIMode) => UIMode) {
    void setDeveloperMode(fn(get(asMode)) === "builder");
  },
};
