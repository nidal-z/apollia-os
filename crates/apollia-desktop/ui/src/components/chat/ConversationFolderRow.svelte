<!--
  A folder of the conversation list: fold it, rename it (from its menu, or by
  a double click on its name when Settings > Chat allows it), remove it, and take a
  conversation dragged onto it. Removing a folder removes no conversation; the menu says
  so next to the action.
-->
<script module lang="ts">
  /** The i18n key of what the menu says under "Delete folder". */
  export function folderDeleteNoteKey(count: number): string {
    if (count === 0) return "chat.folders.delete_note_empty";
    return count === 1 ? "chat.folders.delete_note_one" : "chat.folders.delete_note_many";
  }
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronRight, Edit3, Folder, MoreHorizontal, Trash2 } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { doubleClickRenames } from "$lib/chat/chatBehaviours";
  import { FOLDER_NAME_MAX_CHARS, cleanFolderName } from "$lib/stores/chatFolders";

  interface Props {
    folderId: string;
    name: string;
    /** Conversations of the list the folder holds. */
    count: number;
    collapsed: boolean;
    /** Open on the name being edited: a folder that was just created. */
    renaming?: boolean;
    ontoggle: () => void;
    onrename: (name: string) => void;
    ondelete: () => void;
    /** A dragged conversation is held over the folder and would enter it. */
    dropTarget?: boolean;
    /** The name stopped being edited, kept or not. */
    onrenamed?: () => void;
    /** Whether a double click on the name renames the folder. */
    renameOnDoubleClick?: boolean;
  }

  let {
    folderId,
    name,
    count,
    collapsed,
    renaming = false,
    ontoggle,
    onrename,
    ondelete,
    dropTarget = false,
    onrenamed,
    renameOnDoubleClick = true,
  }: Props = $props();

  let menuOpen = $state(false);
  let editing = $state(false);
  let editValue = $state("");
  let editInput = $state<HTMLInputElement | undefined>(undefined);

  async function startEdit(): Promise<void> {
    editValue = name;
    editing = true;
    await tick();
    editInput?.focus();
    editInput?.select();
  }

  $effect(() => {
    if (renaming && !editing) void startEdit();
  });

  function commitEdit(): void {
    if (!editing) return;
    editing = false;
    const cleaned = cleanFolderName(editValue);
    if (cleaned !== null && cleaned !== name) onrename(cleaned);
    onrenamed?.();
  }

  function cancelEdit(): void {
    editing = false;
    onrenamed?.();
  }

  function onEditKeydown(ev: KeyboardEvent): void {
    ev.stopPropagation();
    if (ev.key === "Enter") {
      ev.preventDefault();
      commitEdit();
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      cancelEdit();
    }
  }
</script>

<div
  class="group flex items-center gap-1 px-2 py-1 transition-colors {dropTarget
    ? 'bg-primary/10'
    : 'hover:bg-muted/40'}"
  role="group"
  aria-label={name}
  data-session-drop={folderId}
  data-testid="chat-folder-row"
  data-folder-id={folderId}
>
  {#if editing}
    <span class="inline-flex h-6 w-6 shrink-0 items-center justify-center text-muted-foreground">
      <Folder size={13} strokeWidth={1.75} aria-hidden="true" />
    </span>
    <input
      bind:this={editInput}
      bind:value={editValue}
      onkeydown={onEditKeydown}
      onblur={commitEdit}
      type="text"
      maxlength={FOLDER_NAME_MAX_CHARS}
      aria-label={$t("chat.folders.name_label")}
      data-testid="chat-folder-rename-input"
      class="min-w-0 flex-1 rounded-sm border border-primary/40 bg-background px-1 py-0.5 text-code-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/40"
    />
  {:else}
    <button
      type="button"
      class="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 py-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-expanded={!collapsed}
      onclick={ontoggle}
      ondblclick={() => {
        if (doubleClickRenames(renameOnDoubleClick, true)) void startEdit();
      }}
      data-testid="chat-folder-toggle"
    >
      {#if collapsed}
        <ChevronRight size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" />
      {:else}
        <ChevronDown size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" />
      {/if}
      <Folder size={13} strokeWidth={1.75} class="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 flex-1 truncate text-code-sm font-medium text-foreground">{name}</span>
      <span class="shrink-0 text-micro-lg tabular-nums text-muted-foreground" data-testid="chat-folder-count">
        {count}
      </span>
    </button>
    <ActionMenu bind:open={menuOpen} align="end" class="w-60">
      {#snippet triggerSlot(props)}
        <button
          type="button"
          {...props}
          class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-muted/60 hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100"
          aria-label={$t("chat.folders.actions", { values: { name } })}
          data-testid="chat-folder-menu-button"
        >
          <MoreHorizontal size={12} />
        </button>
      {/snippet}
      {#snippet body({ close })}
        <ul class="flex flex-col gap-0.5" role="menu" data-testid="chat-folder-menu">
          <li role="none">
            <button
              type="button"
              role="menuitem"
              onclick={() => { close(); void startEdit(); }}
              class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground hover:bg-muted"
              data-testid="chat-folder-menu-rename"
            >
              <Edit3 size={12} /> {$t("chat.folders.rename")}
            </button>
          </li>
          <li role="separator" class="my-0.5 border-t border-border"></li>
          <li role="none">
            <button
              type="button"
              role="menuitem"
              onclick={() => { close(); ondelete(); }}
              class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-destructive hover:bg-destructive/10"
              data-testid="chat-folder-menu-delete"
            >
              <Trash2 size={12} /> {$t("chat.folders.delete")}
            </button>
            <p class="px-2 pb-1 pt-0.5 text-micro-lg text-muted-foreground" data-testid="chat-folder-delete-note">
              {$t(folderDeleteNoteKey(count), { values: { count } })}
            </p>
          </li>
        </ul>
      {/snippet}
    </ActionMenu>
  {/if}
</div>
