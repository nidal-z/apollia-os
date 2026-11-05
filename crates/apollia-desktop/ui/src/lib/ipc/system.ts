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
 * `read_config_file`) are loaded through the settings store's `settingsLoaders`.
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

/** A build of an engine for one kind of hardware, by its wire name. */
export type EngineVariant = "cuda-12" | "cuda-13" | "rocm" | "vulkan" | "cpu" | "metal";

/** An engine of this machine, as `GET /api/v1/engines` lists it. */
export interface EngineEntry {
  engine: "llama-server" | "vllm-mlx" | "nodejs";
  /** Upstream tag of a store package; `null` for an embedded engine. */
  tag: string | null;
  variant: EngineVariant | null;
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
  /** Bytes installing or updating to the newest build downloads, as the verified list states. */
  size: number;
  /** Day the newest build was published, `YYYY-MM-DD`; `null` when the channel does not say. */
  latest_released: string | null;
  /** Day the installed build was published; `null` when unknown or when none is installed. */
  installed_released: string | null;
}

/** The dictation engine, whisper.cpp inside the runner the daemon starts. */
export interface DictationEngine {
  path: string;
  /** The whisper.cpp version the runner states; `null` when it says nothing. */
  version: string | null;
}

/** The maker of a graphics card. */
export type EngineCardVendor = "nvidia" | "amd" | "intel" | "other";

/** One graphics card of the machine, as the engine list sees it. */
export interface EngineHardwareCard {
  vendor: EngineCardVendor;
  name: string;
  /** Dedicated memory in GiB, `null` when unknown. */
  vram_gb: number | null;
  /** The NVIDIA driver version, `580.65.06`; `null` otherwise. */
  driver_version: string | null;
  /** The NVIDIA compute capability, `8.9`. */
  compute_capability: string | null;
  /** The gfx target of an AMD card, `gfx1100`. */
  gfx_target: string | null;
  /** Whether `/dev/kfd` opens; AMD under Linux only. */
  kfd_accessible: boolean | null;
  /** True when the driver did not answer within the probe limit. */
  timed_out: boolean;
}

/** The cards of the machine and what is installed around them. */
export interface EngineHardware {
  /** Every card, NVIDIA ones first. */
  cards: EngineHardwareCard[];
  vulkan_loader: boolean;
  /** Whether the host's ROCm runtime is installed; Linux only. */
  rocm_runtime: boolean;
}

/** One package the verified channel offers for this platform, installed or not. */
export interface EngineAvailablePackage {
  engine: EngineEntry["engine"];
  tag: string;
  variant: EngineVariant;
  /** Archive size in bytes. */
  size: number;
  /** Day the build was published, `YYYY-MM-DD`; `null` when the channel does not say. */
  released: string | null;
  /** Whether this exact build is installed here. */
  installed: boolean;
}

/** How well a variant suits this machine, as the runtime rates it. */
export type EngineVerdict = "recommended" | "compatible" | "incompatible";

/** Why a variant is not recommended, as a code the interface translates. */
export type EngineRatingReason =
  | { code: "driver_too_old"; have: string; need: string }
  | { code: "arch_dropped_in_cuda13"; cc: string }
  | { code: "gfx_not_in_build"; gfx: string }
  | { code: "no_vulkan_loader" }
  | { code: "no_nvidia_driver" }
  | { code: "not_built_for_platform" }
  | { code: "no_gpu" }
  | { code: "engine_sees_no_device" }
  | { code: "no_rocm_runtime" };

/** The verdict of the runtime on one `llama-server` variant. */
export interface EngineRating {
  variant: EngineVariant;
  verdict: EngineVerdict;
  /** Why it is not recommended, or the caveat of a recommendation; `null` when nothing is to say. */
  reason: EngineRatingReason | null;
}

/** The engine list, with the store's channel and pending updates. */
export interface EnginesResponse {
  engines: EngineEntry[];
  channel: EngineChannel;
  manifest_sequence: number | null;
  updates: EngineVariantUpdate[];
  dictation: DictationEngine | null;
  /** The cards of the machine; `null` when the probe did not answer in time. */
  hardware: EngineHardware | null;
  /** The packages the verified channel offers for this platform; empty until a manifest verified. */
  available: EngineAvailablePackage[];
  /** Per `llama-server` variant, whether it suits this machine and why; empty when unknown. */
  ratings: EngineRating[];
  /** Whether this build can verify the engine channel; false for a build without the distribution key. */
  verifiable: boolean;
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

/** What asking for the hardware window did: a new one, or the live one brought forward. */
export type HardwareWindowOutcome = "created" | "focused";

/** Open System > Hardware in a window of its own, or bring that window forward. */
export async function openHardwareWindow(): Promise<HardwareWindowOutcome> {
  return invoke<HardwareWindowOutcome>("open_hardware_window");
}

/** Measure the machine once. Nothing samples between two calls. */
export async function getResourceUsage(): Promise<ResourceUsage> {
  return invoke<ResourceUsage>("get_resource_usage");
}
