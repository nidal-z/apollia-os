<script lang="ts" module>
  import type { ResourceUsage } from "$lib/ipc/system";

  /** The share of `total` that `used` is, from 0 to 100; 0 when nothing is installed. */
  export function sharePercent(used: number, total: number): number {
    if (!(total > 0)) return 0;
    return Math.max(0, Math.min(100, (used / total) * 100));
  }

  /** The tone of a gauge: calm, then a warning, then full. */
  export function gaugeTone(percent: number): "primary" | "warning" | "destructive" {
    if (percent >= 90) return "destructive";
    if (percent >= 75) return "warning";
    return "primary";
  }
</script>

<script lang="ts">
  /**
   * ResourceMonitor - System > Hardware: the memory, the video memory and the
   * processor the machine is using. It shows the measurement it is given and
   * reads nothing itself; the page that mounts it holds the subscription.
   */
  import { t } from "svelte-i18n";
  import { Gauge } from "lucide-svelte";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    /** The latest measurement, `null` until the first one answers. */
    usage: ResourceUsage | null;
  }

  let { usage }: Props = $props();

  const gb = (value: number): string =>
    $t("settings.hardware.gb", { values: { n: value.toFixed(value >= 100 ? 0 : 1) } });
  const of = (used: number, total: number): string =>
    $t("settings.resource_monitor.used_of", { values: { used: gb(used), total: gb(total) } });
</script>

<SettingsSection
  title={$t("settings.resource_monitor.title")}
  description={$t("settings.resource_monitor.desc")}
  data-testid="resource-monitor"
>
  {#snippet icon()}<Gauge size={15} strokeWidth={1.75} />{/snippet}

  {#if !usage}
    <div
      class="flex items-center gap-2 text-body-sm text-muted-foreground"
      data-testid="resource-monitor-measuring"
    >
      <Spinner class="h-4 w-4" />
      {$t("settings.resource_monitor.measuring")}
    </div>
  {:else}
    {@const ram = sharePercent(usage.ram_used_gb, usage.ram_total_gb)}
    <div class="grid gap-4 md:grid-cols-3">
      <div class="flex flex-col gap-1.5" data-testid="resource-monitor-ram">
        <span class="text-caption text-muted-foreground">{$t("settings.hardware.ram")}</span>
        <span class="text-body-sm tabular-nums text-foreground" data-testid="resource-monitor-ram-value">
          {of(usage.ram_used_gb, usage.ram_total_gb)}
        </span>
        <ProgressBar value={ram} size="sm" variant={gaugeTone(ram)} />
      </div>

      <div class="flex flex-col gap-1.5" data-testid="resource-monitor-vram">
        <span class="text-caption text-muted-foreground">{$t("settings.hardware.vram")}</span>
        {#if usage.vram.kind === "dedicated"}
          {@const vram = sharePercent(usage.vram.used_gb, usage.vram.total_gb)}
          <span class="text-body-sm tabular-nums text-foreground" data-testid="resource-monitor-vram-value">
            {of(usage.vram.used_gb, usage.vram.total_gb)}
          </span>
          <ProgressBar value={vram} size="sm" variant={gaugeTone(vram)} />
        {:else if usage.vram.kind === "unified"}
          <span class="text-body-sm text-foreground" data-testid="resource-monitor-vram-unified">
            {$t("settings.resource_monitor.vram_unified")}
          </span>
        {:else}
          <span class="text-body-sm text-muted-foreground" data-testid="resource-monitor-vram-unmeasured">
            {$t("settings.resource_monitor.vram_unmeasured")}
          </span>
        {/if}
      </div>

      <div class="flex flex-col gap-1.5" data-testid="resource-monitor-cpu">
        <span class="text-caption text-muted-foreground">{$t("settings.hardware.processor")}</span>
        <span class="text-body-sm tabular-nums text-foreground" data-testid="resource-monitor-cpu-value">
          {$t("settings.resource_monitor.percent", {
            values: { n: Math.round(usage.cpu_percent) },
          })}
        </span>
        <ProgressBar value={usage.cpu_percent} size="sm" variant={gaugeTone(usage.cpu_percent)} />
      </div>
    </div>
  {/if}
</SettingsSection>
