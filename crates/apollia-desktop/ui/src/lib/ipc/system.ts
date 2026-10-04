/**
 * Typed Tauri IPC wrappers for the System settings sub-page.
 *
 * One thin function per `#[tauri::command]` backing the System page: the CLI
 * symlink install / uninstall. Components call these instead of `invoke()`
 * directly, so command names and payload shapes live in a single place.
 *
 * The self-update check / install wrappers live in `$lib/ipc/updates` (the
 * single source for `check_for_update` / `install_update`). The read-side
 * commands (`get_system_info`, `get_security_posture`, `get_cli_status`,
 * `get_config`) are loaded through the settings store's `settingsLoaders`.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AutomationBoot } from "$lib/automation/types";

/** Install the `apollia` CLI symlink so the runtime is drivable from a shell. */
export async function installCli(): Promise<void> {
  return invoke<void>("install_cli");
}

/** Remove the `apollia` CLI symlink. */
export async function uninstallCli(): Promise<void> {
  return invoke<void>("uninstall_cli");
}

/**
 * The port of the native API this window serves or attaches to, resolved at
 * startup from `[api].port` or from the port a serving daemon recorded.
 */
export async function getApiPort(): Promise<number> {
  return invoke<number>("get_api_port");
}

/** Reveal a session-scoped path in the platform file manager. */
export async function revealSessionPath(
  sessionId: string,
  path: string,
): Promise<void> {
  return invoke<void>("reveal_session_path", { sessionId, path });
}

/**
 * Dev-only: the automation script the runner injected for this session, or
 * `null` outside an automation run. Tree-shaken out of release builds with
 * the rest of the automation harness.
 */
export async function automationScript(): Promise<AutomationBoot | null> {
  return invoke<AutomationBoot | null>("automation_script");
}

/** The Python interpreter setting, as the Advanced section shows it. */
export interface PythonInterpreterSetting {
  /** Absolute path chosen by the operator, or `null` for the bundled one. */
  chosen: string | null;
  /** Absolute path of the bundled interpreter, `null` in a dev tree. */
  bundled: string | null;
  /** Minor version a chosen interpreter has to report. */
  required_minor: number;
}

/** Read the Python interpreter setting. */
export async function getPythonInterpreter(): Promise<PythonInterpreterSetting> {
  return invoke<PythonInterpreterSetting>("get_python_interpreter");
}

/**
 * Choose a Python interpreter, or go back to the bundled one with `null`.
 *
 * The path is checked before it is written; a refusal rejects with the message
 * naming which condition failed.
 */
export async function setPythonInterpreter(path: string | null): Promise<void> {
  return invoke<void>("set_python_interpreter", { path });
}

/** An engine of this machine, as `GET /api/v1/engines` lists it. */
export interface EngineEntry {
  engine: "llama-server" | "vllm-mlx" | "nodejs";
  /** Upstream tag of a store package; `null` for an embedded engine. */
  tag: string | null;
  variant: "cuda" | "vulkan" | "cpu" | "metal" | null;
  path: string;
  source: "embedded" | "downloaded" | "imported";
  /** What the engine states at its source (banner, package metadata); `null` when it says nothing. */
  version: string | null;
  /** True for the build the runtime starts for the model format it serves. */
  in_service: boolean;
}

/** The channel the engines follow, distinct from the application's. */
export type EngineChannel = "stable" | "preview";

/** Whether a newer build of one engine variant exists on the followed channel. */
export interface EngineVariantUpdate {
  engine: EngineEntry["engine"];
  variant: NonNullable<EngineEntry["variant"]>;
  /** Newest tag installed from the store, `null` when none is. */
  installed_tag: string | null;
  /** Newest tag the verified channel offers. */
  latest_tag: string;
  update_available: boolean;
}

/** The dictation engine, whisper.cpp inside the runner the daemon starts. */
export interface DictationEngine {
  path: string;
  /** The whisper.cpp version the runner states; `null` when it says nothing. */
  version: string | null;
}

/** The engine list, with the store's channel and pending updates. */
export interface EnginesResponse {
  engines: EngineEntry[];
  channel: EngineChannel;
  manifest_sequence: number | null;
  updates: EngineVariantUpdate[];
  cuda_proposable: boolean;
  dictation: DictationEngine | null;
}

/** The engines of this machine, each with the version it states. */
export async function listEngines(): Promise<EnginesResponse> {
  return invoke<EnginesResponse>("list_engines");
}

/** The engine store settings, as `PUT /api/v1/engines/settings` answers them. */
export interface EngineSettings {
  channel: EngineChannel;
  base_url: string;
}

/**
 * Follow another engine channel. The engine store of the runtime holds the
 * value; nothing is kept on the interface side.
 */
export async function setEngineChannel(channel: EngineChannel): Promise<EngineSettings> {
  return invoke<EngineSettings>("set_engine_channel", { channel });
}

/** What a verified check of the followed channel answers. */
export interface EngineCheckResponse {
  channel: EngineChannel;
  manifest_sequence: number;
  updates: EngineVariantUpdate[];
}

/**
 * Fetch and verify the followed channel, and learn per variant whether a newer
 * engine exists. Rejects with the runtime's sentence when the channel cannot
 * be reached or verified.
 */
export async function checkEngineUpdates(): Promise<EngineCheckResponse> {
  return invoke<EngineCheckResponse>("check_engine_updates");
}

/** The video memory in use, or why no figure is given. */
export type VramUsage =
  | { kind: "dedicated"; used_gb: number; total_gb: number }
  | { kind: "unified" }
  | { kind: "unmeasured" };

/** One measurement of the machine the runtime runs on. */
export interface ResourceUsage {
  ram_used_gb: number;
  ram_total_gb: number;
  /** Processor load over every core, from 0 to 100. */
  cpu_percent: number;
  vram: VramUsage;
}

/** Measure the machine once. Nothing samples between two calls. */
export async function getResourceUsage(): Promise<ResourceUsage> {
  return invoke<ResourceUsage>("get_resource_usage");
}
