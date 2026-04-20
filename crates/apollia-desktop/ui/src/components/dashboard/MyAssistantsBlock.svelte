<script lang="ts">
  import { t } from "svelte-i18n";
  import { Zap, Info } from "lucide-svelte";
  import type { AgentListItem } from "$lib/types";
  import { navigateTo } from "$lib/stores/navigation";
  import WorkerBadgeSheet from "./WorkerBadgeSheet.svelte";

  interface Props {
    agents: AgentListItem[];
    /** Count of A2A calls done today, per agent name. */
    a2aUsageToday?: Record<string, number>;
  }

  let { agents, a2aUsageToday = {} }: Props = $props();

  let sheetOpen = $state(false);

  function openWorkerSheet(e: MouseEvent) {
    e.stopPropagation();
    sheetOpen = true;
  }

  function statusTone(a: AgentListItem): string {
    if (a.runtime_status === "active") return "bg-success/70";
    if (a.runtime_status === "degraded") return "bg-warning/70";
    return "bg-muted-foreground/40";
  }
</script>

<section
  class="glass-card glass-border rounded-lg p-4"
  data-testid="my-assistants-block"
>
  <header class="mb-3 flex items-center justify-between">
    <h2 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
      {$t("dashboard.my_assistants_title")}
      <button
        type="button"
        class="rounded p-0.5 text-muted-foreground/60 hover:text-foreground"
        onclick={openWorkerSheet}
        aria-label={$t("dashboard.worker_sheet_title")}
        data-testid="worker-info-button"
      >
        <Info size={12} />
      </button>
    </h2>
    <button
      class="text-[11px] text-primary hover:text-primary/80"
      onclick={() => navigateTo("agents")}
    >
      {$t("dashboard.manage")}
    </button>
  </header>

  {#if agents.length === 0}
    <p class="py-6 text-center text-xs text-muted-foreground">
      {$t("dashboard.my_assistants_empty")}
    </p>
  {:else}
    <ul class="flex flex-col gap-2" data-testid="my-assistants-list">
      {#each agents as agent (agent.name)}
        {@const isWorker = agent.agent_type === "worker"}
        {@const a2aCount = a2aUsageToday[agent.name] ?? 0}
        {@const skillsLabel = agent.skills.map((s) => s.name).join(", ")}
        <li class="flex items-center gap-2 rounded-md border border-border/40 bg-card/60 px-3 py-2">
          <span class="h-2 w-2 rounded-full {statusTone(agent)}" aria-hidden="true"></span>
          <span class="flex-1 truncate text-[13px] font-medium">{agent.name}</span>
          {#if isWorker}
            <button
              type="button"
              class="inline-flex items-center gap-1 rounded-md bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary ring-1 ring-primary/20 hover:bg-primary/15"
              title={a2aCount > 0
                ? $t("dashboard.worker_badge_tooltip", { values: { skills: agent.skills.length, calls: a2aCount } })
                : $t("dashboard.worker_badge_tooltip_idle", { values: { skills: agent.skills.length } })}
              onclick={openWorkerSheet}
              data-testid="worker-badge-{agent.name}"
              aria-label={skillsLabel}
            >
              <Zap size={10} />
              <span>A2A</span>
              {#if a2aCount > 0}
                <span class="font-semibold">·{a2aCount}</span>
              {/if}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<WorkerBadgeSheet open={sheetOpen} onclose={() => (sheetOpen = false)} />
