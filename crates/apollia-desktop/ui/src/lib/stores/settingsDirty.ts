import { writable, derived, get } from "svelte/store";
import type { SettingsSubRoute } from "./settings";

export type SavingState = "idle" | "saving" | "saved" | "error";

export interface RouteDirtyState {
  dirty: boolean;
  savingState: SavingState;
  lastError: string | null;
  /** Last explicit-mode save call; used by nav guard to decide whether to prompt. */
  autoSave: "debounced" | "explicit";
}

export type SettingsDirtyMap = Partial<Record<SettingsSubRoute, RouteDirtyState>>;

const DEFAULT: RouteDirtyState = {
  dirty: false,
  savingState: "idle",
  lastError: null,
  autoSave: "explicit",
};

export const settingsDirtyStore = writable<SettingsDirtyMap>({});

/** Per-route scroll preservation. */
export const settingsScrollStore = writable<Partial<Record<SettingsSubRoute, number>>>({});

/** True when any `explicit` sub-route has unsaved changes. */
export const hasExplicitDirty = derived(settingsDirtyStore, ($m) =>
  Object.values($m).some((s) => s?.dirty && s.autoSave === "explicit"),
);

export function setRouteState(route: SettingsSubRoute, patch: Partial<RouteDirtyState>): void {
  settingsDirtyStore.update((m) => {
    const prev = m[route] ?? DEFAULT;
    return { ...m, [route]: { ...prev, ...patch } };
  });
}

export function clearRoute(route: SettingsSubRoute): void {
  settingsDirtyStore.update((m) => {
    const next = { ...m };
    delete next[route];
    return next;
  });
}

export function getRouteState(route: SettingsSubRoute): RouteDirtyState {
  return get(settingsDirtyStore)[route] ?? DEFAULT;
}

export function setScroll(route: SettingsSubRoute, y: number): void {
  settingsScrollStore.update((m) => ({ ...m, [route]: y }));
}

export function getScroll(route: SettingsSubRoute): number {
  return get(settingsScrollStore)[route] ?? 0;
}
