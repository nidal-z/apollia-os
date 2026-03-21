<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { MessageSquare, Plus, Loader2, Bot, X, Terminal, FileText, Code } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions, pendingChatSessionId } from "$lib/stores/chat";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary, CreateSessionRequest, AgentListItem } from "$lib/types";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ChatConversation from "../components/chat/ChatConversation.svelte";

  const SKELETON_COUNT = 4;

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

  const activeAgents = $derived(
    agents.filter((a) => a.runtime_status === "active"),
  );

  onMount(() => {
    const unsub = pendingChatSessionId.subscribe((id) => {
      if (id) {
        selectedSessionId = id;
        pendingChatSessionId.set(null);
      }
    });
    return unsub;
  });

  function openNewChatPicker() {
    showNewChatPicker = true;
    void loadAgents();
  }

  function closeNewChatPicker() {
    showNewChatPicker = false;
  }

  async function loadAgents(): Promise<void> {
    loadingAgents = true;
    try {
      agents = await invoke("list_agents");
    } catch {
      agents = [];
    } finally {
      loadingAgents = false;
    }
  }

  async function createLibreChat(): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = { mode: "libre", tools: selectedTools };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      selectedSessionId = session.id;
      showNewChatPicker = false;
    } catch {
      // user can retry
    } finally {
      creating = false;
    }
  }

  async function createAgentChat(agentName: string): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      selectedSessionId = session.id;
      showNewChatPicker = false;
    } catch {
      // user can retry
    } finally {
      creating = false;
    }
  }

  function navigateToSession(sessionId: string) {
    selectedSessionId = sessionId;
  }

  function closeConversation() {
    selectedSessionId = null;
  }

  function formatRelativeTime(dateStr: string): string {
    const now = Date.now();
    const created = new Date(dateStr).getTime();
    const diffMinutes = Math.floor((now - created) / 60_000);
    if (diffMinutes < 1) return $t("chat.just_now");
    if (diffMinutes < 60) return $t("chat.minutes_ago", { values: { n: diffMinutes } });
    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) return $t("chat.hours_ago", { values: { n: diffHours } });
    const diffDays = Math.floor(diffHours / 24);
    return $t("chat.days_ago", { values: { n: diffDays } });
  }

  function truncate(str: string | null, max: number): string {
    if (!str) return $t("chat.no_messages");
    return str.length > max ? str.slice(0, max) + "\u2026" : str;
  }
</script>

