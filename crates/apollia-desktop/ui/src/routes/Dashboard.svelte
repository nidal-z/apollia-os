<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";
  import { pendingApprovals, pendingCount } from "$lib/stores/hitl";
  import { pendingChatApprovals, pendingChatApprovalCount } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { navigateTo } from "$lib/stores/navigation";
  import { dailyDigest, type DigestFacts } from "$lib/stores/dailyDigest";
  import { LayoutDashboard, Zap } from "lucide-svelte";

  // Operator-mode blocks.
  import DigestHero from "../components/dashboard/DigestHero.svelte";
  import PendingActionsBlock from "../components/dashboard/PendingActionsBlock.svelte";
  import CompletedTodayBlock from "../components/dashboard/CompletedTodayBlock.svelte";
  import MyAssistantsBlock from "../components/dashboard/MyAssistantsBlock.svelte";

  // Builder-mode (legacy layout preserved).
  import DashboardHeader from "../components/dashboard/DashboardHeader.svelte";
  import ActiveAgentCard from "../components/dashboard/ActiveAgentCard.svelte";
  import RecentActivity from "../components/dashboard/RecentActivity.svelte";
  import PendingActions from "../components/dashboard/PendingActions.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  // Adapters for the unified inbox preview feed (reuses types).
  import type { InboxItem, InboxRisk } from "../components/inbox/types";
  import type { PendingApproval, PendingChatApproval } from "$lib/types";

  const RECENT_TASK_LIMIT = 8;
  const COMPLETED_TODAY_LIMIT = 10;
  const PENDING_BLOCK_LIMIT = 5;

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

  // ── Derived agent slices (shared across both modes) ────────────────────────

  const activeAssistants = $derived(
    $agents.filter((a) => a.agent_type !== "worker" && (a.runtime_status === "active" || a.runtime_status === "degraded")),
  );
  const installedAssistants = $derived(
    $agents.slice(0, 12),
  );
  const allWorkers = $derived($agents.filter((a) => a.agent_type === "worker"));

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

  // ── Facts (A.8.2 — header shift from "today" to rolling week) ──────────────

  const last24hIso = $derived.by(() => {
    const d = new Date(Date.now() - 24 * 3600 * 1000);
    return d.toISOString();
  });

  const last7daysIso = $derived.by(() => {
    const d = new Date(Date.now() - 7 * 24 * 3600 * 1000);
    return d.toISOString();
  });

  const tasksCompleted24h = $derived(
    $tasks.filter((x) => x.status === "completed" && x.created_at >= last24hIso).length,
  );
  const tasksFailed24h = $derived(
    $tasks.filter((x) => x.status === "failed" && x.created_at >= last24hIso).length,
  );
  const tasksCompletedWeek = $derived(
    $tasks.filter((x) => x.status === "completed" && x.created_at >= last7daysIso).length,
  );

  const topAssistant = $derived.by<string | null>(() => {
    const counts = new Map<string, number>();
    for (const tk of $tasks) {
      if (tk.status !== "completed" || tk.created_at < last24hIso) continue;
      counts.set(tk.agent_name, (counts.get(tk.agent_name) ?? 0) + 1);
    }
    let top: [string, number] | null = null;
    for (const entry of counts) {
      if (!top || entry[1] > top[1]) top = entry;
    }
    return top ? top[0] : null;
  });

  const facts = $derived<DigestFacts>({
    tasksCompleted: tasksCompleted24h,
    tasksFailed: tasksFailed24h,
    approvalsPending: $pendingCount + $pendingChatApprovalCount,
    insightsCreated: 0, // will plumb through the real count.
    automationsFailed: 0,
    firstFailureLabel: null,
    topAssistant,
  });

  // ── Inbox items adapter (light — just enough for the compact preview) ──────

  function extractRisk(ctx: Record<string, unknown> | undefined): InboxRisk | undefined {
    if (!ctx || typeof ctx !== "object") return undefined;
    const r = (ctx as { risk?: unknown }).risk;
    if (!r || typeof r !== "object") return undefined;
    const rec = r as Record<string, unknown>;
    const level = rec.level;
    if (level !== "low" && level !== "medium" && level !== "high") return undefined;
    return {
      level,
      summary: typeof rec.summary === "string" ? rec.summary : "",
    };
  }

  function taskToInbox(p: PendingApproval): InboxItem {
    const risk = extractRisk(p.context);
    return {
      id: `task:${p.task_id}`,
      kind: "task",
      agentName: p.agent_name || "-",
      summary: risk?.summary || p.prompt || "-",
      suspendedAt: p.suspended_at,
      risk,
      source: p,
    };
  }

  function chatToInbox(c: PendingChatApproval): InboxItem {
    return {
      id: `chat:${c.sessionId}:${c.messageId}:${c.toolName}`,
      kind: "tool",
      agentName: c.sessionId.slice(0, 8),
      sessionId: c.sessionId,
      toolName: c.toolName,
      summary: c.inputPreview.slice(0, 140),
      suspendedAt: c.receivedAt,
      source: c,
    };
  }

  const inboxItems = $derived.by<InboxItem[]>(() => {
    const task = $pendingApprovals.map(taskToInbox);
    const chat = $pendingChatApprovals.map(chatToInbox);
    return [...task, ...chat].sort(
      (a, b) => new Date(a.suspendedAt).getTime() - new Date(b.suspendedAt).getTime(),
    );
  });

  const totalPending = $derived($pendingCount + $pendingChatApprovalCount);

  // ── Digest lifecycle ───────────────────────────────────────────────────────
  // Load on mount + reload when facts actually change (cache-honored).

  onMount(() => {
    dailyDigest.load(untrack(() => facts));
  });

  let lastFactsKey = "";
  $effect(() => {
    const key = JSON.stringify(facts);
    if (key !== lastFactsKey) {
      lastFactsKey = key;
      dailyDigest.load(facts);
    }
  });

  function refreshDigest() {
    dailyDigest.refresh(facts);
  }

  function navigateToAgents() { navigateTo("agents"); }
