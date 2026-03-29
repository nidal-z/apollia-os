<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { MessageSquare, Plus, Loader2, Bot, X, Terminal, FileText, Code } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions, pendingChatSessionId } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary, CreateSessionRequest, AgentListItem } from "$lib/types";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ChatConversation from "../components/chat/ChatConversation.svelte";
  import ChatSessionCard from "../components/chat/ChatSessionCard.svelte";

  let selectedSessionId = $state<string | null>(null);
  let creating = $state(false);
  let showNewChatPicker = $state(false);
  let agents = $state<AgentListItem[]>([]);
  let loadingAgents = $state(false);
  let toolBash = $state(true);
  let toolFileIo = $state(true);
  let toolPython = $state(false);

  const selectedTools = $derived.by(() => {
    const tools: string[] = [];
    if (toolBash) tools.push("bash_executor");
    if (toolFileIo) tools.push("file_io");
    if (toolPython) tools.push("python_executor");
    return tools;
  });

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
    try { agents = await invoke("list_agents"); } catch { agents = []; }
    finally { loadingAgents = false; }
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

      <div class="flex items-center gap-2 flex-wrap">
        <!-- Tool toggles -->
        {#each [
          { id: "bash", checked: toolBash, icon: Terminal, label: "bash" },
          { id: "file", checked: toolFileIo, icon: FileText, label: "file_io" },
          { id: "python", checked: toolPython, icon: Code, label: "python" },
        ] as tool (tool.id)}
          <label class="flex items-center gap-1.5 rounded-md px-2.5 py-1.5 cursor-pointer text-[11px] font-medium transition-all
            {tool.checked ? 'bg-primary/10 text-primary ring-1 ring-primary/20' : 'bg-muted/40 text-muted-foreground hover:bg-muted/60'}">
            <input
              type="checkbox"
              checked={tool.checked}
              onchange={() => {
                if (tool.id === "bash") toolBash = !toolBash;
                else if (tool.id === "file") toolFileIo = !toolFileIo;
                else toolPython = !toolPython;
              }}
              class="sr-only"
              data-testid="pick-tool-{tool.id}"
            />
            <tool.icon size={12} />
            {tool.label}
          </label>
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

        <!-- Separator + agents -->
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
