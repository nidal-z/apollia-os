<script lang="ts">
  import { t } from "svelte-i18n";
  import { tick } from "svelte";
  import { MessageCircle, Pin, Archive, MoreHorizontal, Edit3, Trash2, Check, X, FolderOpen, Settings2 } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import { Button } from "$lib/components/ui/button";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import ListRow from "./ListRow.svelte";

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
    /** Optional configure action - adds a "Configuration" kebab entry. */
    onconfigure?: () => void;
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
    onconfigure,
    projectLabel,
  }: Props = $props();

  const isActive = $derived(rowStateProp === "active");
  const isClosed = $derived(rowStateProp === "closed");
  const isArchived = $derived(rowStateProp === "archived");
  const isPinned = $derived(rowStateProp === "pinned");
  const isUnread = $derived(rowStateProp === "unread" || (unreadCount !== undefined && unreadCount > 0));

  const hasMenu = $derived(Boolean(onrename || ondelete || onconfigure));

  // Kebab menu open state - bound to the canonical ActionMenu/Popover. Kept
  // only to reset the delete-confirmation step whenever the menu closes (e.g.
  // the user dismisses by clicking outside while the confirm strip is shown).
  let menuOpen = $state(false);
  $effect(() => {
    if (!menuOpen) confirmingDelete = false;
  });

  // Inline edit state - entered via double-click on the title or the
  // kebab "Renommer" entry. Enter/blur commits, Escape cancels.
  let editing = $state(false);
  let editValue = $state("");
  let editInput = $state<HTMLInputElement | undefined>(undefined);

  // Inline delete-confirmation step, rendered inside the action-menu popover.
  // The destructive action only runs on an explicit "Confirmer" click.
  let confirmingDelete = $state(false);

  async function startEdit(): Promise<void> {
    if (!onrename) return;
    editValue = title;
    editing = true;
    await tick();
    editInput?.focus();
    editInput?.select();
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
    void startEdit();
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

  /** Guard: row-level navigation ignores clicks on the kebab trigger (the
      menu content itself is portaled, so it never bubbles here) and clicks
      while inline editing or confirming a delete. */
  function onRowClick(ev: MouseEvent): void {
    const target = ev.target as HTMLElement | null;
    if (target?.closest("[data-conversation-row-actions]")) return;
    if (editing || confirmingDelete) return;
    onclick?.(ev);
  }
  function onRowKeydown(ev: KeyboardEvent): void {
    if (ev.key !== "Enter") return;
    const target = ev.target as HTMLElement | null;
    if (target?.closest("[data-conversation-row-actions]")) return;
    if (editing || confirmingDelete) return;
    onclick?.(ev as unknown as MouseEvent);
  }
</script>

<ListRow
  pad="snug"
  align="stretch"
  state={isActive ? "active" : "default"}
  dim={isClosed || isArchived}
  onclick={onclick ? onRowClick : undefined}
  onkeydown={onclick ? onRowKeydown : undefined}
  class="cursor-pointer"
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
      <span>· {$t("common.relative.ago", { values: { time: timestamp } })}</span>
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
    <div class="self-center" data-conversation-row-actions>
      <ActionMenu
        bind:open={menuOpen}
        align="end"
        class="min-w-[10rem]"
        triggerLabel="Actions"
        data-testid="conversation-row-menu-button"
      >
        {#snippet triggerSlot(props)}
          <button
            type="button"
            {...props}
            class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-muted/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 {isActive
              ? 'opacity-100'
              : 'opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100'}"
            aria-label={$t("a11y.actions_menu")}
            data-testid="conversation-row-menu-button"
          >
            <MoreHorizontal size={12} />
          </button>
        {/snippet}
        {#snippet body({ close })}
          {#if confirmingDelete}
            <div
              class="flex items-center gap-1 px-1 py-0.5"
              data-testid="conversation-row-delete-confirm"
            >
              <Button variant="ghost" size="sm"
                type="button"
                onclick={() => { ondelete?.(); confirmingDelete = false; close(); }}
                class="inline-flex flex-1 items-center justify-center gap-1 rounded bg-destructive px-2 py-1 text-[11px] font-semibold text-white hover:bg-destructive/90"
                data-testid="conversation-row-delete-confirm-btn"
              >
                <Check size={11} /> {$t("common.confirm")}
              </Button>
              <Button variant="ghost" size="sm"
                type="button"
                onclick={() => { confirmingDelete = false; close(); }}
                class="inline-flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground"
                data-testid="conversation-row-delete-cancel-btn"
              >
                <X size={11} /> {$t("common.cancel")}
              </Button>
            </div>
          {:else}
            <ul class="flex flex-col gap-0.5" role="menu" data-testid="conversation-row-menu">
              {#if onconfigure}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { close(); onconfigure?.(); }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-foreground hover:bg-muted"
                    data-testid="conversation-row-menu-configure"
                  >
                    <Settings2 size={12} /> {$t("chat.conversation_configure")}
                  </button>
                </li>
              {/if}
              {#if onrename}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { close(); void startEdit(); }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-foreground hover:bg-muted"
                    data-testid="conversation-row-menu-rename"
                  >
                    <Edit3 size={12} /> {$t("chat.rename_session")}
                  </button>
                </li>
              {/if}
              {#if ondelete}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { confirmingDelete = true; }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-destructive hover:bg-destructive/10"
                    data-testid="conversation-row-menu-delete"
                  >
                    <Trash2 size={12} /> {$t("common.delete")}
                  </button>
                </li>
              {/if}
            </ul>
          {/if}
        {/snippet}
      </ActionMenu>
    </div>
  {/if}
</ListRow>
