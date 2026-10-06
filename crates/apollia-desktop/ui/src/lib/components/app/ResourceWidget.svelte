<script lang="ts" module>
  import type { ResourceUsage } from "$lib/ipc/system";

  /** One gauge of the widget. */
  export interface WidgetGauge {
    id: "cpu" | "ram" | "vram";
    /** The i18n key of its name. */
    labelKey: string;
    /** Its share, from 0 to 100; `null` when the figure is not a share. */
    percent: number | null;
  }

  function share(used: number, total: number): number {
    if (!(total > 0)) return 0;
    return Math.max(0, Math.min(100, (used / total) * 100));
  }

  /**
   * The gauges of a measurement, in reading order: the processor, the
   * memory, the GPU memory. The GPU memory has a share only where the
   * machine has dedicated video memory.
   */
  export function widgetGauges(usage: ResourceUsage): WidgetGauge[] {
    return [
      {
        id: "cpu",
        labelKey: "nav.resource_widget.cpu",
        percent: Math.max(0, Math.min(100, usage.cpu_percent)),
      },
      {
        id: "ram",
        labelKey: "nav.resource_widget.ram",
        percent: share(usage.ram_used_gb, usage.ram_total_gb),
      },
      {
        id: "vram",
        labelKey: "nav.resource_widget.vram",
        percent:
          usage.vram.kind === "dedicated"
            ? share(usage.vram.used_gb, usage.vram.total_gb)
            : null,
      },
    ];
  }

  /** The tone of a gauge: calm, then a warning, then full. */
  export function widgetTone(percent: number): "primary" | "warning" | "destructive" {
    if (percent >= 90) return "destructive";
    if (percent >= 75) return "warning";
    return "primary";
  }
</script>

<script lang="ts">
  /**
   * ResourceWidget - the consumption widget at the foot of the sidebar: the
   * processor, the memory and the GPU memory the machine is using. A click
   * opens the resource monitor of System > Hardware.
   *
   * It shows the measurement it is given and reads nothing itself: the
   * sidebar holds the subscription, and only while the widget is shown.
   * `compact` is the 64px rail, where each gauge is a bar named by the
   * tooltip.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight } from "lucide-svelte";
  import { ProgressBar } from "$lib/components/ui/progress";

  interface Props {
    /** The latest measurement, `null` until the first one answers. */
    usage: ResourceUsage | null;
    /** The rail: bars only. */
    compact?: boolean;
    /** Opens the resource monitor. */
    onopen: () => void;
  }

  let { usage, compact = false, onopen }: Props = $props();

  const gauges = $derived(usage ? widgetGauges(usage) : []);

  const gb = (value: number): string =>
    $t("settings.hardware.gb", { values: { n: value.toFixed(value >= 100 ? 0 : 1) } });
  const of = (used: number, total: number): string =>
    $t("settings.resource_monitor.used_of", { values: { used: gb(used), total: gb(total) } });

  /** What a gauge reads, in words. */
  function figure(gauge: WidgetGauge, measured: ResourceUsage): string {
    if (gauge.id === "cpu") {
      return $t("settings.resource_monitor.percent", {
        values: { n: Math.round(measured.cpu_percent) },
      });
    }
    if (gauge.id === "ram") return of(measured.ram_used_gb, measured.ram_total_gb);
    if (measured.vram.kind === "dedicated") {
      return of(measured.vram.used_gb, measured.vram.total_gb);
    }
    return $t(
      measured.vram.kind === "unified"
        ? "nav.resource_widget.vram_unified"
        : "nav.resource_widget.vram_unmeasured",
    );
  }

  const summary = $derived(
    usage
      ? gauges.map((gauge) => `${$t(gauge.labelKey)} ${figure(gauge, usage)}`).join(" · ")
      : $t("nav.resource_widget.measuring"),
  );
</script>

{#if compact}
  <div class="group relative mb-1">
    <button
      type="button"
      class="flex h-10 w-10 flex-col items-stretch justify-center gap-1 rounded-lg px-2 text-muted-foreground transition-all duration-fast hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={onopen}
      aria-label={`${$t("nav.resource_widget.aria")} (${summary})`}
      data-testid="sidebar-resource-widget"
      data-state="compact"
    >
      {#each gauges as gauge (gauge.id)}
        {#if gauge.percent !== null}
          <ProgressBar
            value={gauge.percent}
            size="sm"
            variant={widgetTone(gauge.percent)}
          />
        {/if}
      {/each}
    </button>
    <span
      class="pointer-events-none absolute left-full top-1/2 z-overlay ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-caption font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-fast group-hover:opacity-100"
    >
      {summary}
    </span>
  </div>
{:else}
  <button
    type="button"
    class="flex w-full flex-col gap-2 rounded-lg border border-border bg-card px-3 py-2.5 text-left transition-colors duration-fast hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    onclick={onopen}
    aria-label={$t("nav.resource_widget.aria")}
    data-testid="sidebar-resource-widget"
    data-state="full"
  >
    <span class="flex items-center justify-between text-caption text-muted-foreground">
      {$t("nav.resource_widget.open")}
      <ChevronRight size={14} strokeWidth={1.75} aria-hidden="true" />
    </span>
    {#if !usage}
      <span class="text-caption text-muted-foreground" data-testid="sidebar-resource-widget-measuring">
        {$t("nav.resource_widget.measuring")}
      </span>
    {:else}
      {#each gauges as gauge (gauge.id)}
        <span class="flex flex-col gap-1" data-testid={`sidebar-resource-widget-${gauge.id}`}>
          <span class="flex items-center justify-between gap-2 text-caption">
            <span class="text-muted-foreground">{$t(gauge.labelKey)}</span>
            <span class="tabular-nums text-foreground">{figure(gauge, usage)}</span>
          </span>
          {#if gauge.percent !== null}
            <ProgressBar
              value={gauge.percent}
              size="sm"
              variant={widgetTone(gauge.percent)}
            />
          {/if}
        </span>
      {/each}
    {/if}
  </button>
{/if}
