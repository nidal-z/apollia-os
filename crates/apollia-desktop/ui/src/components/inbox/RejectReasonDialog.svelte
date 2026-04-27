<script lang="ts">
  /**
   * Shared reject-reason dialog.
   *
   * Enforces a 5–500 character reason with live counter and disabled submit.
   */
  import { MIN_REJECT_REASON as MIN_REASON, MAX_REJECT_REASON as MAX_REASON } from "./types";
  import { onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { push, pop, positionOf, stackDepth } from "$lib/stores/inbox/modalStack";

  interface Props {
    open: boolean;
    title?: string;
    submitting?: boolean;
    onconfirm: (reason: string) => void;
    onclose: () => void;
  }

  let { open, title, submitting = false, onconfirm, onclose }: Props = $props();

  let reason = $state("");
  const trimmed = $derived(reason.trim());
  const length = $derived(trimmed.length);
  const valid = $derived(length >= MIN_REASON && length <= MAX_REASON);
  const tooLong = $derived(length > MAX_REASON);

  const stackId = `reject-${Math.random().toString(36).slice(2)}`;

  $effect(() => {
    if (open) {
      push(stackId);
      reason = "";
    } else {
      pop(stackId);
    }
  });

  onDestroy(() => pop(stackId));

  function handleConfirm() {
    if (!valid || submitting) return;
    onconfirm(trimmed);
  }

  const position = $derived($stackDepth > 1 ? positionOf(stackId) : 0);
</script>

<Dialog
  {open}
  {onclose}
  title={title ?? $t("inbox.reject_title")}
  size="sm"
  data-testid="inbox-reject-dialog"
>
  {#if position > 0}
    <div class="mb-2 text-[11px] text-muted-foreground" data-testid="modal-stack-indicator">
      {$t("inbox.dialog_stack_indicator", { values: { current: position, total: $stackDepth } })}
    </div>
  {/if}
  <p class="mb-3 text-sm text-muted-foreground">{$t("inbox.reject_reason_help")}</p>
  <Textarea
    bind:value={reason}
    rows={4}
    placeholder={$t("inbox.reject_placeholder", { values: { min: MIN_REASON } })}
    aria-describedby="inbox-reject-error"
    aria-invalid={!valid && length > 0}
    data-testid="inbox-reject-reason"
  />
  <div class="mt-1 flex items-center justify-between text-[11px]">
    <span
      id="inbox-reject-error"
      class={tooLong ? "text-destructive" : "text-muted-foreground"}
      role={tooLong || (!valid && length > 0) ? "alert" : undefined}
    >
      {#if tooLong}
        {$t("inbox.reject_too_long", { values: { max: MAX_REASON } })}
      {:else if !valid && length > 0}
        {$t("inbox.reject_too_short", { values: { min: MIN_REASON } })}
      {:else}
        {$t("inbox.reject_help", { values: { min: MIN_REASON } })}
      {/if}
    </span>
    <span class="text-muted-foreground">{length} / {MAX_REASON}</span>
  </div>

  <div class="mt-4 flex justify-end gap-2">
    <Button variant="outline" onclick={onclose}>{$t("common.cancel")}</Button>
    <Button
      variant="destructive"
      onclick={handleConfirm}
      disabled={!valid || submitting}
      data-testid="inbox-reject-confirm"
    >
      {submitting ? $t("approvals.rejecting") : $t("inbox.confirm_reject")}
    </Button>
  </div>
</Dialog>
