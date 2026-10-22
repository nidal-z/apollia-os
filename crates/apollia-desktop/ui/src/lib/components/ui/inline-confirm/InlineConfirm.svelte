<script lang="ts">
  /**
   * InlineConfirm - a destructive gesture confirmed in the page, where it was
   * asked, rather than in a modal or a native dialog: what will happen, what
   * goes with it, why the last attempt failed, then Cancel and the confirming
   * action. Presentation only; the caller owns the gesture.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";

  interface Props {
    /** What confirming will do, in one sentence. */
    message: string;
    /** The word of the confirming action. */
    confirmLabel: string;
    /** The word shown while the gesture runs. */
    busyLabel?: string;
    /** The gesture runs: both actions wait. */
    busy?: boolean;
    /** Why the last attempt failed, if it did. */
    error?: string | null;
    /** What goes with the gesture, listed under the message. */
    children?: Snippet;
    onconfirm: () => void;
    oncancel: () => void;
    "data-testid"?: string;
  }

  let {
    message,
    confirmLabel,
    busyLabel,
    busy = false,
    error = null,
    children,
    onconfirm,
    oncancel,
    "data-testid": testid,
  }: Props = $props();
</script>

<Card class="space-y-3 border-destructive/30 bg-destructive/5 px-4 py-3.5" data-testid={testid}>
  <p class="text-body-sm text-foreground">{message}</p>
  {@render children?.()}
  {#if error}
    <p class="text-caption text-destructive" data-testid={testid ? `${testid}-error` : undefined}>{error}</p>
  {/if}
  <div class="flex gap-2">
    <Button
      variant="outline"
      size="sm"
      onclick={oncancel}
      disabled={busy}
      data-testid={testid ? `${testid}-cancel` : undefined}
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      size="sm"
      onclick={onconfirm}
      disabled={busy}
      data-testid={testid ? `${testid}-btn` : undefined}
    >
      {#if busy}<Spinner size={12} class="mr-1.5" />{busyLabel ?? confirmLabel}{:else}{confirmLabel}{/if}
    </Button>
  </div>
</Card>
