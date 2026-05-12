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
    BtnPrimary,
    BtnSecondary,
    Chip,
    StatusDot,
    Card,
    EmptyState,
    InboxRow,
    ProjectCard,
    type InboxType,
  } from "$lib/components/operator";

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
      return `Vous avez ${totalPending} décision${totalPending > 1 ? "s" : ""} en attente.`;
    }
    if (agentsAtWork.length > 0) {
      return `${agentsAtWork.length} agent${agentsAtWork.length > 1 ? "s sont" : " est"} au travail pour vous.`;
    }
    return `${greeting}.`;
  });

  const headlineSubtitle = $derived.by(() => {
    const parts: string[] = [];
    if (totalPending > 0) parts.push(`${totalPending} décision${totalPending > 1 ? "s" : ""} en attente`);
    if (agentsAtWork.length > 0) parts.push(`${agentsAtWork.length} agent${agentsAtWork.length > 1 ? "s" : ""} au travail`);
    if (recentDeliverables.length > 0) parts.push(`${recentDeliverables.length} livrable${recentDeliverables.length > 1 ? "s" : ""} prêt${recentDeliverables.length > 1 ? "s" : ""}`);
    if (parts.length === 0) return "Tout est calme. Lancez une conversation quand vous êtes prêt.";
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

<!-- Unified dashboard — Operator layout shared by both modes; the Builder mode
     receives optional technical overlays via `BuilderOnly` blocks below. -->
<div
  class="mx-auto w-full max-w-6xl"
  data-testid="dashboard-page"
  data-mode={$uiMode}
>
    <PageHeader
      kicker={`TABLEAU DE BORD${todayLabel ? ` · ${todayLabel}` : ""}`}
      title={headlineTitle}
      subtitle={headlineSubtitle}
    >
      {#snippet actions()}
        <BtnSecondary onclick={navigateToProjects}>
          {#snippet icon()}<FolderOpen size={13} />{/snippet}
          Projets
        </BtnSecondary>
        <BtnPrimary onclick={navigateToChat}>
          {#snippet icon()}<MessageSquarePlus size={13} />{/snippet}
          Nouvelle conversation
        </BtnPrimary>
      {/snippet}
    </PageHeader>

    {#if $tasks.length === 0 && $agents.length === 0 && $projects.length === 0}
      <!-- Cold start — welcome the operator. -->
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
                  Décisions en attente
                </h3>
                <span class="text-[11px] text-muted-foreground/70 font-mono">{totalPending}</span>
              </div>
              {#if totalPending > 0}
                <button
                  type="button"
                  class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
                  onclick={navigateToInbox}
                  aria-label="Voir toutes les décisions en attente"
                >
                  Voir tout →
                </button>
              {/if}
            </div>

            {#if inboxItems.length === 0}
              <EmptyState
                title="Rien ne vous attend"
                desc="Vous êtes à jour. Les décisions en attente apparaîtront ici."
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
                      onclick={navigateToInbox}
                      onAction={navigateToInbox}
                    />
                  </div>
                {/each}
                {#if inboxItems.length > PENDING_BLOCK_LIMIT}
                  <button
                    type="button"
                    class="px-4 py-2.5 text-[11.5px] text-primary hover:bg-muted/40 transition-colors text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
                    onclick={navigateToInbox}
                  >
                    + {inboxItems.length - PENDING_BLOCK_LIMIT} autre{inboxItems.length - PENDING_BLOCK_LIMIT > 1 ? "s" : ""} dans la boîte de réception →
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
                Livrables prêts
              </h3>
              <span class="text-[11px] text-muted-foreground/70 font-mono">{recentDeliverables.length}</span>
            </div>
          </div>
          {#if recentDeliverables.length === 0}
            <div class="px-4 pb-4 text-[11.5px] text-muted-foreground/80 leading-[1.5]">
              Aucun livrable terminé aujourd'hui.
            </div>
          {:else}
            <div class="flex flex-col px-2 pb-2">
              {#each recentDeliverables as tk (tk.id)}
                <button
                  type="button"
                  class="text-left flex items-center gap-2.5 px-2 py-2 rounded-lg hover:bg-muted/40 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
                  onclick={navigateToTasks}
                  aria-label={`Ouvrir la tâche ${tk.agent_name}`}
                >
                  <div
                    class="w-7 h-7 rounded-md inline-flex items-center justify-center shrink-0"
                    style="background: hsl(var(--success) / 0.10); color: hsl(var(--success));"
                  >
                    <FileCheck size={12} />
                  </div>
                  <div class="flex-1 min-w-0">
                    <div class="text-[12px] font-medium text-foreground truncate">{tk.agent_name}</div>
                    <div class="text-[10.5px] text-muted-foreground truncate">
                      {formatRelativeTime(tk.created_at)}
                    </div>
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </Card>

        <!-- Secondary card: Agents au travail -->
        <Card class="overflow-hidden">
          <div class="px-4 pt-3.5 pb-2 flex items-baseline justify-between">
            <div class="flex items-baseline gap-2">
              <h3 class="m-0 text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase font-mono">
                Au travail
              </h3>
              <span class="text-[11px] text-muted-foreground/70 font-mono">{agentsAtWork.length}</span>
            </div>
          </div>
          {#if agentsAtWork.length === 0}
            <div class="px-4 pb-4 text-[11.5px] text-muted-foreground/80 leading-[1.5]">
              Aucun agent actif pour l'instant.
            </div>
          {:else}
            <div class="flex flex-col px-2 pb-2 gap-1">
              {#each agentsAtWork.slice(0, 5) as agent (agent.name)}
                <button
                  type="button"
                  class="text-left flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-muted/40 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
                  onclick={() => openDetail(agent)}
                  aria-label={`Détails de l'agent ${agent.name}`}
                >
                  <div
                    class="w-5 h-5 rounded-md inline-flex items-center justify-center shrink-0"
                    style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
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
                </button>
              {/each}
              {#if agentsAtWork.length > 5}
                <Chip size="sm" tone="neutral">+ {agentsAtWork.length - 5}</Chip>
              {/if}
            </div>
          {/if}
        </Card>
      </div>

      <!-- Activité récente strip -->
      <div class="px-8 pt-2">
        <SectionTitle count={recentActivityFeed.length}>
          Activité récente
          {#snippet action()}
            <button
              type="button"
              class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
              onclick={navigateToTasks}
            >
              Tout voir →
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
                <button
                  type="button"
                  class="w-full text-left px-3 py-2.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded-xl"
                  onclick={navigateToTasks}
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
                </button>
              </Card>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Pinned projects strip (horizontal scroll on overflow) -->
      <div class="pt-4 pb-8">
        <SectionTitle count={pinnedProjects.length}>
          Projets épinglés
          {#snippet action()}
            <button
              type="button"
              class="text-[11.5px] text-primary hover:text-primary/80 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 rounded"
              onclick={navigateToProjects}
            >
              Tous les projets →
            </button>
          {/snippet}
        </SectionTitle>
        {#if pinnedProjects.length === 0}
          <div class="px-8">
            <EmptyState
              title="Pas encore de projet"
              desc="Créez un projet pour grouper conversations, agents et livrables."
              tone="info"
            >
              {#snippet icon()}<FolderOpen size={22} />{/snippet}
              {#snippet action()}
                <BtnSecondary onclick={navigateToProjects}>
                  Ouvrir Projets
                </BtnSecondary>
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
