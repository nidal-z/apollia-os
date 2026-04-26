import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type {
  ApollaConfigView,
  SystemInfo,
  SttModelInfo,
  SttConfigView,
  LlmBackendConfig,
  CliStatus,
} from "$lib/types";

/** The canonical settings sub-routes. */
export type SettingsSubRoute =
  | "appearance"
  | "profile"
  | "stt"
  | "llm"
  | "model-hub"
  | "configuration"
  | "tools"
  | "permissions"
  | "system"
  | "memories"
  | "shortcuts"
  | "danger";

export const SETTINGS_SUB_ROUTES: SettingsSubRoute[] = [
  "appearance",
  "profile",
  "stt",
  "llm",
  "model-hub",
  "configuration",
  "tools",
  "permissions",
  "system",
  "memories",
  "shortcuts",
  "danger",
];

export const DEFAULT_SETTINGS_SUB_ROUTE: SettingsSubRoute = "appearance";

/** Active settings sub-route. Persists across `<Settings />` remounts. */
export const settingsSubRoute = writable<SettingsSubRoute>(DEFAULT_SETTINGS_SUB_ROUTE);

export function isSettingsSubRoute(value: string | null | undefined): value is SettingsSubRoute {
  return !!value && (SETTINGS_SUB_ROUTES as string[]).includes(value);
}

/** Navigate to a sub-route, falling back to `appearance` if unknown. */
export function goToSettingsSubRoute(route: string | null | undefined): SettingsSubRoute {
  const target = isSettingsSubRoute(route) ? route : DEFAULT_SETTINGS_SUB_ROUTE;
  if (get(settingsSubRoute) !== target) {
    settingsSubRoute.set(target);
  }
  return target;
}

/**
 * Install a guard that decides whether a sub-route change is allowed to
 * commit. Returning `false` cancels the navigation — the guard is then
 * expected to either resolve the blocker (discard/save) and re-navigate, or
 * restore the store to its previous value (which it already is, since the
 * guard runs before the store is updated). Returns an uninstall function.
 */
type NavigationGuard = (from: SettingsSubRoute, to: SettingsSubRoute) => boolean;
let navigationGuard: NavigationGuard | null = null;

export function installSettingsNavigationGuard(guard: NavigationGuard): () => void {
  navigationGuard = guard;
  return () => {
    if (navigationGuard === guard) navigationGuard = null;
  };
}

/** Navigate, consulting the installed guard. */
export function requestSettingsSubRoute(route: string | null | undefined): SettingsSubRoute | null {
  const current = get(settingsSubRoute);
  const target = isSettingsSubRoute(route) ? route : DEFAULT_SETTINGS_SUB_ROUTE;
  if (target === current) return target;
  if (navigationGuard && !navigationGuard(current, target)) return null;
  settingsSubRoute.set(target);
  return target;
}

// ─── Shared data stores ──────────────────────────────────────────────
// Each sub-route fetches its own data lazily. Stores are shared so that
// switching between sub-routes does not re-fetch already-loaded data.

type LoadState<T> = {
  data: T | null;
  loading: boolean;
  error: string | null;
  loaded: boolean;
};

function loadState<T>(): LoadState<T> {
  return { data: null, loading: false, error: null, loaded: false };
}

export const configStore = writable<LoadState<ApollaConfigView>>(loadState());
export const systemInfoStore = writable<LoadState<SystemInfo>>(loadState());
export const cliStatusStore = writable<LoadState<CliStatus>>(loadState());
export const sttModelsStore = writable<LoadState<SttModelInfo[]>>(loadState());
export const sttConfigStore = writable<LoadState<SttConfigView>>(loadState());
export const llmBackendsStore = writable<LoadState<LlmBackendConfig[]>>(loadState());

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

async function loadOnce<T>(
  store: ReturnType<typeof writable<LoadState<T>>>,
  loader: () => Promise<T>,
  force = false,
): Promise<void> {
  const current = get(store);
  if (!force && (current.loaded || current.loading)) return;
  store.set({ ...current, loading: true, error: null });
  try {
    const data = await loader();
    store.set({ data, loading: false, error: null, loaded: true });
  } catch (err) {
    store.set({ data: current.data, loading: false, error: toErrorMessage(err), loaded: current.loaded });
  }
}

export const settingsLoaders = {
  config: (force = false) =>
    loadOnce(configStore, () => invoke<ApollaConfigView>("get_config"), force),
  systemInfo: (force = false) =>
    loadOnce(systemInfoStore, () => invoke<SystemInfo>("get_system_info"), force),
  cliStatus: (force = false) =>
    loadOnce(cliStatusStore, () => invoke<CliStatus>("get_cli_status"), force),
  sttModels: (force = false) =>
    loadOnce(sttModelsStore, () => invoke<SttModelInfo[]>("list_stt_models"), force),
  sttConfig: (force = false) =>
    loadOnce(sttConfigStore, () => invoke<SttConfigView>("get_stt_config"), force),
  llmBackends: (force = false) =>
    loadOnce(llmBackendsStore, () => invoke<LlmBackendConfig[]>("list_llm_backends"), force),
};
