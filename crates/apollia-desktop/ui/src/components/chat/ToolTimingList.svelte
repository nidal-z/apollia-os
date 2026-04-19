<script lang="ts">
  import type { ToolTiming } from "$lib/types";

  interface Props {
    timings: ToolTiming[];
    limit?: number;
  }

  let { timings, limit = 10 }: Props = $props();

  const rows = $derived(timings.slice(-limit).reverse());

  function deltaBadge(delta: number | null): { label: string; tone: string } {
    if (delta === null) return { label: "—", tone: "muted" };
    const sign = delta >= 0 ? "+" : "";
    const tone = delta >= 50 ? "danger" : delta >= 20 ? "warn" : "ok";
    return { label: `${sign}${delta.toFixed(0)}%`, tone };
  }
</script>

<div class="flex flex-col gap-1.5" data-testid="tool-timing-list">
  {#if rows.length === 0}
    <p class="text-[11px] italic text-muted-foreground/60">
      Aucun appel d'outil mesuré.
    </p>
  {:else}
    {#each rows as timing (`${timing.tool_name}-${timing.actual_ms}`)}
      {@const badge = deltaBadge(timing.delta_pct)}
      <div
        class="flex items-center justify-between rounded-md bg-muted/30 px-2 py-1 text-[11px]"
        data-testid="tool-timing-row"
      >
        <span class="truncate font-mono text-[10px]">{timing.tool_name}</span>
        <div class="flex items-center gap-2 tabular-nums">
          <span class="text-muted-foreground">{timing.actual_ms}ms</span>
          {#if timing.expected_ms !== null}
            <span
              class="rounded px-1.5 py-0.5 text-[10px] font-medium"
              class:bg-success-muted={badge.tone === "ok"}
              class:bg-warning-muted={badge.tone === "warn"}
              class:bg-destructive-muted={badge.tone === "danger"}
              class:text-muted-foreground={badge.tone === "muted"}
              title={`Attendu: ${timing.expected_ms}ms`}
              data-tone={badge.tone}
            >
              {badge.label}
            </span>
          {/if}
        </div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .bg-success-muted {
    background-color: rgb(34 197 94 / 0.2);
    color: rgb(34 197 94);
  }
  .bg-warning-muted {
    background-color: rgb(234 179 8 / 0.2);
    color: rgb(202 138 4);
  }
  .bg-destructive-muted {
    background-color: rgb(239 68 68 / 0.2);
    color: rgb(239 68 68);
  }
</style>
