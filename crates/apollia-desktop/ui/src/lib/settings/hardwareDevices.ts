// Settings > Hardware: the graphics devices one by one, the ones held back,
// and the text "Copy the information" puts on the clipboard.
//
// The devices held back are one named setting of `apollia.toml`,
// `llm.devices.disabled`: a list of the identifiers the engine prints. The
// runtime reads it at every load and at every read of the hardware, so the
// profile read after a write already leaves out the memory of a device turned
// off.

import type { GpuDevice, HardwareProfile } from "$lib/ipc/modelHub";
import type { SettingSource } from "$lib/ipc/settings";

/** The dotted key of the devices held back, as `apollia.toml` names it. */
export const DISABLED_DEVICES_KEY = "llm.devices.disabled";

/** Why a device has no working switch; `null` when it has one. */
export type SwitchLock = "single_unified" | "unlisted" | "policy" | null;

/** One line of the list. */
export interface GpuRow {
  /** Stable key of the line. */
  key: string;
  id: string | null;
  name: string;
  api: string;
  vramGb: number;
  enabled: boolean;
  /** Whether the line shows a switch at all. */
  hasSwitch: boolean;
  /** Why the switch cannot be moved, `null` when it can. */
  lock: SwitchLock;
}

/**
 * The lines of the list, in the order of the engine.
 *
 * A machine with unified memory shows its single device with a switch that
 * stays on: there is nothing else to load onto, and the MLX engine has no
 * device to choose. A device the engine did not list has no identifier to
 * hand it, so it is shown without a switch. A list the organization policy
 * fixes keeps its switches where they are.
 */
export function gpuRows(profile: HardwareProfile | null, source: SettingSource): GpuRow[] {
  if (!profile) return [];
  const unified = profile.accelerator.kind === "apple_silicon";
  return (profile.devices ?? []).map((device, index) => {
    let lock: SwitchLock = null;
    if (unified) lock = "single_unified";
    else if (device.id === null) lock = "unlisted";
    else if (source === "policy") lock = "policy";
    return {
      key: device.id ?? `unlisted-${index}`,
      id: device.id,
      name: device.name,
      api: device.api,
      vramGb: device.vram_gb,
      enabled: unified ? true : device.enabled,
      hasSwitch: lock !== "unlisted",
      lock,
    };
  });
}

/** The interface the listed devices share, or `null` when they differ or none is listed. */
export function sharedApi(rows: readonly GpuRow[]): string | null {
  const apis = new Set(rows.map((row) => row.api));
  return apis.size === 1 ? rows[0].api : null;
}

/** The identifiers held back once the device `id` is turned `on` or off. */
export function nextDisabled(devices: readonly GpuDevice[], id: string, on: boolean): string[] {
  return devices
    .filter((device) => device.id !== null)
    .filter((device) => (device.id === id ? !on : !device.enabled))
    .map((device) => device.id as string);
}

/** Whether "Back to the default" has something to undo. */
export function canRestoreDefault(rows: readonly GpuRow[]): boolean {
  return rows.some((row) => row.lock === null && !row.enabled);
}

/** One engine of the copied text. */
export interface InfoEngine {
  name: string;
  version: string | null;
}

/** What the copied text is made of. */
export interface HardwareInfoInput {
  version: string | null;
  os: string | null;
  arch: string | null;
  hardware: HardwareProfile | null;
  engines: readonly InfoEngine[];
  /** The paths, given in developer mode only; `null` leaves them out. */
  paths: { python: string | null; config: string | null; dataDir: string | null } | null;
}

/** The labels of the copied text, already translated. */
export interface HardwareInfoLabels {
  os: string;
  processor: string;
  cores: string;
  memory: string;
  gpu: string;
  noGpu: string;
  off: string;
  engines: string;
  unknown: string;
  python: string;
  config: string;
  dataDir: string;
}

const gb = (value: number): string => `${value.toFixed(value >= 100 ? 0 : 1)} GB`;

/**
 * The description of the machine as plain text: the version of the
 * application, the operating system, the processor, the memory, each graphics
 * device, each engine with its version, and the paths when they are given.
 */
export function hardwareInfoText(input: HardwareInfoInput, labels: HardwareInfoLabels): string {
  const { hardware } = input;
  const lines = [
    `Apollia OS ${input.version ? `v${input.version}` : labels.unknown}`,
    `${labels.os}: ${input.os ?? labels.unknown}`,
  ];
  if (hardware) {
    const arch = input.arch ? `, ${input.arch}` : "";
    lines.push(
      `${labels.processor}: ${hardware.cpu_model}${arch}, ${hardware.cpu_cores} ${labels.cores}`,
      `${labels.memory}: ${gb(hardware.total_ram_gb)}`,
    );
    const devices = hardware.devices ?? [];
    if (devices.length === 0) lines.push(`${labels.gpu}: ${labels.noGpu}`);
    for (const device of devices) {
      const id = device.id ? `, ${device.id}` : "";
      const off = device.enabled ? "" : `, ${labels.off}`;
      lines.push(`${labels.gpu}: ${device.name}, ${gb(device.vram_gb)}, ${device.api}${id}${off}`);
    }
  }
  if (input.engines.length > 0) {
    lines.push(`${labels.engines}:`);
    for (const engine of input.engines) {
      lines.push(`  ${engine.name}: ${engine.version ?? labels.unknown}`);
    }
  }
  if (input.paths) {
    const { python, config, dataDir } = input.paths;
    if (python) lines.push(`${labels.python}: ${python}`);
    if (config) lines.push(`${labels.config}: ${config}`);
    if (dataDir) lines.push(`${labels.dataDir}: ${dataDir}`);
  }
  return lines.join("\n");
}
