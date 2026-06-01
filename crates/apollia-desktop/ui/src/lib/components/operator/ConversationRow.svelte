<script lang="ts">
  import { MessageCircle, Pin, Archive, MoreHorizontal, Edit3, Trash2, Check, X, FolderOpen } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import { Button } from "$lib/components/ui/button";

  export type ConversationState =
    | "active"
    | "unread"
    | "pinned"
    | "closed"
    | "archived"
    | "default";

  interface Props {
    title: string;
    /** Last message preview / agent attribution. */
    lastMessage?: string;
    timestamp: string;
    state?: ConversationState;
    unreadCount?: number;
    /** True when an agent is currently typing/working in this conversation. */
    live?: boolean;
    onclick?: (e: MouseEvent) => void;
    /**
     * Optional rename action - submits the new title from the inline editor.
     * Providing this callback enables both the kebab menu "Renommer" entry
     * and double-click inline editing on the title.
     */
    onrename?: (newTitle: string) => void;
    /** Optional delete action - shows a kebab menu when provided. */
    ondelete?: () => void;
    /** Project this conversation is linked to (displayed as a small chip). */
    projectLabel?: string;
  }

  let {
    title,
    lastMessage,
    timestamp,
    state: rowStateProp = "default",
    unreadCount,
    live = false,
    onclick,
    onrename,
    ondelete,
    projectLabel,
  }: Props = $props();

  const isActive = $derived(rowStateProp === "active");
  const isClosed = $derived(rowStateProp === "closed");
  const isArchived = $derived(rowStateProp === "archived");
  const isPinned = $derived(rowStateProp === "pinned");
  const isUnread = $derived(rowStateProp === "unread" || (unreadCount !== undefined && unreadCount > 0));

  const hasMenu = $derived(Boolean(onrename || ondelete));
  let menuOpen = $state(false);
  let menuRoot = $state<HTMLDivElement | undefined>(undefined);

  // Inline edit state - entered via double-click on the title or the
  // kebab "Renommer" entry. Enter/blur commits, Escape cancels.
  let editing = $state(false);
  let editValue = $state("");
  let editInput = $state<HTMLInputElement | undefined>(undefined);

  // Inline delete-confirmation state - gating misclicks. The destructive
  // action only runs when the user explicitly clicks "Confirmer". The
  // "Confirmer" button lives in a stable strip that is NOT torn down by
  // the click that opens it (unlike the kebab menu), so the callback can
  // run synchronously without Svelte's reactive re-render swallowing it.
  let confirmingDelete = $state(false);

  function startEdit(): void {
    if (!onrename) return;
    editValue = title;
    editing = true;
    queueMicrotask(() => {
      editInput?.focus();
      editInput?.select();
    });
  }

  function commitEdit(): void {
    if (!editing) return;
    const trimmed = editValue.trim();
    editing = false;
    if (!trimmed || trimmed === title) return;
    onrename?.(trimmed);
  }

  function cancelEdit(): void {
    editing = false;
    editValue = title;
  }

  function onTitleDblClick(ev: MouseEvent): void {
    if (!onrename) return;
    ev.stopPropagation();
    startEdit();
  }

  function onEditKeydown(ev: KeyboardEvent): void {
    if (ev.key === "Enter") {
      ev.preventDefault();
      ev.stopPropagation();
      commitEdit();
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      ev.stopPropagation();
      cancelEdit();
    } else {
      ev.stopPropagation();
    }
  }

  // NOTE: do NOT call `ev.stopPropagation()` / `ev.preventDefault()` in any
  // of the menu-related handlers below. Svelte 5's listener attachment for
  // freshly-mounted elements interacts badly with stopped clicks here - it
  // produced an intermittent "1 click in 2" failure on the menu items.
  // The row-level `onRowClick` already filters out clicks whose target lives
  // inside `menuRoot`, so propagation is harmless.
  function toggleMenu(): void {
    menuOpen = !menuOpen;
    if (!menuOpen) confirmingDelete = false;
  }

  function handleRename(): void {
    menuOpen = false;
    confirmingDelete = false;
    startEdit();
  }

  /** Menu entry: swaps the popover content to the confirmation buttons. */
  function handleDeleteRequest(): void {
    confirmingDelete = true;
  }

  /** Confirmation popover "Confirmer" - actually fires the delete callback. */
  function handleDeleteConfirm(): void {
    ondelete?.();
  }

  function handleDeleteCancel(): void {
    confirmingDelete = false;
    menuOpen = false;
  }

  /** Guard: swallow row-level clicks whose target lives inside the menu
      or the confirmation strip. Prevents misroute to ChatConversation. */
  function onRowClick(ev: MouseEvent): void {
    const target = ev.target as Node | null;
    if (menuRoot && target && menuRoot.contains(target)) return;
    if (editing || confirmingDelete) return;
    onclick?.(ev);
  }
  function onRowKeydown(ev: KeyboardEvent): void {
    if (ev.key !== "Enter") return;
    const target = ev.target as Node | null;
    if (menuRoot && target && menuRoot.contains(target)) return;
    if (editing || confirmingDelete) return;
    onclick?.(ev as unknown as MouseEvent);
  }

  function handleDocumentClick(ev: MouseEvent): void {
    if (!menuOpen) return;
    if (!menuRoot) return;
    // Use composedPath() instead of contains(target) - when a click on a
    // menu item triggers a Svelte re-render that unmounts the clicked
    // element (e.g. clicking "Supprimer" flips `confirmingDelete=true`,
    // which unmounts the menu popover containing the Supprimer button),
    // the target node is detached from the DOM by the time this bubble
    // handler runs. `menuRoot.contains(detachedNode)` then returns false
    // and the menu would be wrongly closed. `composedPath()` captures
    // the path at dispatch time, so menuRoot is still in it.
    const path = ev.composedPath();
    if (!path.includes(menuRoot)) {
      menuOpen = false;
      confirmingDelete = false;
    }
  }

  $effect(() => {
    if (!menuOpen) return;
    document.addEventListener("click", handleDocumentClick);
    return () => document.removeEventListener("click", handleDocumentClick);
  });
