<script lang="ts">
  /**
   * Two-level chat header.
   *
   * Top row: agent avatar, editable title, live status badge, A2A worker badge,
   * primary actions + overflow menu, close button.
   * Bottom row: meta chips - message count / context usage / duration / mode.
   *
   * Title renaming is inline (double-click on the title, or the "Rename" action).
   */
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import {
    X,
    Bot,
    MessageSquare,
    Settings2,
    Menu,
    PanelRight,
    MoreHorizontal,
    Columns2,
    Edit3,
    Trash2,
    Clock,
    Gauge,
  } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { formatElapsedClock } from "$lib/utils";
  import { Spinner } from "$lib/components/ui/progress";
  import type {
    ChatSessionDetail,
    ConversationStatsView,
  } from "$lib/types";
  import A2AWorkerBadge from "./A2AWorkerBadge.svelte";
  import ChatTargetSelector from "./ChatTargetSelector.svelte";
  import type { ChatTarget, ConversationTarget, ModelTarget } from "$lib/chat/chatTargets";
  import { MAX_PANES } from "$lib/chat/chatPanes";
  import PlanModeChip from "./PlanModeChip.svelte";
  import { Button } from "$lib/components/ui/button";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { contextDrawerOpen } from "$lib/stores/chatLayout";
  import { chatTrashEnabled, removalConfirmKey } from "$lib/stores/chatTrash";

  interface Props {
    session: ChatSessionDetail | null;
    stats: ConversationStatsView | null;
    sessionMode: "libre" | "agent";
    sessionAgentName: string | null;
    sessionStatus: "active" | "processing" | "closed";
    /** "libre" mode has Free/Auto variants driven by tools + system prompt -
     *  surface a simple Libre/Auto label for operator persona. */
    autoMode?: boolean;
    hideConfig?: boolean;
    collapseActions?: boolean;
    onclose: () => void;
    onconfigtoggle?: () => void;
    onsessionsopen?: () => void;
    /** Toggles the shell journal / plan rail on viewports below lg. */
    oncontextopen?: () => void;
    onrename: (title: string) => void;
    ondelete: () => void;
    /** Current plan-mode state of the session (drives the header toggle chip). */
    planMode?: boolean;
    /** Called after the plan-mode chip successfully toggles the session. */
    onplanmodechange?: (enabled: boolean) => void;
    /** Bumped by the parent (e.g. /rename) to open the inline title editor. */
    renameTrigger?: number;
    /** Who the conversation talks to, for its selector. */
    target?: ConversationTarget | null;
    /** Called with the agent or the model chosen in the selector; the
     *  selector is shown only when it is given. */
    ontargetpick?: (target: ChatTarget) => void;
    /** Whether the chat shortcut opens this selector: the header of the active pane. */
    active?: boolean;
    /** Asked before the loading window opens; see `ChatTargetSelector`. */
    loadgate?: (target: ModelTarget) => Promise<boolean>;
    /** Opens one more pane beside this one; the button shows when it is given. */
    onsplit?: () => void;
    /** Whether one more pane fits. */
    canSplit?: boolean;
  }

  let {
    session,
    stats,
    sessionMode,
    sessionAgentName,
    sessionStatus,
    autoMode = false,
    hideConfig = false,
    collapseActions = true,
    onclose,
    onconfigtoggle,
    onsessionsopen,
    oncontextopen,
    onrename,
    ondelete,
    planMode = false,
    onplanmodechange,
    renameTrigger = 0,
    target = null,
    ontargetpick,
    active = true,
    loadgate,
    onsplit,
    canSplit = true,
  }: Props = $props();

  // Open the inline title editor when the parent bumps `renameTrigger` (/rename).
  $effect(() => {
    if (renameTrigger > 0) void startEdit();
  });

  let deleteConfirmOpen = $state(false);
  let editing = $state(false);
  let draftTitle = $state("");
  let titleInput = $state<HTMLInputElement | undefined>(undefined);
  let elapsedSeconds = $state(0);

  const displayTitle = $derived(
    session?.title?.trim() ||
      (sessionMode === "agent" && sessionAgentName
        ? sessionAgentName
        : $t("chat.mode_libre")),
  );

  const modeLabel = $derived(
    sessionMode === "agent"
      ? sessionAgentName ?? $t("chat.mode_agent")
      : autoMode
        ? $t("chat.mode_auto")
        : $t("chat.mode_libre"),
  );

  const messageCount = $derived(stats?.message_count ?? session?.messages?.length ?? 0);
  const contextPct = $derived(stats?.context_usage_pct ?? 0);

  // Live duration - recomputed once a second while the session is open.
  $effect(() => {
    if (!session?.created_at) {
      elapsedSeconds = 0;
      return;
    }
    const start = new Date(session.created_at).getTime();
    const end = session.closed_at ? new Date(session.closed_at).getTime() : null;
    const update = () => {
      elapsedSeconds = Math.max(
        0,
        Math.floor(((end ?? Date.now()) - start) / 1000),
      );
    };
    update();
    if (end) return;
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  });

  async function startEdit(): Promise<void> {
    if (sessionStatus === "closed") return;
    draftTitle = session?.title ?? "";
    editing = true;
    await tick();
    titleInput?.focus();
    titleInput?.select();
  }

  function commitEdit(): void {
    if (!editing) return;
    const trimmed = draftTitle.trim();
    editing = false;
    if (trimmed && trimmed !== (session?.title ?? "")) {
      onrename(trimmed);
    }
  }

  function cancelEdit(): void {
    editing = false;
    draftTitle = "";
  }

  function handleTitleKeydown(ev: KeyboardEvent): void {
    if (ev.key === "Enter") {
      ev.preventDefault();
      commitEdit();
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      cancelEdit();
    }
  }

  let closeMenu = $state<(() => void) | null>(null);

  function handleMenuAction(action: "rename" | "delete" | "drawer"): void {
    closeMenu?.();
    switch (action) {
      case "rename":
        void startEdit();
        return;
      case "delete":
        deleteConfirmOpen = true;
        return;
      case "drawer":
        onconfigtoggle?.();
        return;
    }
  }

