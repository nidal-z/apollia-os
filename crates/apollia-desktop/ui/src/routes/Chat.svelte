<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { MessageSquare, Plus, Loader2, Bot, X, ChevronDown, Zap } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions, pendingChatSessionId } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary, CreateSessionRequest, AgentListItem, A2ASkillView } from "$lib/types";
  import { TOOL_GROUPS, TOOL_CATALOG, DEFAULT_ENABLED_TOOLS, getGroupState, toggleGroup } from "$lib/tools/tool-catalog";
  import { uiMode } from "$lib/stores/mode";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ChatConversation from "../components/chat/ChatConversation.svelte";
  import ChatSessionCard from "../components/chat/ChatSessionCard.svelte";

  let selectedSessionId = $state<string | null>(null);
  let creating = $state(false);
  let showNewChatPicker = $state(false);
  let agents = $state<AgentListItem[]>([]);
  let loadingAgents = $state(false);
  let a2aSkills = $state<A2ASkillView[]>([]);
  let enabledTools = $state(new Set<string>(DEFAULT_ENABLED_TOOLS));
  const selectedTools = $derived(Array.from(enabledTools));
  const isOperator = $derived($uiMode === "operator");
  let expandedGroup = $state<string | null>(null);

  const activeAgents = $derived(agents.filter((a) => a.runtime_status === "active"));

  onMount(() => {
    const unsub = pendingChatSessionId.subscribe((id) => {
      if (id) { selectedSessionId = id; pendingChatSessionId.set(null); }
    });
    return unsub;
  });

  function openNewChatPicker() { showNewChatPicker = true; void loadAgents(); }
  function closeNewChatPicker() { showNewChatPicker = false; }

  async function loadAgents(): Promise<void> {
    loadingAgents = true;
    try {
      [agents, a2aSkills] = await Promise.all([
        invoke<AgentListItem[]>("list_agents").catch(() => []),
        invoke<A2ASkillView[]>("list_a2a_skills").catch(() => []),
      ]);
    } finally {
      loadingAgents = false;
    }
  }

  /** Worker agents grouped with status from agents list. */
  const workerAgentGroups = $derived(
    (() => {
      const byName: Record<string, { skills: A2ASkillView[]; status: string | null; installPath: string | null }> = {};
      for (const s of a2aSkills) {
        if (!byName[s.agent_name]) {
          const agentEntry = agents.find((a) => a.name === s.agent_name);
          byName[s.agent_name] = {
            skills: [],
            status: agentEntry?.runtime_status ?? null,
            installPath: agentEntry?.install_path ?? null,
          };
        }
        byName[s.agent_name].skills.push(s);
      }
      return Object.entries(byName);
    })()
  );

  let startingWorker = $state<string | null>(null);

  async function startWorker(installPath: string, agentName: string): Promise<void> {
    startingWorker = agentName;
    try {
      await invoke("start_agent", { path: installPath });
      // Reload to pick up new status
      await loadAgents();
    } catch { /* errors surface via runtime events */ }
    finally { startingWorker = null; }
  }

  async function createLibreChat(): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = { mode: "libre", tools: selectedTools };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      selectedSessionId = session.id; showNewChatPicker = false;
    } catch { /* user can retry */ }
    finally { creating = false; }
  }

  async function createAgentChat(agentName: string): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      selectedSessionId = session.id; showNewChatPicker = false;
    } catch { /* user can retry */ }
    finally { creating = false; }
  }

  function navigateToSession(sessionId: string) { selectedSessionId = sessionId; }
  function closeConversation() { selectedSessionId = null; }

  async function handleDeleteSession(sessionId: string): Promise<void> {
    // Optimistic update: remove from store immediately
    chatSessions.update((sessions) => sessions.filter((s) => s.id !== sessionId));
    if (selectedSessionId === sessionId) selectedSessionId = null;
    try { await invoke("delete_chat_session", { sessionId }); }
    catch (err: unknown) { console.warn("delete_chat_session failed:", err); }
  }

  async function handleRenameSession(sessionId: string, title: string): Promise<void> {
    // Optimistic update: patch title in store immediately
    chatSessions.update((sessions) =>
      sessions.map((s) => (s.id === sessionId ? { ...s, title } : s))
    );
    try { await invoke("rename_chat_session", { sessionId, title }); }
    catch (err: unknown) { console.warn("rename_chat_session failed:", err); }
  }
