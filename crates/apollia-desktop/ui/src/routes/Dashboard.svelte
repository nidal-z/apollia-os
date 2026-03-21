<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";
  import { pendingCount } from "$lib/stores/hitl";
  import { navigateTo } from "$lib/stores/navigation";
  import { LayoutDashboard } from "lucide-svelte";
  import DashboardHeader from "../components/dashboard/DashboardHeader.svelte";
  import ActiveAgentCard from "../components/dashboard/ActiveAgentCard.svelte";
  import RecentActivity from "../components/dashboard/RecentActivity.svelte";
  import PendingActions from "../components/dashboard/PendingActions.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  const RECENT_TASK_LIMIT = 8;

  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  function openDetail(agent: AgentListItem) {
    detailAgent = agent;
    detailOpen = true;
  }
  function closeDetail() { detailOpen = false; }
  function openLogs(agentId: string) { logsAgentId = agentId; logsOpen = true; }
  function openLogsFromDetail(agentId: string) { closeDetail(); openLogs(agentId); }
  function closeLogs() { logsOpen = false; }

  let activeAgents = $derived(
    $agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"),
  );

  let recentTasks = $derived(
    [...$tasks]
      .sort((a, b) => {
        const aW = a.status === "working" || a.status === "submitted" ? 0 : 1;
        const bW = b.status === "working" || b.status === "submitted" ? 0 : 1;
        if (aW !== bW) return aW - bW;
        return b.created_at.localeCompare(a.created_at);
      })
      .slice(0, RECENT_TASK_LIMIT),
  );

  function navigateToAgents() { navigateTo("agents"); }
</script>

<div class="max-w-6xl" data-testid="dashboard-page">
  <DashboardHeader />

  {#if $pendingCount > 0}
    <div class="mt-5" in:fly={{ y: 6, duration: 200 }}>
      <PendingActions count={$pendingCount} />
    </div>
  {/if}

  <!-- Two-column layout: agents left, activity right -->
  <div class="mt-6 grid gap-6 lg:grid-cols-5">
    <!-- Left: Agents (3/5) -->
    <section class="lg:col-span-3" data-testid="dashboard-agents-section">
      <div class="flex items-baseline justify-between mb-3">
        <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('dashboard.active_assistants')}</h2>
        <span class="text-xs text-muted-foreground/60">{activeAgents.length} {$t('dashboard.active_count_suffix')}</span>
      </div>
      {#if activeAgents.length === 0}
        <EmptyState
          icon={LayoutDashboard}
          title={$t('dashboard.no_assistants')}
          ctaLabel={$t('dashboard.discover_assistants')}
          ctaAction={navigateToAgents}
          page="dashboard"
        />
      {:else}
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2" data-testid="dashboard-agents-grid">
          {#each activeAgents as agent, i (agent.name)}
            <div animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200, delay: i * 50 }}>
              <ActiveAgentCard {agent} ondetail={openDetail} />
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Right: Activity (2/5) -->
    <section class="lg:col-span-2" data-testid="dashboard-activity-section">
      <div class="flex items-baseline justify-between mb-3">
        <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('dashboard.recent_activity')}</h2>
        <button class="text-xs text-primary hover:text-primary/80 transition-colors" onclick={() => navigateTo("tasks")}>
          {$t('dashboard.see_all')}
        </button>
      </div>
      <RecentActivity tasks={recentTasks} onagentclick={openLogs} />
    </section>
  </div>
</div>

{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
