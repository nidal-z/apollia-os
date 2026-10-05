<script lang="ts">
  /**
   * GpuDevices - System > Hardware: the graphics devices of the machine, one
   * line each, with the switch that holds a device back from the local
   * engine, "Back to the default", which turns every device on again, and the
   * button that opens Hardware in a window of its own.
   *
   * The list is the engine's own, read with the hardware profile. A switch
   * writes the named setting `llm.devices.disabled` and reads the profile
   * again: the runtime leaves the memory of a device held back out of every
   * estimate, and out of the `--device` list of the next model loaded.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ExternalLink, MonitorCog, RotateCcw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast";
  import { getResolvedSetting, setSetting, type SettingSource } from "$lib/ipc/settings";
  import { openHardwareWindow } from "$lib/ipc/system";
  import type { HardwareProfile } from "$lib/ipc/modelHub";
  import {
    DISABLED_DEVICES_KEY,
    canRestoreDefault,
    gpuRows,
    nextDisabled,
    sharedApi,
  } from "$lib/settings/hardwareDevices";
  import SettingsSection from "../SettingsSection.svelte";
  import PolicyLockedNote from "../model-defaults/PolicyLockedNote.svelte";

  interface Props {
    hardware: HardwareProfile | null;
    /** Reads the hardware profile again, after a switch moved. */
    onreload: () => Promise<unknown>;
    /** True inside the window of its own: the button that opens it is left out. */
    windowed?: boolean;
    /** Where the list of devices held back comes from, before the shell answers. */
    initialSource?: SettingSource;
  }

  let { hardware, onreload, windowed = false, initialSource = "default" }: Props = $props();

  // The shell's answer replaces this seed once mounted.
  // svelte-ignore state_referenced_locally
  let source = $state<SettingSource>(initialSource);
  let busy = $state(false);

  const rows = $derived(gpuRows(hardware, source));
  const api = $derived(sharedApi(rows));
  const gb = (value: number): string =>
    $t("settings.hardware.gb", { values: { n: value.toFixed(value >= 100 ? 0 : 1) } });

  onMount(() => {
    void getResolvedSetting(DISABLED_DEVICES_KEY)
      .then((resolved) => (source = resolved.source))
      .catch(() => (source = "default"));
  });

  /** Write the devices held back, then read the hardware the estimates now see. */
  async function write(disabled: string[]): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      const resolved = await setSetting(DISABLED_DEVICES_KEY, disabled);
      source = resolved.source;
      await onreload();
    } catch {
      addToast($t("settings.hardware.gpu_write_failed"), "error");
      await onreload();
    } finally {
      busy = false;
    }
  }

  function toggle(id: string | null, on: boolean): void {
    if (!hardware || id === null) return;
    void write(nextDisabled(hardware.devices, id, on));
  }

  async function openWindow(): Promise<void> {
    try {
      await openHardwareWindow();
    } catch {
      addToast($t("settings.hardware.window_failed"), "error");
    }
  }
</script>

<SettingsSection title={$t("settings.hardware.gpu")} bodyLayout="flush" data-testid="hardware-gpu">
  {#snippet icon()}<MonitorCog size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <div class="flex flex-wrap items-center justify-end gap-2">
      <Button
        variant="outline"
        size="sm"
        disabled={busy || !canRestoreDefault(rows)}
        onclick={() => void write([])}
        data-testid="hardware-gpu-restore-default"
      >
        {#snippet icon()}<RotateCcw size={13} />{/snippet}
        {$t("settings.hardware.gpu_restore_default")}
      </Button>
      {#if !windowed}
        <Button
          variant="outline"
          size="sm"
          onclick={() => void openWindow()}
          data-testid="hardware-open-window"
        >
          {#snippet icon()}<ExternalLink size={13} />{/snippet}
          {$t("settings.hardware.open_window")}
        </Button>
      {/if}
    </div>
  {/snippet}

  {#if rows.length === 0}
    <p class="pb-3 text-body-sm text-muted-foreground" data-testid="hardware-gpu-none">
      {$t("settings.hardware.no_gpu")}
    </p>
  {:else}
    <p class="pb-1 text-body-sm font-medium text-foreground" data-testid="hardware-gpu-count">
      {#if api}
        {$t("settings.hardware.gpu_count_api", { values: { n: rows.length, api } })}
      {:else}
        {$t("settings.hardware.gpu_count", { values: { n: rows.length } })}
      {/if}
    </p>
    {#if source === "policy"}
      <PolicyLockedNote labelKey="settings.policy_locked" data-testid="hardware-gpu-policy" />
    {/if}
    <ul class="flex flex-col" data-testid="hardware-gpu-list">
      {#each rows as row (row.key)}
        <li
          class="flex items-start justify-between gap-4 border-b border-border/50 py-3 last:border-b-0"
          data-testid="hardware-gpu-row"
          data-device={row.id ?? ""}
          data-enabled={row.enabled}
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-body-sm text-foreground" data-testid="hardware-gpu-name">
              {row.name}
            </p>
            <p class="text-caption text-muted-foreground">
              {#if row.id}
                {$t("settings.hardware.gpu_line_id", {
                  values: { size: gb(row.vramGb), api: row.api, id: row.id },
                })}
              {:else}
                {$t("settings.hardware.gpu_vram", {
                  values: { size: gb(row.vramGb), backend: row.api },
                })}
              {/if}
            </p>
            {#if row.lock === "single_unified"}
              <p class="mt-1 max-w-md text-caption text-muted-foreground" data-testid="hardware-gpu-single-note">
                {$t("settings.hardware.gpu_single_note")}
              </p>
            {:else if row.lock === "unlisted"}
              <p class="mt-1 max-w-md text-caption text-muted-foreground" data-testid="hardware-gpu-unlisted-note">
                {$t("settings.hardware.gpu_unlisted_note")}
              </p>
            {/if}
          </div>
          {#if row.hasSwitch}
            <Toggle
              checked={row.enabled}
              disabled={row.lock !== null || busy}
              onchange={(on) => toggle(row.id, on)}
              aria-label={$t("settings.hardware.gpu_switch_label", { values: { name: row.name } })}
              data-testid="hardware-gpu-switch"
            />
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    <p class="text-caption text-muted-foreground">{$t("settings.hardware.gpu_off_note")}</p>
  {/snippet}
</SettingsSection>
