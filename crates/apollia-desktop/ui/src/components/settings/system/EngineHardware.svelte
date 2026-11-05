<script lang="ts" module>
  import type { EngineCardVendor, EngineHardware } from "$lib/ipc/system";

  /** The maker names, proper nouns; `null` for a maker the catalogue words. */
  export const VENDOR_NAMES: Record<EngineCardVendor, string | null> = {
    nvidia: "NVIDIA",
    amd: "AMD",
    intel: "Intel",
    other: null,
  };

  /** What the card of the page has to say about the machine. */
  export type HardwareState =
    /** The probe did not answer before the engine list was given. */
    | { kind: "unknown" }
    /** No graphics card: an Apple chip, or the processor alone. */
    | { kind: "no_card"; apple: boolean }
    | { kind: "cards"; hardware: EngineHardware };

  /**
   * The state of the card, from `hardware` of the engine list. An Apple chip
   * shows no discrete card; the hardware probe of the inference names it.
   */
  export function hardwareState(
    hardware: EngineHardware | null,
    acceleratorKind: string | null,
  ): HardwareState {
    if (!hardware) return { kind: "unknown" };
    if (hardware.cards.length === 0) {
      return { kind: "no_card", apple: acceleratorKind === "apple_silicon" };
    }
    return { kind: "cards", hardware };
  }
</script>

<script lang="ts">
  /**
   * EngineHardware - System > Engines, "Hardware": the graphics cards the
   * engine list was rated against, each with its maker, its driver and its
   * memory, and whether a Vulkan loader is installed. One source: `hardware`
   * of `GET /api/v1/engines`. Nothing here probes.
   */
  import { t } from "svelte-i18n";
  import { MonitorCog } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    hardware: EngineHardware | null;
    /** `accelerator.kind` of the hardware probe; `null` until it answers. */
    acceleratorKind: string | null;
  }

  let { hardware, acceleratorKind }: Props = $props();

  const view = $derived(hardwareState(hardware, acceleratorKind));

  const gb = (value: number): string =>
    $t("settings.hardware.gb", { values: { n: value.toFixed(value >= 100 ? 0 : 1) } });
</script>

<SettingsSection
  title={$t("settings.engines.hardware_title")}
  description={$t("settings.engines.hardware_desc")}
  bodyLayout="flush"
  data-testid="engine-hardware"
>
  {#snippet icon()}<MonitorCog size={15} strokeWidth={1.75} />{/snippet}

  {#if view.kind === "unknown"}
    <p class="py-3 text-body-sm text-muted-foreground" data-testid="engine-hardware-unknown">
      {$t("settings.engines.hardware_unknown")}
    </p>
  {:else if view.kind === "no_card"}
    <p class="py-3 text-body-sm text-foreground" data-testid="engine-hardware-no-card">
      {view.apple ? $t("settings.engines.hardware_apple") : $t("settings.engines.hardware_none")}
    </p>
  {:else}
    <ul class="divide-y divide-border/50">
      {#each view.hardware.cards as card, i (i)}
        <li class="flex flex-col gap-1 py-3" data-testid="engine-hardware-card-{i}">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-body-sm font-medium text-foreground">{card.name}</span>
            <Badge variant="neutral" outline size="sm">
              {VENDOR_NAMES[card.vendor] ?? $t("settings.engines.hardware_vendor_other")}
            </Badge>
            {#if card.vram_gb !== null && card.vram_gb > 0}
              <Badge variant="neutral" size="sm">{gb(card.vram_gb)}</Badge>
            {/if}
          </div>
          <p class="text-caption text-muted-foreground">
            {#if card.timed_out}
              {$t("settings.engines.hardware_timed_out")}
            {:else if card.driver_version}
              {$t("settings.engines.hardware_driver", { values: { version: card.driver_version } })}
            {:else}
              {$t("settings.engines.hardware_driver_unknown")}
            {/if}
          </p>
        </li>
      {/each}
    </ul>
    <p class="border-t border-border/50 py-3 text-caption text-muted-foreground" data-testid="engine-hardware-vulkan">
      {view.hardware.vulkan_loader
        ? $t("settings.engines.hardware_vulkan_yes")
        : $t("settings.engines.hardware_vulkan_no")}
    </p>
  {/if}
</SettingsSection>
