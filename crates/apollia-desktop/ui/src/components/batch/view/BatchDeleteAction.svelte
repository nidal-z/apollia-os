<script lang="ts">
  /**
   * BatchDeleteAction - the command that deletes a batch, with its
   * confirmation. Disabled while the batch is active, the reason shown on
   * hover and given to assistive technologies as its description. The
   * confirmation says that the files already exported are kept: the deletion
   * removes the batch and its items from the history, nothing else. Compact, it is the icon at the end of a row of the history.
   */
  import { t } from "svelte-i18n";
  import { Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { deleteBatch, type BatchItem, type BatchSummary } from "$lib/ipc/batches";
  import { deleteBlockOf, type DeleteBlock } from "./batchDelete";

  interface Props {
    batch: BatchSummary;
    /** The items of the batch, when the screen has read them. */
    items?: readonly BatchItem[];
    /** The icon of a row instead of a labelled button. */
    compact?: boolean;
    /** Called once the batch is deleted. */
    ondeleted: (batchId: string) => void;
    "data-testid"?: string;
  }

  let { batch, items = [], compact = false, ondeleted, "data-testid": testid = "batch-delete" }: Props = $props();

  let confirming = $state(false);
  let deleting = $state(false);

  /** Why the batch cannot be deleted yet, said on the disabled button. */
  const BLOCK_KEY: Record<DeleteBlock, string> = {
    running: "batch_view.delete.blocked.running",
    paused: "batch_view.delete.blocked.paused",
    in_flight: "batch_view.delete.blocked.in_flight",
  };

  const block = $derived(deleteBlockOf(batch, items));
  const hint = $derived(block === null ? $t("batch_view.delete.label") : $t(BLOCK_KEY[block]));

  async function remove(): Promise<void> {
    if (deleting) return;
    deleting = true;
    try {
      await deleteBatch(batch.batchId);
      confirming = false;
      addToast($t("batch_view.delete.done"), "success", { "data-testid": "batch-deleted" });
      ondeleted(batch.batchId);
    } catch (err) {
      confirming = false;
      const reason = err instanceof Error ? err.message : String(err);
      addToast($t("batch_view.delete.failed", { values: { reason } }), "error");
    } finally {
      deleting = false;
    }
  }
</script>

<Tooltip content={hint} side={compact ? "left" : "bottom"}>
  {#snippet trigger(props)}
    <span {...props} class="inline-flex" data-testid={`${testid}-hint`}>
      <Button
        variant={compact ? "ghost" : "outline"}
        size={compact ? "icon-sm" : "sm"}
        class={block === null ? "" : "pointer-events-none"}
        disabled={block !== null || deleting}
        aria-label={compact ? hint : undefined}
        aria-describedby={block === null ? undefined : `${testid}-${batch.batchId}-reason`}
        onclick={() => (confirming = true)}
        data-testid={testid}
        data-blocked={block ?? ""}
      >
        {#if compact}
          <Trash2 size={14} strokeWidth={1.5} aria-hidden="true" />
        {:else}
          {$t("batch_view.delete.command")}
        {/if}
      </Button>
      {#if block !== null}
        <span id={`${testid}-${batch.batchId}-reason`} class="sr-only">{hint}</span>
      {/if}
    </span>
  {/snippet}
</Tooltip>

<ConfirmDialog
  open={confirming}
  onclose={() => (confirming = false)}
  onconfirm={() => void remove()}
  title={$t("batch_view.delete.title")}
  message={$t("batch_view.delete.body", { values: { count: batch.total } })}
  confirmLabel={$t("batch_view.delete.confirm")}
  cancelLabel={$t("batch_view.delete.cancel")}
  loading={deleting}
  data-testid={`${testid}-dialog`}
/>
