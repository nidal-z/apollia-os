<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";
  import { pendingCount } from "$lib/stores/hitl";
  import { currentRoute } from "$lib/stores/navigation";
  import { Button } from "$lib/components/ui/button";
  import { Bot } from "lucide-svelte";
  import DashboardHeader from "../components/dashboard/DashboardHeader.svelte";
  import ActiveAgentCard from "../components/dashboard/ActiveAgentCard.svelte";
  import RecentActivity from "../components/dashboard/RecentActivity.svelte";
  import PendingActions from "../components/dashboard/PendingActions.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";

  const RECENT_TASK_LIMIT = 5;

  let detailAgent = $state<AgentStatus | null>(null);
  let detailOpen = $state(false);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  function openDetail(agent: AgentStatus) {
    detailAgent = agent;
    detailOpen = true;
  }

  function closeDetail() {
    detailOpen = false;
  }

  function openLogsFromDetail(agentId: string) {
    closeDetail();
    logsAgentId = agentId;
    logsOpen = true;
  }

  function closeLogs() {
    logsOpen = false;
  }

  let activeAgents = $derived(
    $agents.filter((a) => a.state === "active" || a.state === "degraded"),
  );

  let recentTasks = $derived(
    [...$tasks]
      .filter((t) => t.status === "completed" || t.status === "failed")
      .sort((a, b) => b.created_at.localeCompare(a.created_at))
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
      <!-- AC-5: Empty state -->
      <div class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-12" data-testid="dashboard-empty-state">
        <Bot size={32} class="text-muted-foreground/50" />
        <p class="text-sm text-muted-foreground">{$t('dashboard.no_assistants')}</p>
        <Button variant="outline" onclick={navigateToAgents} data-testid="dashboard-discover-btn">
          {$t('dashboard.discover_assistants')}
        </Button>
      </div>
    {:else}
      <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="dashboard-agents-grid">
        {#each activeAgents as agent (agent.id)}
          <ActiveAgentCard {agent} ondetail={openDetail} />
        {/each}
      </div>
    {/if}
  </section>

  <!-- AC-3: Recent activity -->
  <section data-testid="dashboard-activity-section">
    <h2 class="mb-3 text-lg font-semibold">{$t('dashboard.recent_activity')}</h2>
    <RecentActivity tasks={recentTasks} />
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
