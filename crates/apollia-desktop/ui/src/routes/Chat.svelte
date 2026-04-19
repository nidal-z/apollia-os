<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions, pendingChatSessionId } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary } from "$lib/types";
  import { tourOpenChatPicker } from "$lib/stores/tour";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import { navigateTo } from "$lib/stores/navigation";
  import ChatConversation from "../components/chat/ChatConversation.svelte";
  import ChatSessionCard from "../components/chat/ChatSessionCard.svelte";
  import ChatSessionsSidebar from "../components/chat/ChatSessionsSidebar.svelte";
  import ChatShell from "../components/chat/ChatShell.svelte";
  import ContextDrawer from "../components/chat/ContextDrawer.svelte";
  import QuickPicker from "../components/chat/QuickPicker.svelte";
  import {
    decoratedSessions,
    markSessionRead,
  } from "$lib/stores/chatSessions";
  import {
    contextDrawerOpen,
    toggleContextDrawer,
    openSessionsDrawer,
    closeSessionsDrawer,
  } from "$lib/stores/chatLayout";
  import { currentSession } from "$lib/stores/chat";

  let selectedSessionId = $state<string | null>(null);
  let showNewChatPicker = $state(false);

  onMount(() => {
    const unsub = pendingChatSessionId.subscribe((id) => {
      if (id) { selectedSessionId = id; pendingChatSessionId.set(null); }
    });
    const unsubTour = tourOpenChatPicker.subscribe((open) => {
      if (open) { openNewChatPicker(); tourOpenChatPicker.set(false); }
    });
    const handleGlobalNewChat = (ev: KeyboardEvent) => {
      const mod = navigator.platform.toLowerCase().includes("mac") ? ev.metaKey : ev.ctrlKey;
      if (mod && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === "n") {
        ev.preventDefault();
        openNewChatPicker();
      }
    };
    window.addEventListener("keydown", handleGlobalNewChat);
    return () => {
      unsub();
      unsubTour();
      window.removeEventListener("keydown", handleGlobalNewChat);
    };
  });

  function openNewChatPicker() { showNewChatPicker = true; }
  function closeNewChatPicker() { showNewChatPicker = false; }

  function handleSessionCreated(session: ChatSessionSummary): void {
    selectedSessionId = session.id;
    showNewChatPicker = false;
  }

  function navigateToSession(sessionId: string) {
    selectedSessionId = sessionId;
    markSessionRead(sessionId);
  }
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

<div class="mx-auto w-full max-w-6xl" data-testid="chat-page">
  <!-- Header -->
  <div class="relative flex items-end justify-between overflow-hidden rounded-2xl bg-gradient-surface px-5 py-5 shadow-elev-1">
    <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-60" aria-hidden="true"></div>
    <div class="relative">
      <h1 class="text-display-lg text-foreground" data-testid="chat-header">{$t("chat.title")}</h1>
      <p class="mt-2 text-sm text-muted-foreground md:text-base" data-testid="chat-subtitle">{$t("chat.subtitle")}</p>
    </div>
    <Button size="sm" onclick={openNewChatPicker} data-testid="new-chat-button" class="relative gap-1.5">
      <Plus size={13} />
      {$t("chat.new_chat")}
    </Button>
  </div>

  <!-- New chat picker -->
  {#if showNewChatPicker}
    <div class="mt-4" data-testid="new-chat-picker">
      <QuickPicker
        oncreated={handleSessionCreated}
        onclose={closeNewChatPicker}
      />
    </div>
  {/if}


  <!-- Main content -->
  <div class="mt-4">
    {#if $connectionStatus === "connecting"}
      <div class="space-y-1">
        {#each { length: 4 } as _}
          <div class="flex items-center gap-3 rounded-lg px-3 py-2.5">
            <Skeleton variant="avatar" class="h-3.5 w-3.5 rounded" />
            <Skeleton variant="text" class="h-3.5 w-28" />
            <Skeleton variant="text" class="h-3 flex-1" />
            <Skeleton variant="text" class="h-3 w-12" />
          </div>
        {/each}
      </div>
    {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0 && !showNewChatPicker && !selectedSessionId}
      <EmptyState
        icon={EMPTY_STATES.chat.icon}
        title={$t(EMPTY_STATES.chat.titleKey)}
        description={$t(EMPTY_STATES.chat.descriptionKey)}
        primaryLabel={$t(EMPTY_STATES.chat.primaryCtaKey ?? '')}
        primaryAction={openNewChatPicker}
        secondaryLabel={$t(EMPTY_STATES.chat.secondaryCtaKey ?? '')}
        secondaryAction={() => navigateTo("agents")}
        page="chat"
      />
    {:else if selectedSessionId}
      <!-- 3-column shell (US-SP42-022): Sessions / Conversation / ContextDrawer. -->
      <div class="overflow-hidden rounded-lg glass-border border" style="height: calc(100vh - 180px);">
        <ChatShell>
          {#snippet sessions()}
            <ChatSessionsSidebar
              {selectedSessionId}
              onNewChat={openNewChatPicker}
              onSelect={(id) => { navigateToSession(id); closeSessionsDrawer(); }}
              onDelete={handleDeleteSession}
              onRename={handleRenameSession}
            />
          {/snippet}

          {#snippet conversation()}
            <div class="flex h-full min-h-0 flex-col glass-card">
              <ChatConversation
                sessionId={selectedSessionId!}
                onclose={closeConversation}
                onconfigtoggle={toggleContextDrawer}
                onsessionsopen={openSessionsDrawer}
              />
            </div>
          {/snippet}

          {#snippet context()}
            <ContextDrawer
              session={$currentSession}
              onupdated={() => { /* ChatConversation subscribes to runtime events */ }}
              onclose={() => contextDrawerOpen.set(false)}
            />
          {/snippet}
        </ChatShell>
      </div>
    {:else}
      <!-- Session list (no session selected) — card list style -->
      {@const activeDecorated = $decoratedSessions.filter((s) => !s.archived && s.status !== "closed")}
      {@const closedDecorated = $decoratedSessions.filter((s) => !s.archived && s.status === "closed")}
      {#if activeDecorated.length > 0}
        <p class="px-1 pb-2 text-[10px] font-semibold uppercase tracking-wider text-primary/50" data-testid="chat-active-section">
          {$t("chat.active_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20 shadow-sm" data-testid="chat-active-list">
          {#each activeDecorated as session (session.id)}
            <ChatSessionCard
              {session}
              onclick={navigateToSession}
              ondelete={handleDeleteSession}
              onrename={handleRenameSession}
            />
          {/each}
        </div>
      {/if}

      {#if closedDecorated.length > 0}
        <p class="px-1 pt-4 pb-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/40" data-testid="chat-closed-section">
          {$t("chat.closed_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20" data-testid="chat-closed-list">
          {#each closedDecorated as session (session.id)}
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
