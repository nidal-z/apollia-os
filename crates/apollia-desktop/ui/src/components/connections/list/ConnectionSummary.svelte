<script lang="ts">
  /**
   * ConnectionSummary - the line over the grid of the Connections page: how
   * many connections there are, how many work, how many ask for an action,
   * and how many agents use them. Each of the first three is a filter of the
   * grid on its state; choosing the one already chosen gives every state back.
   */
  import { t } from "svelte-i18n";
  import { cn } from "$lib/utils";
  import type { StateFilter } from "$lib/connections/connectionRows";
  import type { DashboardSummary } from "$lib/connections/connectionDashboard";

  interface Props {
    summary: DashboardSummary;
    state: StateFilter;
    onstate: (state: StateFilter) => void;
  }

  let { summary, state, onstate }: Props = $props();

  const counts = $derived([
    { key: "all" as const, labelKey: "connections.summary.total", count: summary.total, tone: "text-foreground" },
    { key: "working" as const, labelKey: "connections.summary.working", count: summary.working, tone: "text-success-a11y" },
    { key: "action" as const, labelKey: "connections.summary.action", count: summary.action, tone: "text-warning-a11y" },
  ]);

  function choose(key: StateFilter): void {
    onstate(key === state && key !== "all" ? "all" : key);
  }
</script>

<div
  class="flex flex-wrap items-center gap-x-1 gap-y-1 text-body-sm"
  role="group"
  aria-label={$t("connections.summary.label")}
  data-testid="connections-summary"
>
  {#each counts as item, i (item.key)}
    {#if i > 0}<span class="text-muted-foreground" aria-hidden="true">·</span>{/if}
    <button
      type="button"
      class={cn(
        "rounded-md px-1.5 py-0.5 font-medium transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        item.key === "all" ? "text-foreground" : item.count > 0 ? item.tone : "text-muted-foreground",
        state === item.key && "bg-muted",
      )}
      aria-pressed={state === item.key}
      onclick={() => choose(item.key)}
      data-testid="connections-summary-{item.key}"
      data-count={item.count}
    >
      {$t(item.labelKey, { values: { count: item.count } })}
    </button>
  {/each}
  <span class="text-muted-foreground" aria-hidden="true">·</span>
  <span class="px-1.5 text-muted-foreground" data-testid="connections-summary-agents" data-count={summary.agents}>
    {$t("connections.summary.agents", { values: { count: summary.agents } })}
  </span>
</div>
