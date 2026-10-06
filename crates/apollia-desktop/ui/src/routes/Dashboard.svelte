<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";
  import { pendingApprovals, pendingCount } from "$lib/stores/hitl";
  import { pendingChatApprovals, pendingChatApprovalCount } from "$lib/stores/chat";
  import { developerMode } from "$lib/stores/developerMode";
  import { navigateTo } from "$lib/stores/navigation";
  import { LayoutDashboard, MessageSquarePlus } from "lucide-svelte";

  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import { EmptyState } from "$lib/components/layout";

  import { PageHeader } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";

  import PendingDecisionsCard from "../components/dashboard/PendingDecisionsCard.svelte";
  import DeliverablesCard from "../components/dashboard/DeliverablesCard.svelte";
  import AgentsAtWorkCard from "../components/dashboard/AgentsAtWorkCard.svelte";
  import RecentActivityStrip from "../components/dashboard/RecentActivityStrip.svelte";
  import {
    buildInboxItems,
    toPendingRows,
    toDeliverableRows,
    toActivityRows,
  } from "../components/dashboard/dashboardData";
  import { formatDayHeadline } from "$lib/format";

  const PENDING_BLOCK_LIMIT = 5;

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

  // ── View rows (shaping delegated to dashboardData) ────────────────────────
  const totalPending = $derived($pendingCount + $pendingChatApprovalCount);
  const inboxItems = $derived(buildInboxItems($pendingApprovals, $pendingChatApprovals));
  const pendingRows = $derived(
    toPendingRows(inboxItems, $t("dashboard.pending_untitled"), $locale ?? "en"),
  );

  const todayStartIso = $derived.by(() => {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d.toISOString();
  });

  const deliverableRows = $derived(toDeliverableRows($tasks, todayStartIso, $locale ?? "en"));
  const activityRows = $derived(toActivityRows($tasks, $locale ?? "en"));

  const agentsAtWork = $derived(
    $agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"),
  );

  // ── Greeting + headline ───────────────────────────────────────────────────
  const greeting = $derived.by(() => {
    const h = new Date().getHours();
    if (h < 12) return $t("dashboard.greeting_morning");
    if (h < 18) return $t("dashboard.greeting_afternoon");
    return $t("dashboard.greeting_evening");
  });

  const todayLabel = $derived(formatDayHeadline(new Date(), $locale ?? "en"));

  const headlineTitle = $derived.by(() => {
    if (totalPending > 0) {
      const key =
        totalPending > 1 ? "dashboard.headline_pending_other" : "dashboard.headline_pending_one";
      return $t(key, { values: { count: totalPending } });
    }
    if (agentsAtWork.length > 0) {
      const key =
        agentsAtWork.length > 1 ? "dashboard.headline_agents_other" : "dashboard.headline_agents_one";
      return $t(key, { values: { count: agentsAtWork.length } });
    }
    return $t("dashboard.headline_greeting", { values: { greeting } });
  });

  const headlineSubtitle = $derived.by(() => {
    const parts: string[] = [];
    if (totalPending > 0) {
      const key =
        totalPending > 1 ? "dashboard.subtitle_pending_other" : "dashboard.subtitle_pending_one";
      parts.push($t(key, { values: { count: totalPending } }));
    }
    if (agentsAtWork.length > 0) {
      const key =
        agentsAtWork.length > 1 ? "dashboard.subtitle_agents_other" : "dashboard.subtitle_agents_one";
      parts.push($t(key, { values: { count: agentsAtWork.length } }));
    }
    if (deliverableRows.length > 0) {
      const key =
        deliverableRows.length > 1
          ? "dashboard.subtitle_deliverables_other"
          : "dashboard.subtitle_deliverables_one";
      parts.push($t(key, { values: { count: deliverableRows.length } }));
    }
    if (parts.length === 0) return $t("dashboard.subtitle_calm");
    return parts.join(" · ") + ".";
  });

  function navigateToAgents() {
    navigateTo("agents");
  }
  function navigateToInbox() {
    navigateTo("inbox");
  }
  function navigateToJournal() {
    navigateTo("journal");
  }
  function navigateToChat() {
    navigateTo("chat");
  }

  const isColdStart = $derived(
    $tasks.length === 0 && $agents.length === 0,
  );
</script>

<!-- Unified dashboard - Operator layout shared by both modes; Builder mode
     receives its technical overlay via DeveloperOnly inside AgentsAtWorkCard. -->
<div class="mx-auto w-full max-w-6xl" data-testid="dashboard-page" data-mode={$developerMode ? "builder" : "operator"}>
  <PageHeader
    kicker={todayLabel || undefined}
    title={headlineTitle}
    subtitle={headlineSubtitle}
  >
    {#snippet actions()}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={navigateToChat}
        data-testid="dashboard-cta-new-chat"
      >
        {#snippet icon()}<MessageSquarePlus size={13} />{/snippet}
        {$t("dashboard.header_action_new_chat")}
      </Button>
    {/snippet}
  </PageHeader>

  {#if isColdStart}
    <!-- Cold start - welcome the operator. -->
    <div class="px-8 mt-6" data-testid="dashboard-cold-start">
      <EmptyState
        icon={LayoutDashboard}
        title={$t("dashboard.cold_start_title")}
        description={$t("dashboard.cold_start_subtitle")}
        primaryLabel={$t("dashboard.browse_agents")}
        primaryAction={navigateToAgents}
        page="dashboard"
      />
    </div>
  {:else}
    <!-- Bento attention zone: 3-column responsive grid -->
    <div class="px-8 pt-6 grid gap-4 lg:grid-cols-3" data-testid="dashboard-bento">
      <div class="lg:col-span-2 lg:row-span-2">
        <PendingDecisionsCard
          rows={pendingRows}
          total={totalPending}
          limit={PENDING_BLOCK_LIMIT}
          onSeeAll={navigateToInbox}
          onOpenInbox={navigateToInbox}
          emptyTitle={$t("dashboard.pending_empty_title")}
          emptyDesc={$t("dashboard.pending_empty_desc")}
          moreLabel={$t(
            pendingRows.length - PENDING_BLOCK_LIMIT > 1
              ? "dashboard.inbox_more_other"
              : "dashboard.inbox_more_one",
            { values: { count: pendingRows.length - PENDING_BLOCK_LIMIT } },
          )}
        />
      </div>

      <DeliverablesCard items={deliverableRows} onOpen={navigateToJournal} />

      <AgentsAtWorkCard
        agents={agentsAtWork}
        onDetail={openDetail}
        onLogs={openLogs}
      />
    </div>

    <RecentActivityStrip items={activityRows} onOpen={navigateToJournal} />
  {/if}
</div>

{#if detailAgent}
  <AgentDetail
    agent={detailAgent}
    open={detailOpen}
    onclose={closeDetail}
    onlogs={openLogsFromDetail}
  />
{/if}
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
