<script lang="ts">
  // Always-visible approval bar for the plan gate.
  //
  // Anchored below the (scrollable) step list so it is reachable in every
  // pending state, however long the plan. The Approve button carries the
  // signature gradient (the single gradient element of the card besides the
  // pico tile) and a Cmd+Enter hint; the secondary action opens a revision
  // (soft gate, never an abandon). Actions are callbacks; the parent owns the
  // IPC and keyboard wiring.
  import { t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** i18n key for the primary (approve) action label. */
    approveKey: string;
    /** i18n key for the secondary action label. */
    secondaryKey: string;
    /** Disables and spins the primary button during the IPC call. */
    isActing?: boolean;
    /** Approve handler. */
    onApprove: () => void;
    /** Secondary (request changes / reject) handler. */
    onSecondary: () => void;
    /** Single-key shortcut hint for the secondary action, e.g. "R". */
    secondaryKbd?: string | null;
    /** Optional edit entry (Builder). */
    onEdit?: (() => void) | null;
    /** i18n key for the revise hint shown at the end of the bar. */
    reviseHintKey?: string | null;
    /** Optional testids for the two primary actions. */
    approveTestid?: string;
    secondaryTestid?: string;
  }

  let {
    approveKey,
    secondaryKey,
    isActing = false,
    onApprove,
    onSecondary,
    secondaryKbd = null,
    onEdit = null,
    reviseHintKey = null,
    approveTestid,
    secondaryTestid,
  }: Props = $props();
</script>

<div
  class="mt-3 flex flex-wrap items-center gap-2 border-t border-border/60 pt-3"
>
  <Button
    variant="primary-gradient"
    size="sm"
    loading={isActing}
    disabled={isActing}
    onclick={onApprove}
    data-testid={approveTestid}
  >
    {#snippet icon()}
      <Check size={14} strokeWidth={2.4} />
    {/snippet}
    {$t(approveKey)}
    {#snippet kbd()}
      <kbd
        class="rounded border border-current px-1 font-mono text-overline opacity-60"
        aria-hidden="true">⌘⏎</kbd
      >
    {/snippet}
  </Button>

  <Button
    variant="outline"
    size="sm"
    disabled={isActing}
    onclick={onSecondary}
    data-testid={secondaryTestid}
  >
    {$t(secondaryKey)}
    {#if secondaryKbd}
      <kbd
        class="ml-2 rounded border border-current px-1 font-mono text-overline opacity-60"
        aria-hidden="true">{secondaryKbd}</kbd
      >
    {/if}
  </Button>

  {#if onEdit}
    <Button
      variant="ghost"
      size="sm"
      disabled={isActing}
      onclick={onEdit}
      data-testid="plan-review-edit"
    >
      {$t("plan_mode.edit_plan")}
    </Button>
  {/if}

  {#if reviseHintKey}
    <span class="ml-auto text-caption text-muted-foreground/80">
      {$t(reviseHintKey)}
    </span>
  {/if}
</div>
