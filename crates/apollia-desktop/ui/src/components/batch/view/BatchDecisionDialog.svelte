<script lang="ts">
  /**
   * BatchDecisionDialog - the decision a batch item waits on, opened from its
   * row: what the agent asks for, and the answers the decision takes. The
   * answer goes through the list of waiting decisions, so deciding here or on
   * the screen of the waiting decisions is the same decision.
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import type { BatchItem, BatchItemAnswer, BatchItemDecision } from "$lib/ipc/batches";
  import { entryOf, type BatchIpc } from "./batchView";

  interface Props {
    batchId: string;
    /** The waiting item, `null` while the dialog is closed. */
    item: BatchItem | null;
    ipc: BatchIpc;
    onclose: () => void;
    /** Called once an answer went through. */
    ondecided: () => void;
  }

  let { batchId, item, ipc, onclose, ondecided }: Props = $props();

  const ANSWERS: readonly BatchItemAnswer[] = ["once", "always", "refuse"];

  type Reading =
    | { status: "loading" }
    | { status: "ready"; decision: BatchItemDecision }
    | { status: "gone" }
    | { status: "error"; reason: string };

  let reading = $state<Reading>({ status: "loading" });
  let sending = $state<BatchItemAnswer | null>(null);
  let refusal = $state<string | null>(null);

  $effect(() => {
    const asked = item;
    if (asked === null) return;
    let stale = false;
    reading = { status: "loading" };
    refusal = null;
    ipc
      .decision(batchId, asked.rank)
      .then((decision) => {
        if (stale) return;
        reading = decision === null ? { status: "gone" } : { status: "ready", decision };
      })
      .catch((err: unknown) => {
        if (stale) return;
        reading = { status: "error", reason: err instanceof Error ? err.message : String(err) };
      });
    return () => {
      stale = true;
    };
  });

  const offered = $derived(
    reading.status === "ready" ? ANSWERS.filter((answer) => reading.status === "ready" && reading.decision.answers.includes(answer)) : [],
  );

  async function answer(choice: BatchItemAnswer): Promise<void> {
    if (reading.status !== "ready" || sending !== null) return;
    sending = choice;
    refusal = null;
    try {
      await ipc.decide(reading.decision.id, choice);
      ondecided();
      onclose();
    } catch (err) {
      refusal = err instanceof Error ? err.message : String(err);
    } finally {
      sending = null;
    }
  }
</script>

<Dialog open={item !== null} {onclose} size="md" title={$t("batch_view.decision.title")} data-testid="batch-decision-dialog">
  {#if item !== null}
    <p class="m-0 text-caption text-muted-foreground" data-testid="batch-decision-item">
      {$t("batch_view.decision.item", { values: { number: item.rank + 1, title: entryOf(item).title } })}
    </p>
    {#if reading.status === "loading"}
      <p class="m-0 mt-3 text-body-sm text-muted-foreground">{$t("common.loading")}</p>
    {:else if reading.status === "gone"}
      <p class="m-0 mt-3 text-body-sm text-foreground" data-testid="batch-decision-gone">
        {$t("batch_view.decision.gone")}
      </p>
    {:else if reading.status === "error"}
      <p class="m-0 mt-3 text-body-sm text-danger-a11y" data-testid="batch-decision-error">{reading.reason}</p>
    {:else}
      <p class="m-0 mt-3 text-body-sm text-foreground" data-testid="batch-decision-summary">
        {reading.decision.summary}
      </p>
      {#if reading.decision.access !== null}
        <p class="m-0 mt-2 text-caption text-muted-foreground" data-testid="batch-decision-access">
          {$t("batch_view.decision.access", { values: { access: reading.decision.access } })}
        </p>
      {/if}
      <p class="m-0 mt-2 text-caption text-muted-foreground">{$t("batch_view.decision.effect")}</p>
    {/if}
    {#if refusal !== null}
      <p class="m-0 mt-3 text-body-sm text-danger-a11y" role="alert" data-testid="batch-decision-refusal">{refusal}</p>
    {/if}
    <DialogFooter>
      <Button variant="ghost" size="sm" onclick={onclose} data-testid="batch-decision-close">
        {$t("batch_view.decision.later")}
      </Button>
      {#each offered as choice (choice)}
        <Button
          variant={choice === "refuse" ? "outline" : "primary-solid"}
          size="sm"
          disabled={sending !== null}
          loading={sending === choice}
          onclick={() => void answer(choice)}
          data-testid={`batch-decision-${choice}`}
        >
          {$t(`batch_view.decision.answer_${choice}`)}
        </Button>
      {/each}
    </DialogFooter>
  {/if}
</Dialog>
