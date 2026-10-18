<script lang="ts">
  /**
   * BatchItemList - the items of a batch, one compact row each, beside the
   * detail of the item open: its number, its input, its state as an icon and
   * how long it took or has been running; below, what its agent is doing, or
   * the start of its answer. Many rows fit, so a large batch reads at a glance.
   */
  import { t } from "svelte-i18n";
  import { Check, CircleDashed, Clock, Pause, ShieldQuestion, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import type { BatchItem } from "$lib/ipc/batches";
  import { parseTimestampMs } from "$lib/utils";
  import { awaitedAccess, entryOf, statusOf } from "./batchView";
  import { elapsedLabel, liveLineOf, secondsSince, type LiveStep } from "./liveSteps";

  interface Props {
    items: BatchItem[];
    selectedRank: number | null;
    onSelect: (item: BatchItem) => void;
    onOpenDecision: (item: BatchItem) => void;
    live?: ReadonlyMap<string, LiveStep>;
    now?: number;
  }

  let { items, selectedRank, onSelect, onOpenDecision, live = new Map(), now = Date.now() }: Props = $props();

  /** How long an item took, or has been running; empty before it ran. */
  function duration(item: BatchItem): string {
    const status = statusOf(item);
    if (status === "running" && item.updatedAt !== undefined) {
      return elapsedLabel(secondsSince(parseTimestampMs(item.updatedAt), now));
    }
    if (typeof item.durationMs === "number" && item.durationMs > 0) {
      return elapsedLabel(Math.round(item.durationMs / 1000));
    }
    return "";
  }

  /** The second line of a row: what the agent does, or the start of the result. */
  function detail(item: BatchItem): string {
    const status = statusOf(item);
    if (status === "running") {
      return liveLineOf(item, item.taskId === null ? undefined : live.get(item.taskId), now, $t);
    }
    if (status === "awaiting") {
      const access = awaitedAccess(item);
      return access === null
        ? $t("batch_view.output.awaiting")
        : $t("batch_view.output.awaiting_access", { values: { access } });
    }
    if (status === "failed") return $t("batch_view.list.failed");
    if (status === "canceled") return $t("batch_view.output.canceled");
    if (status === "completed") return (item.output ?? "").replace(/[#*_`>]/g, "").trim();
    return entryOf(item).summary;
  }
</script>

<ol class="m-0 list-none divide-y divide-border p-0" data-testid="batch-table">
  {#each items as item (item.rank)}
    {@const status = statusOf(item)}
    {@const selected = selectedRank === item.rank}
    <li
      class="relative {selected ? 'bg-primary/10' : 'hover:bg-muted/40'}"
      data-testid="batch-item"
      data-rank={item.rank}
      data-status={status}
    >
      <button
        type="button"
        class="grid w-full grid-cols-[2rem_minmax(0,1fr)_auto] items-start gap-x-2 px-3 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60"
        aria-pressed={selected}
        onclick={() => onSelect(item)}
        data-testid="batch-item-open"
      >
        <span class="pt-px text-caption tabular-nums text-muted-foreground">{item.rank + 1}</span>
        <span class="min-w-0">
          <span class="block truncate text-body-sm font-medium text-foreground">{entryOf(item).title}</span>
          <span
            class="block truncate text-caption {status === 'failed' ? 'text-danger-a11y' : 'text-muted-foreground'}"
            data-testid={status === "running" ? "batch-item-live" : "batch-item-output"}
          >
            {detail(item)}
          </span>
        </span>
        <span class="flex items-center gap-1.5 pt-px text-caption tabular-nums text-muted-foreground">
          {duration(item)}
          <span class="sr-only" data-testid="batch-item-status">{$t(`batch_view.item_status.${status}`)}</span>
          {#if status === "completed"}
            <Check size={14} strokeWidth={2} class="text-success" aria-hidden="true" />
          {:else if status === "failed"}
            <X size={14} strokeWidth={2} class="text-danger-a11y" aria-hidden="true" />
          {:else if status === "running"}
            <Spinner size={12} />
          {:else if status === "awaiting"}
            <ShieldQuestion size={14} strokeWidth={1.75} class="text-warning-a11y" aria-hidden="true" />
          {:else if status === "canceled"}
            <Pause size={14} strokeWidth={1.75} aria-hidden="true" />
          {:else if status === "pending"}
            <Clock size={13} strokeWidth={1.5} aria-hidden="true" />
          {:else}
            <CircleDashed size={13} strokeWidth={1.5} aria-hidden="true" />
          {/if}
        </span>
      </button>
      {#if status === "awaiting"}
        <div class="px-3 pb-2 pl-[2.75rem]">
          <Button
            variant="outline"
            size="sm"
            onclick={(event: MouseEvent) => {
              event.stopPropagation();
              onOpenDecision(item);
            }}
            data-testid="batch-item-decision"
          >
            {$t("batch_view.decision.open")}
          </Button>
        </div>
      {/if}
    </li>
  {/each}
</ol>