{#if selectedSessionId}
  <div class="-m-6 h-[calc(100%+3rem)]">
    <ChatConversation sessionId={selectedSessionId} onclose={closeConversation} />
  </div>
{:else}
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="space-y-1">
        <h1 class="text-2xl font-bold" data-testid="chat-header">{$t("chat.title")}</h1>
        <p class="text-sm text-muted-foreground" data-testid="chat-subtitle">{$t("chat.subtitle")}</p>
      </div>
      <Button onclick={openNewChatPicker} disabled={creating} data-testid="new-chat-button">
        {#if creating}
          <Loader2 class="mr-2 h-4 w-4 animate-spin" />
        {:else}
          <Plus class="mr-2 h-4 w-4" />
        {/if}
        {$t("chat.new_chat")}
      </Button>
    </div>

    <!-- New chat picker (inline) -->
    {#if showNewChatPicker}
      <div class="rounded-xl glass-card glass-border border p-4 space-y-3 animate-fade-in" data-testid="new-chat-picker">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-semibold">{$t("chat.pick_type")}</h3>
          <button
            onclick={closeNewChatPicker}
            class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-foreground hover:glass-inset transition-all"
          >
            <X class="h-3.5 w-3.5" />
          </button>
        </div>

        <!-- Free chat section -->
        <div class="space-y-2.5">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{$t("chat.mode_libre")}</p>
          <div class="flex items-center gap-2">
            <!-- Tool toggles -->
            <label class="flex items-center gap-2 rounded-xl px-3 py-2 cursor-pointer transition-all text-xs font-medium
              {toolBash ? 'glass-card ring-1 ring-primary/30 text-foreground' : 'glass-inset text-muted-foreground hover:glass-surface'}">
              <input type="checkbox" bind:checked={toolBash} class="sr-only" data-testid="pick-tool-bash" />
              <Terminal class="h-3.5 w-3.5" />
              bash
            </label>
            <label class="flex items-center gap-2 rounded-xl px-3 py-2 cursor-pointer transition-all text-xs font-medium
              {toolFileIo ? 'glass-card ring-1 ring-primary/30 text-foreground' : 'glass-inset text-muted-foreground hover:glass-surface'}">
              <input type="checkbox" bind:checked={toolFileIo} class="sr-only" data-testid="pick-tool-file" />
              <FileText class="h-3.5 w-3.5" />
              file_io
            </label>
            <label class="flex items-center gap-2 rounded-xl px-3 py-2 cursor-pointer transition-all text-xs font-medium
              {toolPython ? 'glass-card ring-1 ring-primary/30 text-foreground' : 'glass-inset text-muted-foreground hover:glass-surface'}">
              <input type="checkbox" bind:checked={toolPython} class="sr-only" data-testid="pick-tool-python" />
              <Code class="h-3.5 w-3.5" />
              python
            </label>
            <!-- Start button -->
            <button
              class="flex items-center gap-2 rounded-xl bg-gradient-to-r from-primary to-secondary text-white px-4 py-2 text-xs font-medium
                shadow-sm transition-all hover:shadow-md hover:scale-[1.02] active:scale-95
                disabled:opacity-50 disabled:pointer-events-none"
              onclick={createLibreChat}
              disabled={creating}
              data-testid="pick-libre"
            >
              {#if creating}
                <Loader2 class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <MessageSquare class="h-3.5 w-3.5" />
              {/if}
              {$t("chat.start")}
            </button>
          </div>
        </div>

        <!-- Agent section -->
        {#if loadingAgents}
          <div class="flex items-center gap-2 py-2 text-sm text-muted-foreground">
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
          </div>
        {:else if activeAgents.length > 0}
          <div class="space-y-2.5">
            <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{$t("chat.pick_agent_section")}</p>
            <div class="flex flex-wrap gap-2">
              {#each activeAgents as agent (agent.name)}
                <button
                  class="flex items-center gap-2 rounded-xl glass-surface glass-border border px-4 py-2.5 text-sm font-medium
                    transition-all hover:ring-2 hover:ring-primary/30 hover:shadow-sm active:scale-95
                    disabled:opacity-50 disabled:pointer-events-none"
                  onclick={() => createAgentChat(agent.name)}
                  disabled={creating}
                  data-testid="pick-agent-{agent.name}"
                >
                  <Bot class="h-4 w-4 text-primary" />
                  {agent.name}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Sessions list or empty state -->
    {#if $connectionStatus === "connecting"}
      <div class="space-y-2" data-testid="chat-skeleton">
        {#each { length: SKELETON_COUNT } as _}
          <div class="rounded-xl glass-card p-3 flex items-center gap-3">
            <Skeleton class="h-8 w-8 rounded-lg shrink-0" />
            <div class="flex-1 space-y-1">
              <Skeleton class="h-4 w-[40%]" />
              <Skeleton class="h-3 w-[70%]" />
            </div>
            <Skeleton class="h-5 w-14 rounded-full" />
          </div>
        {/each}
      </div>
    {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0 && !showNewChatPicker}
      <EmptyState
        icon={MessageSquare}
        title={$t("chat.empty_title")}
        subtitle={$t("chat.empty_subtitle")}
        ctaLabel={$t("chat.new_chat")}
        ctaAction={openNewChatPicker}
        page="chat"
      />
    {:else}
      <!-- Active sessions -->
      {#if $activeChatSessions.length > 0}
        <div>
          <h2 class="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground" data-testid="chat-active-section">
            {$t("chat.active_sessions")}
          </h2>
          <div class="space-y-1" data-testid="chat-active-list">
            {#each $activeChatSessions as session (session.id)}
              <button
                class="group w-full flex items-center gap-3 rounded-xl px-3 py-2.5 text-left
                  transition-all hover:glass-surface active:scale-[0.995]"
                onclick={() => navigateToSession(session.id)}
                data-testid="chat-session-{session.id}"
              >
                <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg glass-inset">
                  {#if session.mode === "agent"}
                    <Bot class="h-3.5 w-3.5 text-primary" />
                  {:else}
                    <MessageSquare class="h-3.5 w-3.5 text-secondary" />
                  {/if}
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium truncate">
                      {session.mode === "agent" && session.agent_name ? session.agent_name : $t("chat.mode_libre")}
                    </span>
                    <Badge
                      variant={session.mode === "agent" ? "default" : "secondary"}
                      class="text-[9px] px-1.5 py-0 shrink-0"
                    >
                      {session.mode === "agent" ? $t("chat.mode_agent") : $t("chat.mode_libre")}
                    </Badge>
                  </div>
                  <p class="text-xs text-muted-foreground truncate">{truncate(session.last_message_preview, 80)}</p>
                </div>
                <div class="shrink-0 text-right">
                  <p class="text-[10px] text-muted-foreground/60">{formatRelativeTime(session.created_at)}</p>
                  <p class="text-[10px] text-muted-foreground/40">{$t("chat.message_count", { values: { n: session.message_count ?? 0 } })}</p>
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Closed sessions -->
      {#if $closedChatSessions.length > 0}
        <div>
          <h2 class="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground" data-testid="chat-closed-section">
            {$t("chat.closed_sessions")}
          </h2>
          <div class="space-y-1" data-testid="chat-closed-list">
            {#each $closedChatSessions as session (session.id)}
              <button
                class="group w-full flex items-center gap-3 rounded-xl px-3 py-2.5 text-left opacity-55
                  transition-all hover:glass-surface hover:opacity-75 active:scale-[0.995]"
                onclick={() => navigateToSession(session.id)}
                data-testid="chat-session-{session.id}"
              >
                <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg glass-inset">
                  {#if session.mode === "agent"}
                    <Bot class="h-3.5 w-3.5 text-primary" />
                  {:else}
                    <MessageSquare class="h-3.5 w-3.5 text-secondary" />
                  {/if}
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium truncate">
                      {session.mode === "agent" && session.agent_name ? session.agent_name : $t("chat.mode_libre")}
                    </span>
                    <Badge variant="outline" class="text-[9px] px-1.5 py-0 shrink-0">
                      {$t("chat.status_closed")}
                    </Badge>
                  </div>
                  <p class="text-xs text-muted-foreground truncate">{truncate(session.last_message_preview, 80)}</p>
                </div>
                <div class="shrink-0 text-right">
                  <p class="text-[10px] text-muted-foreground/60">{formatRelativeTime(session.created_at)}</p>
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
{/if}
