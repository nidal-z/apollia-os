import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { readConfigFile, type ConfigFileView } from "$lib/ipc/configFile";
import {
  listEngines,
  getPythonInterpreter,
  type EnginesResponse,
  type PythonInterpreterSetting,
} from "$lib/ipc/system";
import { getHardwareProfile, type HardwareProfile } from "$lib/ipc/modelHub";
import type {
  SystemInfo,
  SecurityPosture,
  SttModelInfo,
  SttConfigView,
  LlmBackendConfig,
  CliStatus,
} from "$lib/types";

/**
 * The canonical settings sub-routes, in the order of the navigation column:
 * the "Settings" group, then the "System" group, then the sections that leave
 * the settings for their own screens and stay listed last until they do. The
 * built-in tools already live on the Connections screen and the permission
 * rules on the Authorizations screen.
 */
export type SettingsSubRoute =
  | "general"
  | "appearance"
  | "profile"
  | "chat"
  | "stt"
  | "model-defaults"
  | "integrations"
  | "notifications"
  | "developer"
  | "configuration"
  | "engines"
  | "hardware"
  | "llm"
  | "model-hub";

export const SETTINGS_SUB_ROUTES: SettingsSubRoute[] = [
  "general",
  "appearance",
  "profile",
  "chat",
  "stt",
  "model-defaults",
  "integrations",
  "notifications",
  "developer",
  "configuration",
  "engines",
  "hardware",
  "llm",
  "model-hub",
];

/**
 * Former sections and the section that now holds their content. A caller that
 * still names one of them (palette action, user menu, help link) lands on the
 * section that carries the moved content rather than on the default.
 */
export const MOVED_SETTINGS_SUB_ROUTES: Readonly<Record<string, SettingsSubRoute>> = {
  about: "general",
  help: "general",
  danger: "general",
  shortcuts: "chat",
  system: "hardware",
};

/**
 * The sections that exist only with the developer mode on: absent from the
 * column, the palette and the window with the mode off.
 */
export const DEVELOPER_ONLY_SETTINGS_SUB_ROUTES: readonly SettingsSubRoute[] = ["configuration"];

/** Whether a section is shown with the developer mode in this position. */
export function settingsSubRouteVisible(route: SettingsSubRoute, developerMode: boolean): boolean {
  return developerMode || !DEVELOPER_ONLY_SETTINGS_SUB_ROUTES.includes(route);
}

/**
 * Key of a settings form in the unsaved-changes store: a settings section, or
 * the capture policy that moved to the Observability screen and keeps its form.
 */
export type SettingsFormRoute = SettingsSubRoute | "observability";

export const DEFAULT_SETTINGS_SUB_ROUTE: SettingsSubRoute = "general";

/** Active settings sub-route. Persists across `<Settings />` remounts. */
export const settingsSubRoute = writable<SettingsSubRoute>(DEFAULT_SETTINGS_SUB_ROUTE);

export function isSettingsSubRoute(value: string | null | undefined): value is SettingsSubRoute {
  return !!value && (SETTINGS_SUB_ROUTES as string[]).includes(value);
}

/** The section a name resolves to: itself, its new home if it moved, or the default. */
export function resolveSettingsSubRoute(route: string | null | undefined): SettingsSubRoute {
  if (isSettingsSubRoute(route)) return route;
  if (route && Object.hasOwn(MOVED_SETTINGS_SUB_ROUTES, route)) {
    return MOVED_SETTINGS_SUB_ROUTES[route];
  }
  return DEFAULT_SETTINGS_SUB_ROUTE;
}

/** Navigate to a sub-route, falling back to `general` if unknown. */
export function goToSettingsSubRoute(route: string | null | undefined): SettingsSubRoute {
  const target = resolveSettingsSubRoute(route);
  if (get(settingsSubRoute) !== target) {
    settingsSubRoute.set(target);
  }
  return target;
}

/**
 * Install a guard that decides whether a sub-route change is allowed to
 * commit. Returning `false` cancels the navigation - the guard is then
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
  const target = resolveSettingsSubRoute(route);
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

export const configStore = writable<LoadState<ConfigFileView>>(loadState());
export const systemInfoStore = writable<LoadState<SystemInfo>>(loadState());
export const securityPostureStore = writable<LoadState<SecurityPosture>>(loadState());
export const cliStatusStore = writable<LoadState<CliStatus>>(loadState());
export const sttModelsStore = writable<LoadState<SttModelInfo[]>>(loadState());
export const sttConfigStore = writable<LoadState<SttConfigView>>(loadState());
export const llmBackendsStore = writable<LoadState<LlmBackendConfig[]>>(loadState());
/** System > Engines: the engines of this machine and their stated versions. */
export const enginesStore = writable<LoadState<EnginesResponse>>(loadState());
/** System > Hardware: the single hardware probe of the inference. */
export const hardwareStore = writable<LoadState<HardwareProfile>>(loadState());
/** System > Hardware, developer mode: the Python interpreter setting. */
export const pythonInterpreterStore = writable<LoadState<PythonInterpreterSetting>>(loadState());

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
    loadOnce(configStore, readConfigFile, force),
  systemInfo: (force = false) =>
    loadOnce(systemInfoStore, () => invoke<SystemInfo>("get_system_info"), force),
  securityPosture: (force = false) =>
    loadOnce(
      securityPostureStore,
      () => invoke<SecurityPosture>("get_security_posture"),
      force,
    ),
  cliStatus: (force = false) =>
    loadOnce(cliStatusStore, () => invoke<CliStatus>("get_cli_status"), force),
  sttModels: (force = false) =>
    loadOnce(sttModelsStore, () => invoke<SttModelInfo[]>("list_stt_models"), force),
  sttConfig: (force = false) =>
    loadOnce(sttConfigStore, () => invoke<SttConfigView>("get_stt_config"), force),
  llmBackends: (force = false) =>
    loadOnce(llmBackendsStore, () => invoke<LlmBackendConfig[]>("list_llm_backends"), force),
  engines: (force = false) => loadOnce(enginesStore, listEngines, force),
  hardware: (force = false) => loadOnce(hardwareStore, getHardwareProfile, force),
  pythonInterpreter: (force = false) =>
    loadOnce(pythonInterpreterStore, getPythonInterpreter, force),
};
