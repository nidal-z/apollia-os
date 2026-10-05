<!--
  The trash of the conversations, shown in place of the list: each removed
  conversation with when it was removed, "Restore" and "Delete permanently".
  Restoring puts the conversation back in the list with its messages; deleting
  permanently asks first, since nothing comes back from it.
-->
<script module lang="ts">
  import type { TrashedChatSession } from "$lib/ipc/chat";

  /** What the foot of the view says: where removed conversations go. */
  export function trashHintKey(trashOn: boolean): string {
    return trashOn ? "chat.trash.hint" : "chat.trash.hint_off";
  }

  /** The name a row shows: the title, else the agent, else the placeholder. */
  export function trashedTitle(session: TrashedChatSession, untitled: string): string {
    return session.title?.trim() || session.agent_name || untitled;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { ArrowLeft, RotateCcw, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import {
    chatTrashEnabled,
    eraseFromTrash,
    loadTrash,
    restoreFromTrash,
    trashedSessions,
    trashedWhen,
  } from "$lib/stores/chatTrash";

  interface Props {
    /** Return to the list of conversations. */
    onback: () => void;
    /** A conversation left the trash for the list. */
    onrestored: (sessionId: string) => void;
  }

  let { onback, onrestored }: Props = $props();

  let loaded = $state(false);
  let loadFailed = $state(false);
  let busyId = $state<string | null>(null);
  let erasing = $state<TrashedChatSession | null>(null);
  let failure = $state<string | null>(null);

  onMount(() => {
    void loadTrash().then((ok) => {
      loadFailed = !ok;
      loaded = true;
    });
  });

  function when(session: TrashedChatSession): string {
    const text = trashedWhen(session.deleted_at, new Date(), $locale ?? "fr");
    return $t(text.key, { values: text.values });
  }

  async function run(sessionId: string, action: () => Promise<void>): Promise<boolean> {
    busyId = sessionId;
    failure = null;
    try {
      await action();
      return true;
    } catch (err: unknown) {
      failure = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      busyId = null;
    }
  }

  async function restore(session: TrashedChatSession): Promise<void> {
    if (await run(session.id, () => restoreFromTrash(session.id))) onrestored(session.id);
  }

  async function eraseConfirmed(): Promise<void> {
    const session = erasing;
    erasing = null;
    if (session) await run(session.id, () => eraseFromTrash(session.id));
  }
</script>

<div class="flex min-h-0 flex-1 flex-col" data-testid="chat-trash">
  <div class="flex items-center gap-1.5 px-3.5 pb-2 pt-4">
    <button
      type="button"
      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={$t("chat.trash.back")}
      title={$t("chat.trash.back")}
      onclick={onback}
      data-testid="chat-trash-back"
    >
      <ArrowLeft size={14} strokeWidth={1.75} aria-hidden="true" />
    </button>
    <h2 class="text-heading-sm font-semibold tracking-[-0.2px] text-foreground">
      {$t("chat.trash.title")}
    </h2>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2" data-testid="chat-trash-list">
    {#if failure}
      <p
        class="mx-1 mb-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-caption text-destructive"
        role="alert"
        data-testid="chat-trash-error"
      >
        {$t("chat.trash.action_failed", { values: { error: failure } })}
      </p>
    {/if}
    {#if !loaded}
      <p class="px-4 py-6 text-center text-caption text-muted-foreground">
        {$t("common.loading")}
      </p>
    {:else if loadFailed}
      <p
        class="px-4 py-6 text-center text-caption text-muted-foreground"
        data-testid="chat-trash-load-failed"
      >
        {$t("chat.trash.load_failed")}
      </p>
    {:else if $trashedSessions.length === 0}
      <p
        class="px-4 py-6 text-center text-caption text-muted-foreground"
        data-testid="chat-trash-empty"
      >
        {$t("chat.trash.empty")}
      </p>
    {:else}
      <ul class="flex flex-col gap-1.5">
        {#each $trashedSessions as session (session.id)}
          <li
            class="rounded-lg border border-border bg-background px-3 py-2.5"
            data-testid="chat-trash-row"
            data-session-id={session.id}
          >
            <p class="truncate text-body-sm font-medium text-foreground">
              {trashedTitle(session, $t("chat.untitled_session"))}
            </p>
            <p class="mt-0.5 truncate text-caption text-muted-foreground first-letter:uppercase">
              {#if session.agent_name}{session.agent_name} · {/if}{when(session)}
            </p>
            <div class="mt-2 flex flex-wrap items-center gap-1.5">
              <Button
                variant="outline"
                size="sm"
                disabled={busyId === session.id}
                onclick={() => void restore(session)}
                data-testid="chat-trash-restore"
              >
                {#snippet icon()}<RotateCcw size={12} />{/snippet}
                {$t("chat.trash.restore")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                class="text-destructive hover:bg-destructive/10"
                disabled={busyId === session.id}
                onclick={() => (erasing = session)}
                data-testid="chat-trash-erase"
              >
                {#snippet icon()}<Trash2 size={12} />{/snippet}
                {$t("chat.trash.erase")}
              </Button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <p
    class="border-t border-border px-3.5 py-3 text-caption text-muted-foreground"
    data-testid="chat-trash-hint"
  >
    {$t(trashHintKey($chatTrashEnabled))}
  </p>
</div>

<ConfirmDialog
  open={erasing !== null}
  title={$t("chat.trash.erase_confirm_title")}
  message={$t("chat.trash.erase_confirm_message")}
  confirmLabel={$t("chat.trash.erase")}
  cancelLabel={$t("common.cancel")}
  onclose={() => (erasing = null)}
  onconfirm={() => void eraseConfirmed()}
  data-testid="chat-trash-erase-dialog"
/>
