<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";
  import { pendingCount } from "$lib/stores/hitl";
  import { currentRoute } from "$lib/stores/navigation";
  import { LayoutDashboard } from "lucide-svelte";
  import DashboardHeader from "../components/dashboard/DashboardHeader.svelte";
  import ActiveAgentCard from "../components/dashboard/ActiveAgentCard.svelte";
  import RecentActivity from "../components/dashboard/RecentActivity.svelte";
  import PendingActions from "../components/dashboard/PendingActions.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  const RECENT_TASK_LIMIT = 5;

  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  function openDetail(agent: AgentListItem) {
    detailAgent = agent;
    detailOpen = true;
  }

  function closeDetail() {
    detailOpen = false;
  }

  function openLogs(agentId: string) {
    logsAgentId = agentId;
    logsOpen = true;
  }

  function openLogsFromDetail(agentId: string) {
    closeDetail();
    openLogs(agentId);
  }

  function closeLogs() {
    logsOpen = false;
  }

  let activeAgents = $derived(
    $agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"),
  );

  let recentTasks = $derived(
    [...$tasks]
      .sort((a, b) => {
        // Working tasks first, then by date
        const aWorking = a.status === "working" || a.status === "submitted" ? 0 : 1;
        const bWorking = b.status === "working" || b.status === "submitted" ? 0 : 1;
        if (aWorking !== bWorking) return aWorking - bWorking;
        return b.created_at.localeCompare(a.created_at);
      })
      .slice(0, RECENT_TASK_LIMIT),
  );

  function navigateToAgents() {
    currentRoute.set("agents");
  }
</script>

<div class="space-y-6" data-testid="dashboard-page">
  <!-- AC-1: Header with greeting and summary -->
  <DashboardHeader />

  <!-- AC-4: Pending actions (approvals) -->
  {#if $pendingCount > 0}
    <PendingActions count={$pendingCount} />
  {/if}

  <!-- AC-2: Active assistants -->
  <section data-testid="dashboard-agents-section">
    <h2 class="mb-3 text-lg font-semibold">{$t('dashboard.active_assistants')}</h2>
    {#if activeAgents.length === 0}
      <EmptyState
        icon={LayoutDashboard}
        title={$t('dashboard.no_assistants')}
        ctaLabel={$t('dashboard.discover_assistants')}
        ctaAction={navigateToAgents}
        page="dashboard"
      />
    {:else}
      <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="dashboard-agents-grid">
        {#each activeAgents as agent (agent.name)}
          <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
            <ActiveAgentCard {agent} ondetail={openDetail} />
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <!-- AC-3: Recent activity -->
  <section data-testid="dashboard-activity-section">
    <h2 class="mb-3 text-lg font-semibold">{$t('dashboard.recent_activity')}</h2>
    <RecentActivity tasks={recentTasks} onagentclick={openLogs} />
  </section>
</div>

<!-- Agent detail sheet -->
{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}

<!-- Logs drawer -->
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