</script>

<div class="max-w-6xl" data-testid="chat-page">
  <!-- Header -->
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight" data-testid="chat-header">{$t("chat.title")}</h1>
      <p class="mt-1 text-sm text-muted-foreground" data-testid="chat-subtitle">{$t("chat.subtitle")}</p>
    </div>
    <Button size="sm" onclick={openNewChatPicker} disabled={creating} data-testid="new-chat-button" class="gap-1.5">
      <Plus size={13} />
      {$t("chat.new_chat")}
    </Button>
  </div>

  <!-- New chat picker -->
  {#if showNewChatPicker}
    <div class="mt-4 glass-card glass-border rounded-lg p-4 animate-fade-in" data-testid="new-chat-picker">
      <div class="flex items-center justify-between mb-3">
        <span class="text-xs font-medium">{$t("chat.pick_type")}</span>
        <button onclick={closeNewChatPicker} class="h-6 w-6 inline-flex items-center justify-center rounded text-muted-foreground/50 hover:text-foreground transition-colors" aria-label="Close" data-testid="chat-picker-close">
          <X size={13} />
        </button>
      </div>

      <div class="space-y-2">
        <!-- Row 1: group chips + Start button + agents -->
        <div class="flex items-center gap-2 flex-wrap">
          {#each TOOL_GROUPS as group (group.id)}
            {@const state = getGroupState(group, enabledTools)}
            {@const GroupIcon = group.icon}
            {@const partialCount = group.tools.filter(id => enabledTools.has(id)).length}
            {@const isExpanded = expandedGroup === group.id}
            <button
              class="flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-[11px] font-medium transition-all
                {state === 'all'
                  ? `bg-primary/10 text-primary ring-1 ${isExpanded ? 'ring-primary/40' : 'ring-primary/20'}`
                  : state === 'some'
                  ? `bg-primary/5 text-primary/60 ring-1 ${isExpanded ? 'ring-primary/30' : 'ring-primary/10'}`
                  : isExpanded
                  ? 'bg-muted/60 text-foreground ring-1 ring-border/60'
                  : 'bg-muted/40 text-muted-foreground hover:bg-muted/60'}"
              onclick={() => { expandedGroup = isExpanded ? null : group.id; }}
              data-testid="pick-group-{group.id}"
            >
              <GroupIcon size={12} />
              {isOperator ? $t(group.labelOperatorKey) : $t(group.labelBuilderKey)}
              {#if state === "some"}
                <span class="opacity-50 text-[10px]">{partialCount}/{group.tools.length}</span>
              {:else if state === "all" && !isOperator}
                <span class="opacity-50 text-[10px]">{group.tools.length}</span>
              {/if}
              <ChevronDown
                size={10}
                class="transition-transform duration-150 {isExpanded ? 'rotate-180' : ''}"
              />
            </button>
          {/each}

          <button
            class="flex items-center gap-1.5 rounded-md bg-primary text-primary-foreground px-3 py-1.5 text-[11px] font-medium
              transition-colors hover:bg-primary/90 active:scale-[0.97] disabled:opacity-50 disabled:pointer-events-none"
            onclick={createLibreChat}
            disabled={creating}
            data-testid="pick-libre"
          >
            {#if creating}<Loader2 size={12} class="animate-spin" />{:else}<MessageSquare size={12} />{/if}
            {$t("chat.start")}
          </button>

          {#if !loadingAgents && activeAgents.length > 0}
            <span class="text-muted-foreground/30 mx-1">|</span>
            {#each activeAgents as agent (agent.name)}
              <button
                class="flex items-center gap-1.5 rounded-md bg-muted/40 px-2.5 py-1.5 text-[11px] font-medium
                  transition-all hover:bg-primary/10 hover:text-primary active:scale-[0.97] disabled:opacity-50"
                onclick={() => createAgentChat(agent.name)}
                disabled={creating}
                data-testid="pick-agent-{agent.name}"
              >
                <Bot size={12} />
                {agent.name}
              </button>
            {/each}
          {/if}
        </div>

        <!-- Row 1b: Worker agents (A2A) with status -->
        {#if workerAgentGroups.length > 0}
          <div class="flex flex-col gap-1.5 pt-2 border-t border-border/15">
            <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/40">
              {$t("chat.a2a_workers")}
            </span>
            {#each workerAgentGroups as [agentName, { skills, status, installPath }] (agentName)}
              {@const isActive = status === "active" || status === "degraded"}
              <div class="flex items-center gap-2 flex-wrap">
                <!-- Status dot + name -->
                <div class="flex items-center gap-1.5">
                  <span class="h-1.5 w-1.5 rounded-full {isActive ? 'bg-secondary/80' : 'bg-muted-foreground/30'}"></span>
                  <span class="text-[10px] font-medium {isActive ? 'text-secondary/80' : 'text-muted-foreground/50'} flex items-center gap-0.5">
                    <Zap size={9} />
                    {agentName}
                  </span>
                </div>
                <!-- Skills -->
                {#if isActive}
                  {#each skills as skill (skill.skill_id)}
                    <span
                      class="rounded px-1.5 py-0.5 text-[10px] bg-secondary/10 text-secondary/80"
                      title={skill.description}
                      data-testid="a2a-skill-{skill.skill_id}"
                    >
                      {skill.skill_name}
                    </span>
                  {/each}
                {:else}
                  <span class="text-[10px] text-muted-foreground/40 italic">{$t("chat.a2a_worker_stopped")}</span>
                  {#if installPath}
                    <button
                      class="flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] bg-primary/10 text-primary/70 hover:bg-primary/20 transition-colors disabled:opacity-40"
                      onclick={() => startWorker(installPath, agentName)}
                      disabled={startingWorker === agentName}
                      data-testid="a2a-start-worker-{agentName}"
                    >
                      {startingWorker === agentName ? $t("agents.starting_agent") : $t("chat.a2a_start_worker")}
                    </button>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <!-- Row 2: expanded group detail (accordion) -->
        {#if expandedGroup}
          {@const group = TOOL_GROUPS.find(g => g.id === expandedGroup)}
          {#if group}
            {@const groupTools = TOOL_CATALOG.filter(t => t.group === group.id)}
            {@const groupState = getGroupState(group, enabledTools)}
            <div class="flex flex-wrap items-center gap-1.5 pt-2 pl-1 border-t border-border/20 animate-fade-in">
              <!-- Individual tool chips -->
              {#each groupTools as tool (tool.id)}
                {@const checked = enabledTools.has(tool.id)}
                {@const ToolIcon = tool.icon}
                <label
                  class="flex items-center gap-1.5 rounded-md px-2 py-1 cursor-pointer text-[11px] font-medium transition-all
                    {checked
                      ? 'bg-primary/10 text-primary ring-1 ring-primary/20'
                      : 'bg-muted/30 text-muted-foreground hover:bg-muted/50'}"
                  title={isOperator ? tool.id : $t(tool.descBuilderKey)}
                  data-testid="pick-tool-{tool.id}"
                >
                  <input
                    type="checkbox"
                    {checked}
                    onchange={() => {
                      const next = new Set(enabledTools);
                      if (next.has(tool.id)) next.delete(tool.id); else next.add(tool.id);
                      enabledTools = next;
                    }}
                    class="sr-only"
                  />
                  <ToolIcon size={11} />
                  {isOperator ? $t(tool.labelOperatorKey) : tool.id}
                </label>
              {/each}
              <!-- Quick all/none -->
              <button
                class="ml-auto text-[10px] text-muted-foreground/40 hover:text-primary transition-colors px-1"
                onclick={() => { enabledTools = toggleGroup(group, enabledTools); }}
              >
                {groupState === "all" ? "− none" : "+ all"}
              </button>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  {/if}

  <!-- Main content -->
  <div class="mt-4">
    {#if $connectionStatus === "connecting"}
      <div class="space-y-1">
        {#each { length: 4 } as _}
          <div class="flex items-center gap-3 rounded-lg px-3 py-2.5">
            <Skeleton class="h-3.5 w-3.5 rounded" />
            <Skeleton class="h-3.5 w-28" />
            <Skeleton class="h-3 flex-1" />
            <Skeleton class="h-3 w-12" />
          </div>
        {/each}
      </div>
    {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0 && !showNewChatPicker && !selectedSessionId}
      <EmptyState
        icon={MessageSquare}
        title={$t("chat.empty_title")}
        subtitle={$t("chat.empty_subtitle")}
        ctaLabel={$t("chat.new_chat")}
        ctaAction={openNewChatPicker}
        page="chat"
      />
    {:else if selectedSessionId}
      <!-- Two-column: session sidebar + conversation -->
      <div class="flex gap-0" style="height: calc(100vh - 180px);">
        <!-- Session sidebar -->
        <div class="w-60 shrink-0 flex flex-col border-r border-border/30 pr-2 overflow-hidden">
          <!-- New chat button in sidebar -->
          <button
            class="mb-2 flex items-center gap-1.5 rounded-lg px-3 py-2 text-[11px] font-medium
              text-muted-foreground bg-muted/30 hover:bg-muted/50 hover:text-foreground
              transition-all active:scale-[0.98] w-full"
            onclick={openNewChatPicker}
            data-testid="sidebar-new-chat"
          >
            <Plus size={12} />
            {$t("chat.new_chat")}
          </button>

          {#if $activeChatSessions.length > 0}
            <p class="px-3 pt-1 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-primary/50">{$t("chat.active_sessions")}</p>
            <div class="space-y-1">
              {#each $activeChatSessions as session (session.id)}
                <ChatSessionCard
                  {session}
                  selected={selectedSessionId === session.id}
                  onclick={navigateToSession}
                  ondelete={handleDeleteSession}
                  onrename={handleRenameSession}
                />
              {/each}
            </div>
          {/if}

          {#if $closedChatSessions.length > 0}
            <p class="px-3 pt-3 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/40">{$t("chat.closed_sessions")}</p>
            <div class="space-y-1 overflow-y-auto">
              {#each $closedChatSessions as session (session.id)}
                <ChatSessionCard
                  {session}
                  selected={selectedSessionId === session.id}
                  onclick={navigateToSession}
                  ondelete={handleDeleteSession}
                  onrename={handleRenameSession}
                />
              {/each}
            </div>
          {/if}
        </div>

        <!-- Conversation — glass card -->
        <div class="flex-1 min-w-0 ml-2 glass-card glass-border rounded-lg overflow-hidden">
          <ChatConversation sessionId={selectedSessionId} onclose={closeConversation} />
        </div>
      </div>
    {:else}
      <!-- Session list (no session selected) — card list style -->
      {#if $activeChatSessions.length > 0}
        <p class="px-1 pb-2 text-[10px] font-semibold uppercase tracking-wider text-primary/50" data-testid="chat-active-section">
          {$t("chat.active_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20 shadow-sm" data-testid="chat-active-list">
          {#each $activeChatSessions as session (session.id)}
            <ChatSessionCard
              {session}
              onclick={navigateToSession}
              ondelete={handleDeleteSession}
              onrename={handleRenameSession}
            />
          {/each}
        </div>
      {/if}

      {#if $closedChatSessions.length > 0}
        <p class="px-1 pt-4 pb-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/40" data-testid="chat-closed-section">
          {$t("chat.closed_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20" data-testid="chat-closed-list">
          {#each $closedChatSessions as session (session.id)}
            <ChatSessionCard
              {session}
              onclick={navigateToSession}
              ondelete={handleDeleteSession}
              onrename={handleRenameSession}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>