</script>

<div
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onclick={onclick ? onRowClick : undefined}
  onkeydown={onclick ? onRowKeydown : undefined}
  class="group relative flex gap-2.5 px-3.5 py-3 cursor-pointer border-b border-border/60 transition-colors {isActive
    ? 'bg-primary/10'
    : 'bg-transparent hover:bg-muted/40'}"
  style:opacity={isClosed || isArchived ? "0.55" : "1"}
>
  <div
    class="w-[22px] h-[22px] rounded-md shrink-0 mt-0.5 inline-flex items-center justify-center {isActive
      ? 'bg-primary text-white'
      : 'bg-muted text-muted-foreground'}"
  >
    {#if isPinned}
      <Pin size={11} />
    {:else if isArchived}
      <Archive size={11} />
    {:else}
      <MessageCircle size={11} />
    {/if}
  </div>
  <div class="flex-1 min-w-0">
    {#if editing}
      <input
        bind:this={editInput}
        bind:value={editValue}
        onkeydown={onEditKeydown}
        onblur={commitEdit}
        onclick={(e) => e.stopPropagation()}
        ondblclick={(e) => e.stopPropagation()}
        type="text"
        maxlength="100"
        data-testid="conversation-row-rename-input"
        class="w-full rounded-sm border border-primary/40 bg-background px-1 py-0.5 text-[12.5px] text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/40"
        style:font-weight={isActive || isUnread ? 600 : 500}
      />
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="text-[12.5px] text-foreground truncate"
        style:font-weight={isActive || isUnread ? 600 : 500}
        style:text-decoration={isClosed ? "line-through" : "none"}
        ondblclick={onTitleDblClick}
      >
        {title}
      </div>
    {/if}
    <div class="text-[10.5px] text-muted-foreground mt-0.5 inline-flex items-center gap-1.5 flex-wrap">
      {#if lastMessage}<span class="truncate">{lastMessage}</span>{/if}
      <span>· il y a {timestamp}</span>
      {#if projectLabel}
        <span
          class="inline-flex items-center gap-0.5 rounded-sm px-1 py-px text-[9.5px] text-primary/80 bg-primary/10 max-w-[9rem]"
          title={projectLabel}
          data-testid="conversation-row-project-chip"
        >
          <FolderOpen size={9} class="shrink-0" />
          <span class="truncate">{projectLabel}</span>
        </span>
      {/if}
      {#if live}
        <span>·</span>
        <span class="text-primary inline-flex items-center gap-1">
          <StatusDot color="hsl(var(--primary))" glow /> en cours
        </span>
      {/if}
    </div>
  </div>
  {#if unreadCount !== undefined && unreadCount > 0}
    <span
      class="self-center bg-primary text-white text-[9.5px] font-semibold px-1.5 py-px rounded-full"
    >
      {unreadCount}
    </span>
  {/if}
  {#if hasMenu}
    <div class="relative self-center" bind:this={menuRoot}>
      <button
        type="button"
        onclick={toggleMenu}
        class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-muted/60 hover:text-foreground focus:opacity-100 {isActive || menuOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}"
        aria-label="Actions"
        aria-expanded={menuOpen}
        data-testid="conversation-row-menu-button"
      >
        <MoreHorizontal size={12} />
      </button>
      {#if menuOpen && !confirmingDelete}
        <div
          class="absolute right-0 top-full z-30 mt-1 min-w-[10rem] rounded-md border border-border/50 bg-card py-1 shadow-elev-2"
          role="menu"
          data-testid="conversation-row-menu"
        >
          {#if onrename}
            <Button variant="ghost" size="sm"
              type="button"
              role="menuitem"
              onclick={handleRename}
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/60"
              data-testid="conversation-row-menu-rename"
            >
              <Edit3 size={12} /> Renommer
            </Button>
          {/if}
          {#if ondelete}
            <Button variant="ghost" size="sm"
              type="button"
              role="menuitem"
              onclick={handleDeleteRequest}
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-destructive hover:bg-destructive/10"
              data-testid="conversation-row-menu-delete"
            >
              <Trash2 size={12} /> Supprimer
            </Button>
          {/if}
        </div>
      {/if}
      {#if menuOpen && confirmingDelete}
        <div
          class="absolute right-0 top-full z-30 mt-1 flex min-w-[10rem] items-center gap-1 rounded-md border border-border/50 bg-card px-2 py-1 shadow-elev-2"
          data-testid="conversation-row-delete-confirm"
        >
          <Button variant="ghost" size="sm"
            type="button"
            onclick={handleDeleteConfirm}
            class="inline-flex flex-1 items-center justify-center gap-1 rounded bg-destructive px-2 py-1 text-[11px] font-semibold text-white hover:bg-destructive/90 focus:outline-none focus-visible:ring-2 focus-visible:ring-destructive/40"
            data-testid="conversation-row-delete-confirm-btn"
          >
            <Check size={11} /> Confirmer
          </Button>
          <Button variant="ghost" size="sm"
            type="button"
            onclick={handleDeleteCancel}
            class="inline-flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            data-testid="conversation-row-delete-cancel-btn"
          >
            <X size={11} /> Annuler
          </Button>
        </div>
      {/if}
    </div>
  {/if}
</div>