</script>

<div
  class="border-b border-border/30 px-4 pt-2.5 pb-2"
  data-testid="chat-conversation-header"
>
  <!-- Top row: avatar · title · status · A2A · actions · close -->
  <div class="flex items-center gap-2 min-w-0">
    {#if onsessionsopen}
      <button
        type="button"
        onclick={onsessionsopen}
        class="lg:hidden h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors shrink-0"
        aria-label={$t("a11y.open_sessions")}
        data-testid="chat-open-sessions-button"
      >
        <Menu size={14} />
      </button>
    {/if}

    <!-- Agent avatar -->
    <div
      class="h-7 w-7 inline-flex items-center justify-center rounded-full shrink-0 {sessionMode === 'agent'
        ? 'bg-primary/15 text-primary'
        : 'bg-muted/50 text-muted-foreground'}"
      aria-hidden="true"
    >
      {#if sessionMode === "agent"}
        <Bot size={14} />
      {:else}
        <MessageSquare size={14} />
      {/if}
    </div>

    <!-- Editable title -->
    <div class="min-w-0 flex-1">
      {#if editing}
        <input
          bind:this={titleInput}
          bind:value={draftTitle}
          onkeydown={handleTitleKeydown}
          onblur={commitEdit}
          type="text"
          maxlength="120"
          placeholder={$t("chat.rename_placeholder")}
          class="w-full rounded-sm border border-primary/40 bg-background px-1.5 py-0.5 text-body-sm font-medium outline-none focus:border-primary"
          data-testid="chat-header-title-input"
        />
      {:else}
        <button
          type="button"
          ondblclick={startEdit}
          class="group flex min-w-0 items-center gap-1 rounded-sm px-1 py-0.5 text-left transition-colors hover:bg-muted/40"
          aria-label={$t("chat.rename_session")}
          data-testid="chat-header-title"
        >
          <span class="truncate text-body-sm font-medium">{displayTitle}</span>
          <Edit3
            size={10}
            class="shrink-0 text-muted-foreground/0 group-hover:text-muted-foreground/60 transition-colors"
          />
        </button>
      {/if}
    </div>

    <!-- Agent or model of the conversation -->
    {#if ontargetpick}
      <div class="min-w-0 shrink">
        <ChatTargetSelector
          conversation={target}
          onpick={ontargetpick}
          disabled={sessionStatus !== "active"}
          ejectable
          listens={active}
          {loadgate}
        />
      </div>
    {/if}

    <!-- Status badge -->
    {#if sessionStatus === "closed"}
      <Badge variant="secondary" class="text-micro-xs px-1.5 py-0 shrink-0">
        {$t("chat.status_closed")}
      </Badge>
    {:else if sessionStatus === "processing"}
      <span
        class="flex items-center gap-1 text-caption text-primary/70 shrink-0"
        data-testid="chat-header-processing"
      >
        <Spinner size={11} />
        <span class="hidden sm:inline">{$t("chat.thinking")}</span>
      </span>
    {:else}
      <Badge variant="outline" class="text-micro-xs px-1.5 py-0 shrink-0 border-success/30 text-success">
        {$t("chat.status_active")}
      </Badge>
    {/if}

    <!-- A2A workers popover -->
    <div class="hidden sm:inline-flex shrink-0">
      <A2AWorkerBadge />
    </div>

    <!-- Action cluster -->
    <div class="flex items-center gap-1 shrink-0" data-chat-header-menu-root>
      {#if session}
        <PlanModeChip
          sessionId={session.id}
          {planMode}
          disabled={sessionStatus === "closed"}
          onchange={onplanmodechange}
        />
      {/if}
      {#if oncontextopen}
        <button
          type="button"
          onclick={oncontextopen}
          class="h-7 w-7 inline-flex items-center justify-center rounded-md transition-colors {$contextDrawerOpen
            ? 'bg-primary/12 text-primary'
            : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted/40'}"
          aria-label={$t("plan_session.tab_journal")}
          aria-pressed={$contextDrawerOpen}
          data-testid="chat-open-context-button"
        >
          <PanelRight size={14} />
        </button>
      {/if}
      {#if onsplit}
        <button
          type="button"
          onclick={onsplit}
          disabled={!canSplit}
          class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:text-foreground hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
          aria-label={$t("chat.panes.split")}
          title={canSplit
            ? $t("chat.panes.split")
            : $t("chat.panes.split_full", { values: { max: MAX_PANES } })}
          data-testid="chat-split-button"
        >
          <Columns2 size={14} />
        </button>
      {/if}
      {#if !hideConfig}
        <Button variant="ghost" size="sm"
          type="button"
          onclick={() => onconfigtoggle?.()}
          class="{collapseActions ? 'hidden md:inline-flex' : 'inline-flex'} h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
          aria-label={$t("chat.config_title")}
          data-testid="chat-config-button"
        >
          <Settings2 size={14} />
        </Button>
      {/if}

      <!-- Overflow / actions menu -->
      <ActionMenu
        align="end"
        class="min-w-[12rem]"
        triggerLabel={$t("chat.session_actions")}
        data-testid="chat-header-overflow-button"
      >
        {#snippet triggerSlot(props)}
          <button
            type="button"
            {...props}
            class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            aria-label={$t("chat.session_actions")}
            data-testid="chat-header-overflow-button"
          >
            <MoreHorizontal size={14} />
          </button>
        {/snippet}
        {#snippet body({ close })}
          {void (closeMenu = close)}
          <ul class="flex flex-col gap-0.5" role="menu" data-testid="chat-header-overflow-menu">
            <li role="none">
              <button
                type="button"
                role="menuitem"
                onclick={() => handleMenuAction("rename")}
                disabled={sessionStatus === "closed"}
                class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground hover:bg-muted disabled:opacity-40 disabled:cursor-not-allowed"
                data-testid="chat-header-menu-rename"
              >
                <Edit3 size={12} /> {$t("chat.rename_session")}
              </button>
            </li>

            {#if !hideConfig && onconfigtoggle}
              <li role="none" class="md:hidden">
                <button
                  type="button"
                  role="menuitem"
                  onclick={() => handleMenuAction("drawer")}
                  class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground hover:bg-muted"
                  data-testid="chat-header-menu-drawer"
                >
                  <Settings2 size={12} /> {$t("chat.toggle_context_drawer")}
                </button>
              </li>
            {/if}

            <li role="separator" aria-hidden="true">
              <div class="my-1 border-t border-border/30"></div>
            </li>

            <li role="none">
              <button
                type="button"
                role="menuitem"
                onclick={() => handleMenuAction("delete")}
                class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-destructive hover:bg-destructive/10"
                data-testid="chat-header-menu-delete"
              >
                <Trash2 size={12} /> {$t("chat.delete_session")}
              </button>
            </li>
          </ul>
        {/snippet}
      </ActionMenu>

      <button
        type="button"
        onclick={onclose}
        class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
        aria-label={$t("a11y.close")}
        data-testid="chat-close-button"
      >
        <X size={14} />
      </button>
    </div>
  </div>

  <!-- Meta row: counts / context / duration / mode -->
  <div
    class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-micro text-muted-foreground/70"
    data-testid="chat-header-meta"
  >
    <span class="inline-flex items-center gap-1" data-testid="chat-header-meta-mode">
      <span class="h-1.5 w-1.5 rounded-full bg-primary/60" aria-hidden="true"></span>
      <span class="truncate">{modeLabel}</span>
    </span>
    <span class="inline-flex items-center gap-1" data-testid="chat-header-meta-messages">
      <MessageSquare size={10} class="shrink-0" />
      <span>{$t("chat.message_count", { values: { n: messageCount } })}</span>
    </span>
    {#if contextPct > 0}
      <span class="inline-flex items-center gap-1" data-testid="chat-header-meta-context">
        <Gauge size={10} class="shrink-0" />
        <span>{Math.round(contextPct)}%</span>
      </span>
    {/if}
    <span class="inline-flex items-center gap-1" data-testid="chat-header-meta-duration">
      <Clock size={10} class="shrink-0" />
      <span>{formatElapsedClock(elapsedSeconds)}</span>
    </span>
    <!-- A2A badge on xs viewports - keeps it visible without header overflow. -->
    <div class="sm:hidden ml-auto">
      <A2AWorkerBadge />
    </div>
  </div>
</div>

<ConfirmDialog
  open={deleteConfirmOpen}
  title={$t("chat.delete_confirm_title")}
  message={$t(removalConfirmKey($chatTrashEnabled))}
  confirmLabel={$t("chat.delete_session")}
  cancelLabel={$t("common.cancel")}
  onclose={() => (deleteConfirmOpen = false)}
  onconfirm={() => { deleteConfirmOpen = false; ondelete(); }}
  data-testid="chat-header-delete-confirm"
/>
