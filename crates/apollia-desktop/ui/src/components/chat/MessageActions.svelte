<script lang="ts">
  /**
   * The row of actions under a message. Under an answer, in this order:
   * regenerate, continue (the last answer only), branch off, copy, edit,
   * delete. Under a user message: edit, delete.
   *
   * Regenerate and the edit of a user message come from props, as before. The
   * other actions come from the context the conversation sets; without it (a
   * bubble shown outside a conversation) they are not drawn.
   */
  import { getContext } from "svelte";
  import { t } from "svelte-i18n";
  import { ArrowRight, Check, Copy, GitFork, Pencil, RefreshCw, Trash2 } from "lucide-svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { ANSWER_ACTIONS, type AnswerActionsContext } from "$lib/chat/answerActions";
  import type { ChatMessageView } from "$lib/types";

  interface Props {
    message: ChatMessageView;
    /** What "copy" puts in the clipboard. */
    copyText: string;
    /** True while a turn is generating: every action that changes it waits. */
    busy?: boolean;
    /** Regenerate the answer; the button is absent without it. */
    onregenerate?: (messageId: string) => void;
    /** Open the inline editor of the bubble; the button is absent without it. */
    onstartedit?: () => void;
  }

  let { message, copyText, busy = false, onregenerate, onstartedit }: Props = $props();

  // A system or tool line of the thread is not a message the runtime acts on.
  const offered = getContext<AnswerActionsContext | undefined>(ANSWER_ACTIONS);
  const answers = $derived(
    message.role === "user" || message.role === "assistant" ? offered : undefined,
  );
  const isUser = $derived(message.role === "user");
  const hasText = $derived(!!message.content);
  const isLastAnswer = $derived(!!answers && answers.lastAnswerId === message.id);
  const continueRefusal = $derived(answers?.continueRefusal ?? null);
  const continueLabel = $derived(
    continueRefusal ? $t(continueRefusal) : $t("chat.continue_answer"),
  );

  let copied = $state(false);
  let confirmingDelete = $state(false);

  async function handleCopy(): Promise<void> {
    if (!copyText) return;
    try {
      await navigator.clipboard.writeText(copyText);
      copied = true;
      setTimeout(() => { copied = false; }, 1500);
    } catch {
      // clipboard API may not be available
    }
  }

  function handleContinue(): void {
    if (continueRefusal || busy) return;
    answers?.continueAnswer(message.id);
  }

  function confirmDelete(): void {
    confirmingDelete = false;
    answers?.remove(message.id);
  }
</script>

<div class="chat-actions" data-testid="chat-message-actions-{message.id}">
  {#if !isUser}
    {#if onregenerate && hasText}
      <button
        type="button"
        onclick={() => onregenerate?.(message.id)}
        disabled={busy}
        title={$t("chat.regenerate")}
        aria-label={$t("chat.regenerate")}
        data-testid="chat-message-regenerate-{message.id}"
      >
        <RefreshCw size={15} />
      </button>
    {/if}
    {#if isLastAnswer}
      <!-- Disabled with its reason: `aria-disabled` keeps the button
           reachable, so the reason is read on hover and by a screen reader. -->
      <button
        type="button"
        onclick={handleContinue}
        aria-disabled={continueRefusal !== null || busy}
        title={continueLabel}
        aria-label={continueLabel}
        data-testid="chat-message-continue-{message.id}"
      >
        <ArrowRight size={15} />
      </button>
    {/if}
    {#if answers && hasText}
      <button
        type="button"
        onclick={() => answers?.fork(message.id)}
        title={$t("chat.fork")}
        aria-label={$t("chat.fork")}
        data-testid="chat-message-fork-{message.id}"
      >
        <GitFork size={15} />
      </button>
    {/if}
    {#if hasText}
      <button
        type="button"
        onclick={handleCopy}
        title={$t("chat.copy_message")}
        aria-label={$t("chat.copy_message")}
        data-testid="chat-message-copy-{message.id}"
      >
        {#if copied}
          <Check size={15} class="text-success" />
        {:else}
          <Copy size={15} />
        {/if}
      </button>
    {/if}
  {/if}
  {#if onstartedit && hasText}
    <button
      type="button"
      onclick={onstartedit}
      disabled={busy}
      title={$t("chat.edit_message")}
      aria-label={$t("chat.edit_message")}
      data-testid="chat-message-edit-{message.id}"
    >
      <Pencil size={15} />
    </button>
  {/if}
  {#if answers}
    <button
      type="button"
      onclick={() => (confirmingDelete = true)}
      disabled={busy}
      title={$t("chat.delete_message")}
      aria-label={$t("chat.delete_message")}
      data-testid="chat-message-delete-{message.id}"
    >
      <Trash2 size={15} />
    </button>
  {/if}
</div>

{#if answers}
  <ConfirmDialog
    open={confirmingDelete}
    onclose={() => (confirmingDelete = false)}
    onconfirm={confirmDelete}
    title={$t("chat.delete_message_title")}
    message={isUser ? $t("chat.delete_message_user_body") : $t("chat.delete_message_answer_body")}
    confirmLabel={$t("chat.delete_message")}
    data-testid="chat-message-delete-confirm-{message.id}"
  />
{/if}
