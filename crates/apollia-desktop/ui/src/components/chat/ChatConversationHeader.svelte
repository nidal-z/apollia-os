<script lang="ts">
  /**
   * Two-level chat header.
   *
   * Top row: agent avatar, editable title, live status badge, A2A worker badge,
   * primary actions + overflow menu, close button.
   * Bottom row: meta chips — message count / context usage / duration / mode.
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
    MoreHorizontal,
    Edit3,
    Download,
    Trash2,
    FileJson,
    FileText,
    Clock,
    Gauge,
  } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Spinner } from "$lib/components/ui/progress";
  import type { ChatSessionDetail, ConversationStatsView } from "$lib/types";
  import type { ExportFormat } from "$lib/chat/exportConversation";
  import A2AWorkerBadge from "./A2AWorkerBadge.svelte";

  interface Props {
    session: ChatSessionDetail | null;
    stats: ConversationStatsView | null;
    sessionMode: "libre" | "agent";
    sessionAgentName: string | null;
    sessionStatus: "active" | "processing" | "closed";
    /** "libre" mode has Free/Auto variants driven by tools + system prompt —
     *  surface a simple Libre/Auto label for operator persona (B.5). */
    autoMode?: boolean;
    hideConfig?: boolean;
    collapseActions?: boolean;
    onclose: () => void;
    onconfigtoggle?: () => void;
    onsessionsopen?: () => void;
    onrename: (title: string) => void;
    onexport: (format: ExportFormat) => void;
    ondelete: () => void;
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
    onrename,
    onexport,
    ondelete,
  }: Props = $props();

  let menuOpen = $state(false);
  let exportOpen = $state(false);
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

  // Live duration — recomputed once a second while the session is open.
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

  function formatDuration(total: number): string {
    if (total < 60) return `${total}s`;
    const m = Math.floor(total / 60);
    const s = total % 60;
    if (m < 60) return s === 0 ? `${m}m` : `${m}m ${s}s`;
    const h = Math.floor(m / 60);
    const rm = m % 60;
    return rm === 0 ? `${h}h` : `${h}h ${rm}m`;
  }

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

  function handleMenuAction(action: "rename" | "delete" | "drawer"): void {
    menuOpen = false;
    switch (action) {
      case "rename":
        void startEdit();
        return;
      case "delete":
        ondelete();
        return;
      case "drawer":
        onconfigtoggle?.();
        return;
    }
  }

  function handleExport(format: ExportFormat): void {
    exportOpen = false;
    menuOpen = false;
    onexport(format);
  }

  function handleDocumentClick(ev: MouseEvent): void {
    if (!menuOpen && !exportOpen) return;
    const target = ev.target as Node | null;
    if (!target) return;
    const root = document.querySelector<HTMLElement>(
      "[data-chat-header-menu-root]",
    );
    if (root && !root.contains(target)) {
      menuOpen = false;
      exportOpen = false;
    }
  }

  $effect(() => {
    if (menuOpen || exportOpen) {
      document.addEventListener("click", handleDocumentClick);
      return () => document.removeEventListener("click", handleDocumentClick);
    }
  });
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
        class="md:hidden h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors shrink-0"
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
          class="w-full rounded-sm border border-primary/40 bg-background px-1.5 py-0.5 text-[13px] font-medium outline-none focus:border-primary"
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
          <span class="truncate text-[13px] font-medium">{displayTitle}</span>
          <Edit3
            size={10}
            class="shrink-0 text-muted-foreground/0 group-hover:text-muted-foreground/60 transition-colors"
          />
        </button>
      {/if}
    </div>

    <!-- Status badge -->
    {#if sessionStatus === "closed"}
      <Badge variant="secondary" class="text-[9px] px-1.5 py-0 shrink-0">
        {$t("chat.status_closed")}
      </Badge>
    {:else if sessionStatus === "processing"}
      <span
        class="flex items-center gap-1 text-[11px] text-primary/70 shrink-0"
        data-testid="chat-header-processing"
      >
        <Spinner size={11} />
        <span class="hidden sm:inline">{$t("chat.thinking")}</span>
      </span>
    {:else}
      <Badge variant="outline" class="text-[9px] px-1.5 py-0 shrink-0 border-success/30 text-success">
        {$t("chat.status_active")}
      </Badge>
    {/if}

    <!-- A2A workers popover -->
    <div class="hidden sm:inline-flex shrink-0">
      <A2AWorkerBadge />
    </div>

    <!-- Action cluster -->
    <div class="flex items-center gap-0.5 shrink-0" data-chat-header-menu-root>
      {#if !hideConfig}
        <button
          type="button"
          onclick={() => onconfigtoggle?.()}
          class="{collapseActions ? 'hidden md:inline-flex' : 'inline-flex'} h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
          aria-label={$t("chat.config_title")}
          data-testid="chat-config-button"
        >
          <Settings2 size={14} />
        </button>
      {/if}

      <!-- Overflow / actions menu -->
      <div class="relative">
        <button
          type="button"
          onclick={() => (menuOpen = !menuOpen)}
          class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
          aria-label={$t("chat.session_actions")}
          aria-expanded={menuOpen}
          data-testid="chat-header-overflow-button"
        >
          <MoreHorizontal size={14} />
        </button>
        {#if menuOpen}
          <div
            class="absolute right-0 top-full mt-1 z-20 min-w-[12rem] rounded-md border border-border/50 bg-card shadow-elev-2 py-1 animate-fade-in"
            role="menu"
            data-testid="chat-header-overflow-menu"
          >
            <button
              type="button"
              role="menuitem"
              onclick={() => handleMenuAction("rename")}
              disabled={sessionStatus === "closed"}
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60 disabled:opacity-40 disabled:cursor-not-allowed"
              data-testid="chat-header-menu-rename"
            >
              <Edit3 size={12} /> {$t("chat.rename_session")}
            </button>

            <!-- Export submenu -->
            <div class="relative">
              <button
                type="button"
                role="menuitem"
                onclick={() => (exportOpen = !exportOpen)}
                class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60"
                aria-expanded={exportOpen}
                data-testid="chat-header-menu-export"
              >
                <Download size={12} /> {$t("chat.export_session")}
              </button>
              {#if exportOpen}
                <div
                  class="absolute right-full top-0 mr-1 min-w-[11rem] rounded-md border border-border/50 bg-card shadow-elev-2 py-1"
                  role="menu"
                >
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => handleExport("markdown-with-tools")}
                    class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60"
                    data-testid="chat-header-menu-export-md"
                  >
                    <FileText size={12} /> {$t("chat.export_markdown")}
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => handleExport("json")}
                    class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60"
                    data-testid="chat-header-menu-export-json"
                  >
                    <FileJson size={12} /> {$t("chat.export_json")}
                  </button>
                </div>
              {/if}
            </div>

            {#if !hideConfig && onconfigtoggle}
              <button
                type="button"
                role="menuitem"
                onclick={() => handleMenuAction("drawer")}
                class="md:hidden w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60"
                data-testid="chat-header-menu-drawer"
              >
                <Settings2 size={12} /> {$t("chat.toggle_context_drawer")}
              </button>
            {/if}

            <div class="my-1 border-t border-border/30" aria-hidden="true"></div>

            <button
              type="button"
              role="menuitem"
              onclick={() => handleMenuAction("delete")}
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs text-destructive hover:bg-destructive/10"
              data-testid="chat-header-menu-delete"
            >
              <Trash2 size={12} /> {$t("chat.delete_session")}
            </button>
          </div>
        {/if}
      </div>

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
    class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground/70"
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
      <span>{formatDuration(elapsedSeconds)}</span>
    </span>
    <!-- A2A badge on xs viewports — keeps it visible without header overflow. -->
    <div class="sm:hidden ml-auto">
      <A2AWorkerBadge />
    </div>
  </div>
</div>
