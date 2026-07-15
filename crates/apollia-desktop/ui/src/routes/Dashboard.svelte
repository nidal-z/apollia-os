<script lang="ts">
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
  import { projects } from "$lib/stores/projects";
  import { formatRelativeTime } from "$lib/utils";
  import {
    LayoutDashboard,
    Sparkles,
    MessageSquarePlus,
    FolderOpen,
    Activity,
    FileCheck,
  } from "lucide-svelte";

  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import LegacyEmptyState from "../components/common/EmptyState.svelte";
  import { BuilderOnly } from "$lib/components/shared";

  // V3 Operator design-system components.
  import {
    PageHeader,
    SectionTitle,
    StatusDot,
    Card,
    EmptyState,
    InboxRow,
    ListRow,
    ProjectCard,
    type InboxType,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  // Adapters for the unified inbox preview feed (reuses types).
  import type { InboxItem, InboxRisk } from "../components/inbox/types";
  import type { PendingApproval, PendingChatApproval } from "$lib/types";

  const PENDING_BLOCK_LIMIT = 5;
  const PINNED_PROJECTS_LIMIT = 6;

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

  // ── Inbox items adapter (light - just enough for the compact preview) ──────

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

  // ── Tasks completed today (recent deliverables) ───────────────────────────
  const todayStartIso = $derived.by(() => {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d.toISOString();
  });
  const last24hIso = $derived.by(() =>
    new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
  );
  const completedToday = $derived(
    $tasks.filter((tk) => tk.status === "completed" && tk.created_at >= todayStartIso),
  );
  const recentDeliverables = $derived(completedToday.slice(0, 4));

  // ── Active agents at work right now ───────────────────────────────────────
  const agentsAtWork = $derived(
    $agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"),
  );

  // ── Recent activity (last few task events) ────────────────────────────────
  const recentActivityFeed = $derived(
    [...$tasks]
      .sort((a, b) => b.created_at.localeCompare(a.created_at))
      .slice(0, 4),
  );

  // ── Pinned projects (most recently updated) ───────────────────────────────
  const pinnedProjects = $derived(
    [...$projects]
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .slice(0, PINNED_PROJECTS_LIMIT),
  );

  // ── Greeting + headline ───────────────────────────────────────────────────
  const greeting = $derived.by(() => {
    const h = new Date().getHours();
    if (h < 12) return $t("dashboard.greeting_morning");
    if (h < 18) return $t("dashboard.greeting_afternoon");
    return $t("dashboard.greeting_evening");
  });

  const todayLabel = $derived.by(() => {
    try {
      return new Date().toLocaleDateString("fr-FR", {
        weekday: "long",
        day: "numeric",
        month: "long",
      }).toUpperCase();
    } catch {
      return "";
    }
  });

  const headlineTitle = $derived.by(() => {
    if (totalPending > 0) {
      const key = totalPending > 1 ? "dashboard.headline_pending_other" : "dashboard.headline_pending_one";
      return $t(key, { values: { count: totalPending } });
    }
    if (agentsAtWork.length > 0) {
      const key = agentsAtWork.length > 1 ? "dashboard.headline_agents_other" : "dashboard.headline_agents_one";
      return $t(key, { values: { count: agentsAtWork.length } });
    }
    return $t("dashboard.headline_greeting", { values: { greeting } });
  });

  const headlineSubtitle = $derived.by(() => {
    const parts: string[] = [];
    if (totalPending > 0) {
      const key = totalPending > 1 ? "dashboard.subtitle_pending_other" : "dashboard.subtitle_pending_one";
      parts.push($t(key, { values: { count: totalPending } }));
    }
    if (agentsAtWork.length > 0) {
      const key = agentsAtWork.length > 1 ? "dashboard.subtitle_agents_other" : "dashboard.subtitle_agents_one";
      parts.push($t(key, { values: { count: agentsAtWork.length } }));
    }
    if (recentDeliverables.length > 0) {
      const key = recentDeliverables.length > 1 ? "dashboard.subtitle_deliverables_other" : "dashboard.subtitle_deliverables_one";
      parts.push($t(key, { values: { count: recentDeliverables.length } }));
    }
    if (parts.length === 0) return $t("dashboard.subtitle_calm");
    return parts.join(" · ") + ".";
  });

  // ── Inbox row adapter for the V3 InboxRow component ───────────────────────
  function inboxKindToType(item: InboxItem): InboxType {
    if (item.kind === "tool") return "approval";
    if (item.kind === "task") return "approval";
    return "approval";
  }

  function navigateToAgents() { navigateTo("agents"); }
  function navigateToInbox() { navigateTo("inbox"); }
  function navigateToTasks() { navigateTo("tasks"); }
  function navigateToProjects() { navigateTo("projects"); }
  function navigateToChat() { navigateTo("chat"); }

  // ── Status mapping for ProjectCard ────────────────────────────────────────
  type ProjectStatus = "active" | "pause" | "blocked" | "done";
  function projectStatus(_p: { updated_at: string }): ProjectStatus {
    // Heuristic: projects updated in last 24h are "active", else "pause".
    return _p.updated_at >= last24hIso ? "active" : "pause";
  }
</script>

<!-- Unified dashboard - Operator layout shared by both modes; the Builder mode
     receives optional technical overlays via `BuilderOnly` blocks below. -->
<div
  class="mx-auto w-full max-w-6xl"
  data-testid="dashboard-page"
  data-mode={$uiMode}
>
    <PageHeader
      kicker={todayLabel || undefined}
      title={headlineTitle}
      subtitle={headlineSubtitle}
    >
      {#snippet actions()}
        <Button variant="outline" size="sm" onclick={navigateToProjects} data-testid="dashboard-cta-projects">
          {#snippet icon()}<FolderOpen size={13} />{/snippet}
          {$t("dashboard.header_action_projects")}
        </Button>
        <Button variant="primary-solid" size="sm" onclick={navigateToChat} data-testid="dashboard-cta-new-chat">
          {#snippet icon()}<MessageSquarePlus size={13} />{/snippet}
          {$t("dashboard.header_action_new_chat")}
        </Button>
      {/snippet}
    </PageHeader>

    {#if $tasks.length === 0 && $agents.length === 0 && $projects.length === 0}
      <!-- Cold start - welcome the operator. -->
      <div class="px-8 mt-6" data-testid="dashboard-cold-start">
        <LegacyEmptyState
          icon={LayoutDashboard}
          title={$t('dashboard.cold_start_title')}
          subtitle={$t('dashboard.cold_start_subtitle')}
          ctaLabel={$t('dashboard.browse_agents')}
          ctaAction={navigateToAgents}
          page="dashboard"
        />
      </div>
    {:else}
      <!-- Bento attention zone: 3-column responsive grid -->
      <div
        class="px-8 pt-6 grid gap-4 lg:grid-cols-3"
        data-testid="dashboard-bento"
      >
        <!-- Primary card: Décisions en attente (spans 2 cols on lg) -->
        <div class="lg:col-span-2 lg:row-span-2">
          <Card class="overflow-hidden">
            <div class="px-5 pt-4 pb-3 flex items-baseline justify-between border-b border-border/40">
              <div class="flex items-baseline gap-2">
                <h3 class="m-0 text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase font-mono">
                  {$t("dashboard.card_pending_title")}
                </h3>
                <span class="text-[11px] text-muted-foreground/70 font-mono">{totalPending}</span>
              </div>
              {#if totalPending > 0}
                <button
                  type="button"
                  class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
                  onclick={navigateToInbox}
                  aria-label={$t("dashboard.see_all_pending_aria")}
                  data-testid="dashboard-see-all-pending"
                >
                  {$t("dashboard.see_all_arrow")}
                </button>
              {/if}
            </div>

            {#if inboxItems.length === 0}
              <EmptyState
                title={$t("dashboard.pending_empty_title")}
                desc={$t("dashboard.pending_empty_desc")}
                tone="success"
              >
                {#snippet icon()}<FileCheck size={22} />{/snippet}
              </EmptyState>
            {:else}
              <div class="flex flex-col" data-testid="dashboard-pending-list">
                {#each inboxItems.slice(0, PENDING_BLOCK_LIMIT) as item, i (item.id)}
                  <div
                    in:fly={{ y: 6, duration: 200, delay: i * 30 }}
                  >
                    <InboxRow
                      type={inboxKindToType(item)}
                      title={item.summary || "(sans titre)"}
                      agent={item.agentName}
                      timestamp={formatRelativeTime(item.suspendedAt)}
                      unread={i === 0}
                      onAction={navigateToInbox}
                    />
                  </div>
                {/each}
                {#if inboxItems.length > PENDING_BLOCK_LIMIT}
                  <button
                    type="button"
                    class="px-4 py-2.5 text-[11.5px] text-primary hover:bg-muted/40 transition-colors text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
                    onclick={navigateToInbox}
                    data-testid="dashboard-inbox-more"
                  >
                    {$t(
                      inboxItems.length - PENDING_BLOCK_LIMIT > 1
                        ? "dashboard.inbox_more_other"
                        : "dashboard.inbox_more_one",
                      { values: { count: inboxItems.length - PENDING_BLOCK_LIMIT } },
                    )}
                  </button>
                {/if}
              </div>
            {/if}
          </Card>
        </div>

        <!-- Secondary card: Livrables prêts -->
        <Card class="overflow-hidden">
          <div class="px-4 pt-3.5 pb-2 flex items-baseline justify-between">
            <div class="flex items-baseline gap-2">
              <h3 class="m-0 text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase font-mono">
                {$t("dashboard.card_deliverables_title")}
              </h3>
              <span class="text-[11px] text-muted-foreground/70 font-mono">{recentDeliverables.length}</span>
            </div>
          </div>
          {#if recentDeliverables.length === 0}
            <div class="px-4 pb-4 text-[11.5px] text-muted-foreground/80 leading-[1.5]">
              {$t("dashboard.deliverables_empty")}
            </div>
          {:else}
            <div class="flex flex-col px-2 pb-2">
              {#each recentDeliverables as tk (tk.id)}
                <ListRow
                  variant="nav"
                  align="center"
                  onclick={navigateToTasks}
                  aria-label={$t("dashboard.open_task_aria", { values: { name: tk.agent_name } })}
                  data-testid="dashboard-deliverable-{tk.id}"
                >
                  <div
                    class="w-7 h-7 rounded-md inline-flex items-center justify-center shrink-0 bg-success/10 text-success"
                  >
                    <FileCheck size={12} />
                  </div>
                  <div class="flex-1 min-w-0">
                    <div class="text-[12px] font-medium text-foreground truncate">{tk.agent_name}</div>
                    <div class="text-[10.5px] text-muted-foreground truncate">
                      {formatRelativeTime(tk.created_at)}
                    </div>
                  </div>
                </ListRow>
              {/each}
            </div>
          {/if}
        </Card>

        <!-- Secondary card: Agents au travail -->
        <Card class="overflow-hidden">
          <div class="px-4 pt-3.5 pb-2 flex items-baseline justify-between">
            <div class="flex items-baseline gap-2">
              <h3 class="m-0 text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase font-mono">
                {$t("dashboard.card_agents_title")}
              </h3>
              <span class="text-[11px] text-muted-foreground/70 font-mono">{agentsAtWork.length}</span>
            </div>
          </div>
          {#if agentsAtWork.length === 0}
            <div class="px-4 pb-4 text-[11.5px] text-muted-foreground/80 leading-[1.5]">
              {$t("dashboard.agents_empty")}
            </div>
          {:else}
            <div class="flex flex-col px-2 pb-2 gap-1">
              {#each agentsAtWork.slice(0, 5) as agent (agent.name)}
                <ListRow
                  variant="nav"
                  align="center"
                  onclick={() => openDetail(agent)}
                  aria-label={$t("dashboard.agent_detail_aria", { values: { name: agent.name } })}
                  data-testid="dashboard-agent-row-{agent.name}"
                >
                  <div
                    class="w-5 h-5 rounded-md inline-flex items-center justify-center shrink-0 bg-gradient-to-br from-primary to-secondary"
                  >
                    <Sparkles size={10} class="text-white" />
                  </div>
                  <div class="flex-1 min-w-0">
                    <span class="text-[12px] font-medium text-foreground truncate block">{agent.name}</span>
                    <BuilderOnly>
                      <span class="text-[10px] text-muted-foreground/70 font-mono truncate block" data-testid="dashboard-agent-tech-line">
                        v{agent.version}{#if agent.tags.length > 0} · {agent.tags.slice(0, 3).join(" · ")}{/if}{#if agent.agent_type === "worker"} · WORKER{/if}
                      </span>
                    </BuilderOnly>
                  </div>
                  <StatusDot
                    color={agent.runtime_status === "active" ? "hsl(var(--primary))" : "hsl(var(--warning))"}
                    glow={agent.runtime_status === "active"}
                  />
                </ListRow>
              {/each}
              {#if agentsAtWork.length > 5}
                <Badge size="sm" variant="neutral">+ {agentsAtWork.length - 5}</Badge>
              {/if}
            </div>
          {/if}
        </Card>
      </div>

      <!-- Activité récente strip -->
      <div class="px-8 pt-2">
        <SectionTitle count={recentActivityFeed.length}>
          {$t("dashboard.recent_activity_title")}
          {#snippet action()}
            <button
              type="button"
              class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
              onclick={navigateToTasks}
              data-testid="dashboard-recent-see-all"
            >
              {$t("dashboard.see_all_short_arrow")}
            </button>
          {/snippet}
        </SectionTitle>
        {#if recentActivityFeed.length === 0}
          <div class="px-8 pb-2 text-[11.5px] text-muted-foreground/80">
            {$t('dashboard.no_recent_activity')}
          </div>
        {:else}
          <div class="px-8 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
            {#each recentActivityFeed as tk (tk.id)}
              <Card hover class="">
                <Button variant="ghost"
                  size="auto"
                  type="button"
                  class="w-full text-left px-3 py-2.5 flex-col items-stretch focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded-xl"
                  onclick={navigateToTasks}
                  data-testid="dashboard-activity-{tk.id}"
                >
                  <div class="flex items-center gap-1.5 mb-1">
                    <Activity size={10} class="text-muted-foreground" />
                    <span class="text-[10.5px] font-mono uppercase tracking-wider text-muted-foreground">
                      {tk.status}
                    </span>
                  </div>
                  <div class="text-[12.5px] font-medium text-foreground truncate">{tk.agent_name}</div>
                  <div class="text-[10.5px] text-muted-foreground/80 mt-0.5">
                    {formatRelativeTime(tk.created_at)}
                  </div>
                </Button>
              </Card>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Pinned projects strip (horizontal scroll on overflow) -->
      <div class="pt-4 pb-8">
        <SectionTitle count={pinnedProjects.length}>
          {$t("dashboard.pinned_projects_title")}
          {#snippet action()}
            <button
              type="button"
              class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
              onclick={navigateToProjects}
              data-testid="dashboard-projects-see-all"
            >
              {$t("dashboard.see_all_projects_arrow")}
            </button>
          {/snippet}
        </SectionTitle>
        {#if pinnedProjects.length === 0}
          <div class="px-8">
            <EmptyState
              title={$t("dashboard.projects_empty_title")}
              desc={$t("dashboard.projects_empty_desc")}
              tone="info"
            >
              {#snippet icon()}<FolderOpen size={22} />{/snippet}
              {#snippet action()}
                <Button variant="outline" size="sm" onclick={navigateToProjects}>
                  {$t("dashboard.open_projects")}
                </Button>
              {/snippet}
            </EmptyState>
          </div>
        {:else}
          <div
            class="px-8 flex gap-3 overflow-x-auto pb-2"
            style="scrollbar-width: thin;"
            data-testid="dashboard-pinned-projects"
          >
            {#each pinnedProjects as p, i (p.id)}
              <div
                class="shrink-0 w-[260px]"
                animate:flip={{ duration: 200 }}
                in:fly={{ y: 6, duration: 200, delay: i * 30 }}
                data-testid="dashboard-project-{p.id}"
              >
                <ProjectCard
                  title={p.name}
                  description={p.description ?? undefined}
                  status={projectStatus(p)}
                  lastActivity={formatRelativeTime(p.updated_at)}
                  hover
                  onclick={navigateToProjects}
                />
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