</script>

{#if $uiMode === "operator"}
  <!-- Operator mode — Daily Digest Dashboard ("Aujourd'hui chez Apollia") -->
  <div class="mx-auto w-full max-w-6xl" data-testid="dashboard-page" data-mode="operator">
    <DigestHero
      digest={$dailyDigest.digest}
      weekTasksCount={tasksCompletedWeek}
      weekPendingCount={totalPending}
      loading={$dailyDigest.loading}
      onrefresh={refreshDigest}
    />

    {#if $tasks.length === 0 && $agents.length === 0}
      <!-- Cold start — welcome the operator. -->
      <div class="mt-6" data-testid="dashboard-cold-start">
        <EmptyState
          icon={LayoutDashboard}
          title={$t('dashboard.cold_start_title')}
          subtitle={$t('dashboard.cold_start_subtitle')}
          ctaLabel={$t('dashboard.browse_templates')}
          ctaAction={navigateToAgents}
          page="dashboard"
        />
      </div>
    {:else}
      <!-- 3-col grid on lg, 1-col stack on smaller viewports. -->
      <div class="mt-6 grid gap-4 lg:grid-cols-3" data-testid="dashboard-grid">
        <PendingActionsBlock items={inboxItems} {totalPending} limit={PENDING_BLOCK_LIMIT} />
        <CompletedTodayBlock tasks={$tasks} limit={COMPLETED_TODAY_LIMIT} />
        <MyAssistantsBlock agents={installedAssistants} />
      </div>
    {/if}
  </div>
{:else}
  <!-- Builder mode — retains the previous layout with full observability. -->
  <div class="mx-auto w-full max-w-6xl" data-testid="dashboard-page" data-mode="builder">
    <DashboardHeader />

    {#if $pendingCount > 0}
      <div class="mt-5" in:fly={{ y: 6, duration: 200 }}>
        <PendingActions count={$pendingCount} />
      </div>
    {/if}

    <div class="mt-6 grid gap-6 lg:grid-cols-5">
      <section class="lg:col-span-3" data-testid="dashboard-agents-section">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('dashboard.active_assistants')}</h2>
          <span class="text-xs text-muted-foreground/60">{activeAssistants.length} {$t('dashboard.active_count_suffix')}</span>
        </div>
        {#if activeAssistants.length === 0}
          <EmptyState
            icon={LayoutDashboard}
            title={$t('dashboard.no_assistants')}
            ctaLabel={$t('dashboard.discover_assistants')}
            ctaAction={navigateToAgents}
            page="dashboard"
          />
        {:else}
          <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2" data-testid="dashboard-agents-grid">
            {#each activeAssistants as agent, i (agent.name)}
              <div animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 200, delay: i * 50 }}>
                <ActiveAgentCard {agent} ondetail={openDetail} />
              </div>
            {/each}
          </div>
        {/if}

        {#if allWorkers.length > 0}
          <div class="mt-5 pt-4 border-t border-border/20">
            <div class="flex items-center justify-between mb-2">
              <h3 class="flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground/60">
                <Zap size={11} class="text-secondary/60" />{$t('dashboard.workers_section')}
              </h3>
              <button class="text-[10px] text-primary/60 hover:text-primary transition-colors" onclick={navigateToAgents}>
                {$t('dashboard.manage')}
              </button>
            </div>
            <div class="flex flex-wrap gap-1.5">
              {#each allWorkers as worker (worker.name)}
                {@const isActive = worker.runtime_status === "active" || worker.runtime_status === "degraded"}
                <div
                  class="flex items-center gap-1.5 rounded-md px-2 py-1 text-[10px] border
                    {isActive
                      ? 'bg-secondary/10 border-secondary/20 text-secondary/80'
                      : 'bg-muted/30 border-border/30 text-muted-foreground/50'}"
                  title={worker.skills.map(s => s.name).join(', ')}
                  data-testid="dashboard-worker-{worker.name}"
                >
                  <Zap size={8} />
                  <span class="font-medium">{worker.name}</span>
                  {#if isActive}
                    <span class="h-1.5 w-1.5 rounded-full bg-secondary/70"></span>
                  {:else}
                    <span class="text-[9px] opacity-50">{$t('agents.stopped')}</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </section>

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
{/if}

{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
