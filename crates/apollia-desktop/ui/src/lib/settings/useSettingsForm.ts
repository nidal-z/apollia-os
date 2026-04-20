import { onDestroy, tick } from "svelte";
import type { SettingsSubRoute } from "$lib/stores/settings";
import { setRouteState, getRouteState, clearRoute } from "$lib/stores/settingsDirty";

export type AutoSaveMode = "debounced" | "explicit";

export interface UseSettingsFormOptions<T> {
  /** Identifier of the owning sub-route (used for dirty/savingState tracking). */
  route: SettingsSubRoute;
  /** Initial snapshot — used as baseline for dirty detection. */
  initial: T;
  /** Persist handler. Must throw on failure. */
  onSave: (values: T) => Promise<void>;
  /** `debounced` = silent save 1000ms after last edit; `explicit` = user clicks Save / Cmd+S. */
  autoSave: AutoSaveMode;
  /** Debounce delay for `debounced` mode. Default 1000ms. */
  debounceMs?: number;
  /** Called after a successful save (for toast/banner). */
  onSaved?: () => void;
  /** Called after a failed save. */
  onError?: (err: unknown) => void;
}

export interface SettingsForm<T> {
  /** Live, reactive values — Svelte 5 `$state` proxy wrapper. */
  values: T;
  /** Set a single field and mark the form dirty. */
  setField: <K extends keyof T>(key: K, value: T[K]) => void;
  /** Sync the full live value (e.g. from a Svelte-reactive external `$state`). */
  sync: (next: T) => void;
  /** Replace entire snapshot (e.g. when the backend returns a fresh view). */
  replace: (next: T) => void;
  /** Whether the current values differ from the saved snapshot. */
  dirty: () => boolean;
  /** Trigger an immediate save (bypasses debounce). Returns true on success. */
  save: () => Promise<boolean>;
  /** Reset to the last saved snapshot. */
  reset: () => void;
}

function deepEqual(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/**
 * Standardised settings-form hook. Tracks dirty state, drives savingState
 * transitions on the shared `settingsDirtyStore`, and — in `debounced` mode —
 * auto-persists changes 1 second after the last edit.
 *
 * Usage (Svelte 5):
 *
 * ```ts
 * const form = useSettingsForm({
 *   route: "stt",
 *   initial: sttConfig,
 *   autoSave: "explicit",
 *   onSave: (v) => invoke("update_stt_config", { config: v }),
 * });
 * ```
 */
export function useSettingsForm<T extends object>(
  options: UseSettingsFormOptions<T>,
): SettingsForm<T> {
  const { route, onSave, autoSave, onSaved, onError } = options;
  const debounceMs = options.debounceMs ?? 1000;

  let saved: T = clone(options.initial);
  const live: { value: T } = { value: clone(options.initial) };
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let destroyed = false;

  setRouteState(route, { dirty: false, savingState: "idle", lastError: null, autoSave });

  function isDirty(): boolean {
    return !deepEqual(live.value, saved);
  }

  function refreshDirty(): void {
    const d = isDirty();
    const prev = getRouteState(route);
    if (prev.dirty !== d) {
      setRouteState(route, { dirty: d });
    }
  }

  async function performSave(silent: boolean): Promise<boolean> {
    if (!isDirty()) return true;
    const snapshot = clone(live.value);
    setRouteState(route, { savingState: "saving", lastError: null });
    try {
      await onSave(snapshot);
      saved = snapshot;
      if (destroyed) return true;
      setRouteState(route, { savingState: "saved", dirty: isDirty(), lastError: null });
      onSaved?.();
      if (silent) {
        setTimeout(() => {
          const s = getRouteState(route);
          if (s.savingState === "saved") setRouteState(route, { savingState: "idle" });
        }, 2000);
      }
      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setRouteState(route, { savingState: "error", lastError: msg });
      onError?.(err);
      return false;
    }
  }

  function scheduleDebounced(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      void performSave(true);
    }, debounceMs);
  }

  function setField<K extends keyof T>(key: K, value: T[K]): void {
    (live.value as T)[key] = value;
    refreshDirty();
    if (autoSave === "debounced") scheduleDebounced();
  }

  function sync(next: T): void {
    live.value = clone(next);
    refreshDirty();
    if (autoSave === "debounced" && isDirty()) scheduleDebounced();
  }

  function replace(next: T): void {
    live.value = clone(next);
    saved = clone(next);
    setRouteState(route, { dirty: false, savingState: "idle", lastError: null });
  }

  async function save(): Promise<boolean> {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (getRouteState(route).savingState === "saving") return false;
    return performSave(false);
  }

  function reset(): void {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    live.value = clone(saved);
    setRouteState(route, { dirty: false, savingState: "idle", lastError: null });
  }

  onDestroy(() => {
    destroyed = true;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
      if (autoSave === "debounced" && isDirty() && getRouteState(route).savingState !== "saving") {
        void performSave(true);
      }
    }
    if (!isDirty()) clearRoute(route);
  });

  return {
    get values() {
      return live.value;
    },
    setField,
    sync,
    replace,
    dirty: isDirty,
    save,
    reset,
  } as SettingsForm<T>;
}

/** Re-export for consumers that want to tick before restoring scroll. */
export { tick };

/**
 * Register an active settings form with the parent `<Settings />` page so that
 * the global Cmd+S / 3-option dialog can drive its `save` / `reset` handlers.
 *
 * Implementation note: we deliberately use a `window`-level hook rather than a
 * Svelte context, because sub-routes are loaded via dynamic `import()` and do
 * not share a live parent context at script evaluation time.
 */
type RegisterFn = (
  route: SettingsSubRoute,
  handle: { save: () => Promise<boolean>; reset: () => void },
) => () => void;

export function registerSettingsForm<T>(
  route: SettingsSubRoute,
  form: SettingsForm<T>,
): () => void {
  if (typeof window === "undefined") return () => {};
  const register = (window as unknown as { __apolliaRegisterSettingsForm?: RegisterFn })
    .__apolliaRegisterSettingsForm;
  if (!register) return () => {};
  return register(route, { save: form.save, reset: form.reset });
}

