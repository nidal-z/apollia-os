<script lang="ts" module>
  import { isDestructiveKey } from "$lib/components/ui/dialog/destructiveGate";

  /** What the row's keyboard map can trigger. Neither entry destroys. */
  export interface FactRowKeyActions {
    startEdit: () => void;
    requestDelete: () => void;
  }

  /** The subset of `KeyboardEvent` the row's keyboard map reads. */
  export interface FactRowKeyEvent {
    key: string;
    preventDefault: () => void;
  }

  /**
   * The row's keyboard map: Enter edits, Delete and Backspace *request* a
   * deletion. The request opens a confirmation, it does not delete, which is
   * the whole point of routing the keystroke through here.
   */
  export function onFactRowKeydown(
    event: FactRowKeyEvent,
    editing: boolean,
    actions: FactRowKeyActions,
  ): void {
    if (editing) return;
    if (event.key === "Enter") {
      event.preventDefault();
      actions.startEdit();
      return;
    }
    if (isDestructiveKey(event.key)) {
      event.preventDefault();
      actions.requestDelete();
    }
  }
</script>

<script lang="ts">
  /**
   * ProfileFactRow - one memory fact: key + provenance + value, with inline
   * edit, hover actions, and a right-click context menu (edit / copy / delete).
   * Owns its own edit state; persistence hits the profile store immediately and
   * the parent list is updated through `onUpdated` / `onDeleted`.
   *
   * The three delete paths (keystroke, trash button, menu entry) all open the
   * same confirmation; only its confirm button reaches `deleteProfileEntry`.
   */
  import { t } from "svelte-i18n";
  import { Brain, Pencil, Trash2, Copy, Check, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import {
    cancelDestruction,
    commitDestruction,
    createDestructiveGate,
    requestDestruction,
  } from "$lib/components/ui/dialog/destructiveGate";
  import { reportError } from "$lib/errors/reportError";
  import { setProfileEntry, deleteProfileEntry, type ProfileEntryView } from "$lib/ipc/profile";
  import ProfileSourceBadge from "./ProfileSourceBadge.svelte";

  interface Props {
    fact: ProfileEntryView;
    onUpdated: (updated: ProfileEntryView) => void;
    onDeleted: (key: string) => void;
  }

  let { fact, onUpdated, onDeleted }: Props = $props();

  let editing = $state(false);
  let editValue = $state("");
  let busy = $state(false);
  let deleteGate = $state(createDestructiveGate());

  function startEdit(): void {
    editValue = fact.value;
    editing = true;
  }

  function cancelEdit(): void {
    editing = false;
    editValue = "";
  }

  async function saveEdit(): Promise<void> {
    const next = editValue.trim();
    if (!next || next === fact.value) {
      cancelEdit();
      return;
    }
    busy = true;
    try {
      onUpdated(await setProfileEntry(fact.key, next));
      cancelEdit();
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = false;
    }
  }

  function askRemove(): void {
    requestDestruction(deleteGate);
  }

  async function remove(): Promise<void> {
    busy = true;
    try {
      await deleteProfileEntry(fact.key);
      onDeleted(fact.key);
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = false;
    }
  }

  async function copyValue(): Promise<void> {
    try {
      await navigator.clipboard.writeText(fact.value);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  const menu = $derived<ActionMenuItem[]>([
    {
      id: "edit",
      label: $t("settings.profile.memory.edit"),
      icon: Pencil,
      onclick: startEdit,
    },
    {
      id: "copy",
      label: $t("settings.profile.memory.copy_value"),
      icon: Copy,
      onclick: () => void copyValue(),
    },
    {
      id: "delete",
      label: $t("common.delete"),
      icon: Trash2,
      variant: "destructive",
      onclick: askRemove,
    },
  ]);

  function onKeydown(e: KeyboardEvent): void {
    onFactRowKeydown(e, editing, { startEdit, requestDelete: askRemove });
  }
</script>

<ContextMenu items={menu} data-testid="profile-fact-context-menu">
  <div
    class="group flex items-start gap-3 rounded-lg px-2.5 py-2.5 text-left transition-colors hover:bg-surface-2/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
    tabindex={editing ? -1 : 0}
    role="button"
    onkeydown={onKeydown}
    data-testid="profile-fact"
  >
    <span
      class="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-md border border-border/70 bg-surface-2 text-muted-foreground"
    >
      <Brain size={13} strokeWidth={1.75} aria-hidden="true" />
    </span>

    <div class="min-w-0 flex-1">
      <div
        class="flex flex-wrap items-center gap-2 text-overline uppercase tracking-wide text-muted-foreground/80"
      >
        <span class="font-mono normal-case tracking-normal">{fact.key}</span>
        <ProfileSourceBadge source={fact.written_by} />
      </div>

      {#if editing}
        <div class="mt-1.5 flex items-center gap-2">
          <Input
            bind:value={editValue}
            aria-label={$t("settings.profile.memory.value_placeholder")}
            size="sm"
            class="flex-1"
            onkeydown={(e: KeyboardEvent) => {
              if (e.key === "Enter") void saveEdit();
              else if (e.key === "Escape") cancelEdit();
            }}
            data-testid="profile-fact-edit-input"
          />
          <Button
            variant="ghost"
            size="icon-sm"
            loading={busy}
            onclick={() => void saveEdit()}
            aria-label={$t("common.save")}
            data-testid="profile-fact-edit-save"
          >
            <Check size={14} strokeWidth={2} />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onclick={cancelEdit}
            aria-label={$t("common.cancel")}
            data-testid="profile-fact-edit-cancel"
          >
            <X size={14} strokeWidth={2} />
          </Button>
        </div>
      {:else}
        <p class="mt-0.5 break-words text-body-sm text-foreground">{fact.value}</p>
      {/if}
    </div>

    {#if !editing}
      <div
        class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
      >
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={startEdit}
          aria-label={$t("settings.profile.memory.edit")}
          data-testid="profile-fact-edit"
        >
          <Pencil size={14} strokeWidth={1.9} />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          loading={busy}
          onclick={askRemove}
          class="text-muted-foreground hover:text-destructive"
          aria-label={$t("common.delete")}
          data-testid="profile-fact-delete"
        >
          <Trash2 size={14} strokeWidth={1.9} />
        </Button>
      </div>
    {/if}
  </div>
</ContextMenu>

<ConfirmDialog
  open={deleteGate.open}
  onclose={() => cancelDestruction(deleteGate)}
  onconfirm={() => void commitDestruction(deleteGate, remove)}
  title={$t("settings.profile.memory.delete_dialog.title")}
  message={$t("settings.profile.memory.delete_dialog.message", {
    values: { key: fact.key },
  })}
  confirmLabel={$t("settings.profile.memory.delete_dialog.confirm")}
  cancelLabel={$t("common.cancel")}
  loading={busy}
  data-testid="profile-fact-delete-confirm"
/>
