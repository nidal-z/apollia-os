<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Bot,
    MessageSquare,
    Trash2,
    Pencil,
    Check,
    X,
    FolderOpen,
    MoreVertical,
    Pin,
    PinOff,
    Archive,
    ArchiveRestore,
  } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Popover } from "$lib/components/ui/popover";
  import { projects } from "$lib/stores/projects";
  import type { DecoratedSession, SidebarDensity } from "$lib/stores/chatSessions";

  interface Props {
    session: DecoratedSession;
    selected?: boolean;
    density?: SidebarDensity;
    onclick: (id: string) => void;
    ondelete?: (id: string) => void;
    onrename?: (id: string, title: string) => void;
    onpin?: (id: string) => void;
    onarchive?: (id: string) => void;
  }

  let {
    session,
    selected = false,
    density = "md",
    onclick,
    ondelete,
    onrename,
    onpin,
    onarchive,
  }: Props = $props();

  let editing = $state(false);
  let editValue = $state("");
  let confirmingDelete = $state(false);
  let menuOpen = $state(false);

  const isClosed = $derived(session.status === "closed");
  const isProcessing = $derived(session.status === "processing");

  const relativeTime = $derived.by(() => {
    const now = Date.now();
    const anchor = new Date(session.lastActivityAt).getTime();
    const diffMs = now - anchor;
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

  const projectName = $derived.by(() => {
    if (!session.project_id) return null;
    const proj = $projects.find((p) => p.id === session.project_id);
    return proj?.name ?? null;
  });

  const preview = $derived.by(() => {
    if (session.last_message_preview) return session.last_message_preview;
    const count = session.message_count ?? 0;
    if (count > 0) return `${count} ${$t("chat.messages_suffix")}`;
    return $t("chat.no_preview");
  });

  // Density configuration.
  const pad = $derived(density === "sm" ? "px-2.5 py-2" : density === "lg" ? "px-3 py-3" : "px-3 py-2.5");
  const iconBox = $derived(density === "sm" ? "w-5 h-5" : "w-6 h-6");
  const iconPx = $derived(density === "sm" ? 11 : 13);
  const showPreview = $derived(density !== "sm");
  const previewLines = $derived(density === "lg" ? "line-clamp-2" : "line-clamp-1");

  function stopEvent(event: Event) {
    event.stopPropagation();
    event.stopImmediatePropagation();
    event.preventDefault();
  }

  function startEditing(event: MouseEvent) {
    stopEvent(event);
    editValue = session.title || "";
    editing = true;
    menuOpen = false;
  }

  function confirmEdit(event?: Event) {
    if (event) stopEvent(event);
    const trimmed = editValue.trim();
    if (trimmed && onrename) onrename(session.id, trimmed);
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
    menuOpen = false;
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

  function handlePin(event: MouseEvent) {
    stopEvent(event);
    onpin?.(session.id);
    menuOpen = false;
  }

  function handleArchive(event: MouseEvent) {
    stopEvent(event);
    onarchive?.(session.id);
    menuOpen = false;
  }

  function handleCardClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action]")) return;
    onclick(session.id);
  }

  function handleCardKeydown(event: KeyboardEvent) {
    if (editing) return;
    if (event.key === "Enter" || event.key === " ") onclick(session.id);
  }
</script>

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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    role="button"
    tabindex="0"
    class="group relative w-full rounded-lg text-left transition-all duration-150 cursor-pointer
      {pad}
      {selected
        ? 'bg-primary/10 ring-1 ring-primary/30 text-foreground shadow-sm border-l-2 border-primary/60'
        : isClosed
          ? 'opacity-70 grayscale-[40%] hover:opacity-90 hover:bg-muted/30'
          : 'hover:bg-muted/40 hover:shadow-sm'}"
    data-testid="chat-session-card-{session.id}"
    data-density={density}
    data-unread={session.unread || null}
    data-pinned={session.pinned || null}
    onclick={handleCardClick}
    onkeydown={handleCardKeydown}
  >
    <!-- Header row: icon + title + time -->
    <div class="flex items-center gap-2">
      <!-- Mode icon / avatar -->
      <div
        class="shrink-0 relative flex items-center justify-center rounded-md
          {iconBox}
          {session.mode === 'agent'
            ? 'bg-primary/10 text-primary'
            : 'bg-muted/60 text-muted-foreground'}"
      >
        {#if session.mode === "agent"}
          <Bot size={iconPx} />
        {:else}
          <MessageSquare size={iconPx} />
        {/if}
        {#if session.pinned}
          <span
            class="absolute -top-1 -right-1 rounded-full bg-amber-500 p-0.5 text-white shadow"
            title={$t("chat.pinned")}
          >
            <Pin size={7} strokeWidth={3} />
          </span>
        {/if}
      </div>

      <div class="flex-1 min-w-0">
        {#if editing}
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
              aria-label={$t("common.save")}
            >
              <Check size={13} />
            </button>
            <button
              class="rounded p-0.5 text-muted-foreground hover:bg-muted/60 transition-colors"
              onclick={cancelEdit}
              aria-label={$t("common.cancel")}
            >
              <X size={13} />
            </button>
          </div>
        {:else}
          <div class="flex items-center gap-1.5 min-w-0">
            {#if session.unread}
              <span
                class="shrink-0 h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_0_2px_rgba(0,0,0,0.02)]"
                title={$t("chat.unread")}
                data-testid="chat-session-unread-{session.id}"
              ></span>
            {/if}
            <span class="block text-[13px] {session.unread ? 'font-semibold' : 'font-medium'} truncate leading-tight">
              {title}
            </span>
          </div>
        {/if}
      </div>

      {#if !editing}
        <span class="text-[10px] text-muted-foreground/60 shrink-0 tabular-nums">{relativeTime}</span>
        <!-- Kebab menu (always rendered, B.21) -->
        <div data-action class="shrink-0">
          <Popover bind:open={menuOpen} side="bottom" align="end">
            {#snippet trigger()}
              <button
                type="button"
                class="rounded-md p-0.5 text-muted-foreground/50 hover:text-foreground hover:bg-muted/50 transition-colors
                  {menuOpen ? 'bg-muted/50 text-foreground' : ''}"
                aria-label={$t("chat.session_actions")}
                data-testid="chat-session-menu-{session.id}"
                onclick={(e) => stopEvent(e)}
              >
                <MoreVertical size={12} />
              </button>
            {/snippet}
            {#snippet content()}
              <div class="flex flex-col gap-0.5 min-w-[160px]">
                {#if onpin}
                  <button
                    class="flex items-center gap-2 rounded px-2 py-1.5 text-[11px] text-left hover:bg-muted/60 transition-colors"
                    onclick={handlePin}
                    data-testid="chat-session-pin-{session.id}"
                  >
                    {#if session.pinned}
                      <PinOff size={12} />
                      {$t("chat.unpin_session")}
                    {:else}
                      <Pin size={12} />
                      {$t("chat.pin_session")}
                    {/if}
                  </button>
                {/if}
                {#if onrename}
                  <button
                    class="flex items-center gap-2 rounded px-2 py-1.5 text-[11px] text-left hover:bg-muted/60 transition-colors"
                    onclick={startEditing}
                    data-testid="chat-session-rename-{session.id}"
                  >
                    <Pencil size={12} />
                    {$t("chat.rename_session")}
                  </button>
                {/if}
                {#if onarchive}
                  <button
                    class="flex items-center gap-2 rounded px-2 py-1.5 text-[11px] text-left hover:bg-muted/60 transition-colors"
                    onclick={handleArchive}
                    data-testid="chat-session-archive-{session.id}"
                  >
                    {#if session.archived}
                      <ArchiveRestore size={12} />
                      {$t("chat.unarchive_session")}
                    {:else}
                      <Archive size={12} />
                      {$t("chat.archive_session")}
                    {/if}
                  </button>
                {/if}
                {#if ondelete}
                  <div class="h-px bg-border/40 my-0.5"></div>
                  <button
                    class="flex items-center gap-2 rounded px-2 py-1.5 text-[11px] text-left text-destructive hover:bg-destructive/10 transition-colors"
                    onclick={handleDeleteClick}
                    data-testid="chat-session-delete-{session.id}"
                  >
                    <Trash2 size={12} />
                    {$t("chat.delete_session")}
                  </button>
                {/if}
              </div>
            {/snippet}
          </Popover>
        </div>
      {/if}
    </div>

    {#if !editing && showPreview}
      <!-- Info row: badges -->
      <div class="mt-1.5 flex items-center gap-1.5 pl-8 flex-wrap">
        {#if isProcessing}
          <span class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] font-medium bg-amber-500/10 text-amber-600">
            <Spinner size={9} />
            {$t("chat.status_processing")}
          </span>
        {:else if isClosed}
          <span class="inline-flex items-center rounded px-1.5 py-0.5 text-[9px] font-medium bg-muted/40 text-muted-foreground/60">
            {$t("chat.status_closed")}
          </span>
        {:else}
          <span class="inline-flex items-center rounded px-1.5 py-0.5 text-[9px] font-medium bg-emerald-500/10 text-emerald-600">
            {$t("chat.status_active")}
          </span>
        {/if}

        <!-- Agent name / mode -->
        {#if session.mode === "agent" && session.agent_name}
          <span class="inline-flex items-center rounded px-1.5 py-0.5 text-[9px] font-medium bg-primary/8 text-primary/70 truncate max-w-[120px]">
            {session.agent_name}
          </span>
        {/if}

        <!-- Project badge -->
        {#if projectName}
          <span class="inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium bg-primary/5 text-primary/60">
            <FolderOpen size={8} strokeWidth={2} />
            {projectName}
          </span>
        {/if}

        {#if session.archived}
          <span class="inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium bg-muted/40 text-muted-foreground/60">
            <Archive size={8} />
            {$t("chat.filter_archived")}
          </span>
        {/if}
      </div>

      <!-- Preview text -->
      {#if preview}
        <p class="mt-1 text-[11px] text-muted-foreground/60 pl-8 leading-relaxed {previewLines}">
          {preview}
        </p>
      {/if}
    {/if}
  </div>
{/if}

