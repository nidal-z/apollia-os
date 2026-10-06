<script lang="ts">
  /**
   * The centre of the chat: up to four conversations side by side, each with
   * its header, its answers and its composer.
   *
   * The active pane carries a mark while several are open; a click or a focus
   * inside a pane makes it the active one. In a narrow window one pane shows
   * at a time and the others are reached by the tabs above it; a pane that is
   * not shown stays mounted, so its draft and its answer in progress are kept.
   */
  import { t } from "svelte-i18n";
  import { Plus, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { connectionStatus, chatSessions } from "$lib/stores/sse";
  import { activeChatSessions, closedChatSessions } from "$lib/stores/chat";
  import { markSessionRead } from "$lib/stores/chatSessions";
  import { openSessionsDrawer, toggleContextDrawer } from "$lib/stores/chatLayout";
  import {
    canSplit,
    isDivided,
    paneGrid,
    paneShown,
    tabsShown,
    type ChatPane,
    type PaneSeat,
  } from "$lib/chat/chatPanes";
  import { chatPanes } from "$lib/stores/chatPanes";
  import type { ChatSessionSummary } from "$lib/types";
  import ChatConversation from "./ChatConversation.svelte";
  import EmptySessionsState from "./EmptySessionsState.svelte";
  import QuickPicker from "./QuickPicker.svelte";

  interface Props {
    /** Whether the window is too narrow for panes side by side. */
    narrow: boolean;
    /** Whether the active pane shows the picker of a new conversation. */
    showPicker: boolean;
    oncreated: (session: ChatSessionSummary) => void;
    onpickerclose: () => void;
    onnewchat: () => void;
    /** Opens the right panel on the settings of the active conversation. */
    onconfig: () => void;
    ondelete: (sessionId: string) => void;
  }

  let { narrow, showPicker, oncreated, onpickerclose, onnewchat, onconfig, ondelete }: Props =
    $props();

  const layout = $derived($chatPanes);
  const divided = $derived(isDivided(layout));
  const grid = $derived(paneGrid(layout.panes.length, narrow));
  const noConversation = $derived(
    $activeChatSessions.length === 0 && $closedChatSessions.length === 0,
  );

  function seatOf(pane: ChatPane): PaneSeat {
    return {
      id: pane.id,
      active: pane.id === layout.activeId,
      divided,
      split: () => void chatPanes.split(),
      canSplit: canSplit(layout),
      activate: () => chatPanes.activate(pane.id),
    };
  }

  function titleOf(pane: ChatPane): string {
    const session = $chatSessions.find((s) => s.id === pane.sessionId);
    return session?.title?.trim() || session?.agent_name || $t("chat.panes.untitled");
  }

  function openIn(pane: ChatPane, sessionId: string): void {
    chatPanes.show(sessionId, pane.id);
    markSessionRead(sessionId);
  }

  function newChatIn(pane: ChatPane): void {
    chatPanes.empty(pane.id);
    chatPanes.activate(pane.id);
    onnewchat();
  }
</script>

<main class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col bg-background">
  {#if tabsShown(layout, narrow)}
    <div
      class="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-3"
      role="tablist"
      aria-label={$t("chat.panes.tabs_aria")}
      data-testid="chat-pane-tabs"
    >
      {#each layout.panes as pane (pane.id)}
        <button
          type="button"
          role="tab"
          aria-selected={pane.id === layout.activeId}
          class="max-w-48 shrink-0 truncate border-b-2 px-3 py-2 text-body-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60 {pane.id ===
          layout.activeId
            ? 'border-primary font-medium text-foreground'
            : 'border-transparent text-muted-foreground hover:text-foreground'}"
          onclick={() => chatPanes.activate(pane.id)}
          data-testid="chat-pane-tab"
        >
          {titleOf(pane)}
        </button>
      {/each}
    </div>
  {/if}

  <div
    class="grid min-h-0 flex-1 {grid.columns === 2 ? 'grid-cols-2' : 'grid-cols-1'} {grid.rows === 2
      ? 'grid-rows-2'
      : 'grid-rows-1'}"
    data-testid="chat-panes"
  >
    {#each layout.panes as pane, index (pane.id)}
      {@const active = pane.id === layout.activeId}
      <div
        class="min-h-0 min-w-0 flex-col overflow-hidden {paneShown(layout, pane.id, narrow)
          ? 'flex'
          : 'hidden'} {grid.wide === index ? 'col-span-2' : ''} {divided
          ? 'border-t-2 ' + (active ? 'border-primary' : 'border-transparent')
          : ''} {divided && !narrow ? 'border-l border-l-border first:border-l-0' : ''}"
        role="group"
        aria-label={$t("chat.panes.aria", { values: { n: index + 1 } })}
        aria-current={divided && active ? "true" : undefined}
        onfocusin={() => chatPanes.activate(pane.id)}
        onclickcapture={() => chatPanes.activate(pane.id)}
        data-testid="chat-pane"
      >
        {#if showPicker && active}
          <!-- The picker fills the pane and scrolls internally; it takes the
               place of the conversation, so the two never stack. -->
          <div
            class="flex min-h-0 flex-1 flex-col overflow-y-auto px-6 py-4"
            data-testid="new-chat-picker"
          >
            <QuickPicker {oncreated} onclose={onpickerclose} />
          </div>
        {:else if pane.sessionId}
          <div class="flex h-full min-h-0 flex-1 flex-col">
          <ChatConversation
            sessionId={pane.sessionId}
            pane={seatOf(pane)}
            onclose={() => chatPanes.close(pane.id)}
            onsessionsopen={openSessionsDrawer}
            oncontextopen={toggleContextDrawer}
            onconfigtoggle={onconfig}
            {ondelete}
            onnewChat={() => newChatIn(pane)}
            onopensession={(id) => openIn(pane, id)}
          />
          </div>
        {:else}
          {#if divided}
            <div class="flex shrink-0 justify-end px-4 pt-2.5">
              <button
                type="button"
                class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => chatPanes.close(pane.id)}
                aria-label={$t("chat.panes.close")}
                data-testid="chat-pane-close"
              >
                <X size={14} />
              </button>
            </div>
          {/if}
          {#if $connectionStatus === "connecting"}
            <div class="flex flex-1 items-center justify-center">
              <div class="flex flex-col items-center gap-3 text-muted-foreground">
                <Skeleton variant="text" class="h-3 w-32" />
                <Skeleton variant="text" class="h-3 w-24" />
              </div>
            </div>
          {:else if noConversation}
            <div class="flex flex-1 items-center justify-center px-6 py-10">
              <EmptySessionsState onnewChat={() => newChatIn(pane)} />
            </div>
          {:else}
            <!-- No conversation in this pane but the list is not empty. -->
            <div
              class="flex flex-1 items-center justify-center px-6 py-10"
              data-testid="chat-pane-empty"
            >
              <div class="max-w-sm text-center">
                {#if !divided}
                  <h3 class="mb-3 text-heading-lg font-semibold tracking-[-0.3px] text-foreground">
                    {$t("chat.subtitle")}
                  </h3>
                {/if}
                <p class="mb-5 text-body-sm text-muted-foreground">
                  {$t(divided ? "chat.panes.empty" : "chat.select_or_start")}
                </p>
                <div class="inline-flex">
                  <Button variant="primary-solid" size="sm" onclick={() => newChatIn(pane)}>
                    {#snippet icon()}<Plus size={12} />{/snippet}
                    {$t("chat.new_chat")}
                  </Button>
                </div>
              </div>
            </div>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
</main>
