<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot, MessageSquare, Trash2, Pencil, Check, X, Loader2 } from "lucide-svelte";
  import type { ChatSessionSummary } from "$lib/types";

  interface Props {
    session: ChatSessionSummary;
    selected?: boolean;
    onclick: (id: string) => void;
    ondelete?: (id: string) => void;
    onrename?: (id: string, title: string) => void;
  }

  let { session, selected = false, onclick, ondelete, onrename }: Props = $props();

  let editing = $state(false);
  let editValue = $state("");
  let confirmingDelete = $state(false);

  const isClosed = $derived(session.status === "closed");
  const isProcessing = $derived(session.status === "processing");

  const relativeTime = $derived.by(() => {
    const now = Date.now();
    const created = new Date(session.created_at).getTime();
    const diffMs = now - created;
    const diffMinutes = Math.floor(diffMs / 60_000);
    if (diffMinutes < 1) return $t("chat.just_now");
    if (diffMinutes < 60) return $t("chat.minutes_ago", { values: { n: diffMinutes } });
    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) return $t("chat.hours_ago", { values: { n: diffHours } });
    const diffDays = Math.floor(diffHours / 24);
    return $t("chat.days_ago", { values: { n: diffDays } });
  });

  const title = $derived.by(() => {
    if (session.title) return session.title;
    if (session.mode === "agent" && session.agent_name) return session.agent_name;
    return $t("chat.mode_libre");
  });

  const modeLabel = $derived(
    session.mode === "agent" ? $t("chat.mode_agent") : $t("chat.mode_libre")
  );

  const preview = $derived.by(() => {
    if (session.last_message_preview) {
      return session.last_message_preview.length > 50
        ? session.last_message_preview.slice(0, 50) + "\u2026"
        : session.last_message_preview;
    }
    const count = session.message_count ?? 0;
    if (count > 0) return `${count} ${$t("chat.messages_suffix")}`;
    return $t("chat.no_preview");
  });

  function stopEvent(event: Event) {
    event.stopPropagation();
    event.stopImmediatePropagation();
    event.preventDefault();
  }

  function startEditing(event: MouseEvent) {
    stopEvent(event);
    editValue = session.title || "";
    editing = true;
  }

  function confirmEdit(event?: Event) {
    if (event) stopEvent(event);
    const trimmed = editValue.trim();
    if (trimmed && onrename) {
      onrename(session.id, trimmed);
    }
    editing = false;
  }

  function cancelEdit(event?: Event) {
    if (event) stopEvent(event);
    editing = false;
  }

  function handleEditKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      confirmEdit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelEdit();
    }
  }

  function handleDeleteClick(event: MouseEvent) {
    stopEvent(event);
    confirmingDelete = true;
  }

  function confirmDelete(event: MouseEvent) {
    stopEvent(event);
    ondelete?.(session.id);
    confirmingDelete = false;
  }

  function cancelDelete(event: MouseEvent) {
    stopEvent(event);
    confirmingDelete = false;
  }

  function handleCardClick(event: MouseEvent) {
    // Don't navigate if the click was on an action button or edit area
    const target = event.target as HTMLElement;
    if (target.closest("[data-action]")) return;
    onclick(session.id);
  }

  function handleCardKeydown(event: KeyboardEvent) {
    // Don't navigate if we're editing
    if (editing) return;
    if (event.key === "Enter" || event.key === " ") onclick(session.id);
  }
</script>

