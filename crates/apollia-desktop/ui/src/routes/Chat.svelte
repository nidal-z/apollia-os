<script lang="ts">
  import { t } from "svelte-i18n";
  import { MessageSquare } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions } from "$lib/stores/chat";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ChatSessionCard from "../components/chat/ChatSessionCard.svelte";
  import NewChatDialog from "../components/chat/NewChatDialog.svelte";

  const SKELETON_COUNT = 3;

  let dialogOpen = $state(false);

  function openNewChat() {
    dialogOpen = true;
  }

  function closeDialog() {
    dialogOpen = false;
  }

  function navigateToSession(_sessionId: string) {
    // Navigation vers la vue conversation (STORY-206)
    // Pour l'instant, reste sur la route /chat
    void _sessionId;
  }

  function handleSessionCreated(sessionId: string) {
    navigateToSession(sessionId);
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-bold" data-testid="chat-header">{$t("chat.title")}</h1>
      <p class="text-sm text-muted-foreground" data-testid="chat-subtitle">{$t("chat.subtitle")}</p>
    </div>
    <Button onclick={openNewChat} data-testid="new-chat-button">
      {$t("chat.new_chat")}
    </Button>
  </div>

  <!-- Sessions list, skeleton loaders, or empty state -->
  {#if $connectionStatus === "connecting"}
    <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="chat-skeleton">
      {#each { length: SKELETON_COUNT } as _}
        <div class="rounded-lg border bg-background p-4">
          <div class="flex items-start justify-between">
            <div class="flex items-center gap-2">
              <Skeleton class="h-5 w-20 rounded-full" />
              <Skeleton class="h-4 w-24" />
            </div>
            <Skeleton class="h-5 w-14 rounded-full" />
          </div>
          <Skeleton class="mt-2 h-3 w-[80%]" />
          <div class="mt-2 flex justify-between">
            <Skeleton class="h-3 w-16" />
            <Skeleton class="h-3 w-20" />
          </div>
        </div>
      {/each}
    </div>
  {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0}
    <EmptyState
      icon={MessageSquare}
      title={$t("chat.empty_title")}
      subtitle={$t("chat.empty_subtitle")}
      ctaLabel={$t("chat.new_chat")}
      ctaAction={openNewChat}
      page="chat"
    />
  {:else}
    <!-- Active sessions -->
    {#if $activeChatSessions.length > 0}
      <div>
        <h2 class="mb-2 text-sm font-semibold text-muted-foreground" data-testid="chat-active-section">
          {$t("chat.active_sessions")}
        </h2>
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="chat-active-grid">
          {#each $activeChatSessions as session (session.id)}
            <ChatSessionCard {session} onclick={navigateToSession} />
          {/each}
        </div>
      </div>
    {/if}

    <!-- Closed sessions -->
    {#if $closedChatSessions.length > 0}
      <div>
        <h2 class="mb-2 text-sm font-semibold text-muted-foreground" data-testid="chat-closed-section">
          {$t("chat.closed_sessions")}
        </h2>
        <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="chat-closed-grid">
          {#each $closedChatSessions as session (session.id)}
            <ChatSessionCard {session} onclick={navigateToSession} />
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<!-- New chat dialog -->
<NewChatDialog open={dialogOpen} onclose={closeDialog} oncreated={handleSessionCreated} />