<!-- Delete confirmation overlay -->
{#if confirmingDelete}
  <div
    class="relative rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2.5 animate-fade-in"
    data-testid="chat-session-delete-confirm-{session.id}"
  >
    <p class="text-[11px] font-medium text-destructive mb-2">{$t("chat.delete_confirm_title")}</p>
    <p class="text-[10px] text-muted-foreground mb-2.5">{$t("chat.delete_confirm_message")}</p>
    <div class="flex items-center gap-2">
      <button
        class="flex-1 rounded-md bg-destructive px-2 py-1 text-[11px] font-medium text-destructive-foreground
          hover:bg-destructive/90 transition-colors"
        onclick={confirmDelete}
        data-testid="chat-session-delete-yes-{session.id}"
      >
        {$t("common.delete")}
      </button>
      <button
        class="flex-1 rounded-md bg-muted/60 px-2 py-1 text-[11px] font-medium text-muted-foreground
          hover:bg-muted transition-colors"
        onclick={cancelDelete}
        data-testid="chat-session-delete-no-{session.id}"
      >
        {$t("common.cancel")}
      </button>
    </div>
  </div>
{:else}
  <!-- Normal card -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    role="button"
    tabindex="0"
    class="group relative w-full rounded-lg px-3 py-2.5 text-left transition-all duration-150 cursor-pointer
      {selected
        ? 'bg-primary/10 ring-1 ring-primary/20 text-foreground shadow-sm'
        : isClosed
          ? 'opacity-45 hover:opacity-70 hover:bg-muted/30'
          : 'hover:bg-muted/40 hover:shadow-sm'}"
    data-testid="chat-session-card-{session.id}"
    onclick={handleCardClick}
    onkeydown={handleCardKeydown}
  >
    <!-- Action buttons (visible on hover) -->
    {#if !editing}
      <div data-action class="absolute right-2 top-2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-all">
        {#if onrename}
          <button
            class="rounded-md p-1 text-muted-foreground/60 hover:bg-muted/60 hover:text-foreground transition-all"
            onclick={startEditing}
            title={$t("chat.rename_session")}
            data-testid="chat-session-rename-{session.id}"
          >
            <Pencil size={11} />
          </button>
        {/if}
        {#if ondelete}
          <button
            class="rounded-md p-1 text-muted-foreground/60 hover:bg-destructive/10 hover:text-destructive transition-all"
            onclick={handleDeleteClick}
            title={$t("chat.delete_session")}
            data-testid="chat-session-delete-{session.id}"
          >
            <Trash2 size={11} />
          </button>
        {/if}
      </div>
    {/if}

    <!-- Header row: icon + title + time -->
    <div class="flex items-center gap-2">
      <!-- Mode icon with colored background -->
      <div class="shrink-0 flex items-center justify-center w-6 h-6 rounded-md
        {session.mode === 'agent'
          ? 'bg-primary/10 text-primary'
          : 'bg-muted/60 text-muted-foreground'}">
        {#if session.mode === "agent"}
          <Bot size={13} />
        {:else}
          <MessageSquare size={13} />
        {/if}
      </div>

      <div class="flex-1 min-w-0">
        {#if editing}
          <!-- Inline rename input -->
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div data-action class="flex items-center gap-1" onclick={(e) => stopEvent(e)}>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              type="text"
              bind:value={editValue}
              onkeydown={handleEditKeydown}
              placeholder={$t("chat.rename_placeholder")}
              class="flex-1 min-w-0 h-6 rounded-md border border-border/50 bg-background px-1.5 text-[12px]
                focus:outline-none focus:ring-1 focus:ring-primary/40"
              data-testid="chat-session-rename-input-{session.id}"
              autofocus
            />
            <button
              class="rounded p-0.5 text-primary hover:bg-primary/10 transition-colors"
              onclick={confirmEdit}
            >
              <Check size={13} />
            </button>
            <button
              class="rounded p-0.5 text-muted-foreground hover:bg-muted/60 transition-colors"
              onclick={cancelEdit}
            >
              <X size={13} />
            </button>
          </div>
        {:else}
          <span class="block text-[13px] font-medium truncate leading-tight">{title}</span>
        {/if}
      </div>

      {#if !editing}
        <span class="text-[10px] text-muted-foreground/50 shrink-0 tabular-nums">{relativeTime}</span>
      {/if}
    </div>

    <!-- Info row: mode badge + preview + status -->
    {#if !editing}
      <div class="mt-1.5 flex items-center gap-1.5 pl-8">
        <!-- Mode badge -->
        <span class="inline-flex items-center rounded px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide
          {session.mode === 'agent'
            ? 'bg-primary/8 text-primary/70'
            : 'bg-muted/50 text-muted-foreground/60'}">
          {modeLabel}
        </span>

        <!-- Status indicator -->
        {#if isProcessing}
          <span class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] font-medium bg-amber-500/10 text-amber-600">
            <Loader2 size={9} class="animate-spin" />
            {$t("chat.status_processing")}
          </span>
        {:else if isClosed}
          <span class="inline-flex items-center rounded px-1.5 py-0.5 text-[9px] font-medium bg-muted/40 text-muted-foreground/50">
            {$t("chat.status_closed")}
          </span>
        {/if}

        <!-- Message count -->
        {#if session.message_count > 0}
          <span class="text-[9px] text-muted-foreground/40 tabular-nums">
            {session.message_count} msg
          </span>
        {/if}
      </div>

      <!-- Preview text -->
      {#if preview}
        <p class="mt-1 text-[11px] text-muted-foreground/50 truncate pl-8 leading-relaxed italic">{preview}</p>
      {/if}
    {/if}
  </div>
{/if}
