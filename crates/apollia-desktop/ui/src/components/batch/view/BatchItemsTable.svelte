<script lang="ts">
  /**
   * BatchItemsTable - the table of a batch, one row per item: its number, its
   * input, its status, its output or the cause of its failure, and the sources
   * its answer cites. A row opens the detail of its item; a waiting item
   * offers to open the decision it waits on.
   */
  import { t } from "svelte-i18n";
  import { BookOpen } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { BatchItem } from "$lib/ipc/batches";
  import { ITEM_TONE, awaitedAccess, citedSources, entryOf, statusOf } from "./batchView";

  interface Props {
    items: BatchItem[];
    /** The rank of the item whose detail is open, `null` when none is. */
    selectedRank: number | null;
    onSelect: (item: BatchItem) => void;
    onOpenDecision: (item: BatchItem) => void;
  }

  let { items, selectedRank, onSelect, onOpenDecision }: Props = $props();

  const HEADERS = ["number", "input", "status", "output", "sources"] as const;

  function output(item: BatchItem): string {
    const status = statusOf(item);
    if (status === "awaiting") {
      const access = awaitedAccess(item);
      return access === null
        ? $t("batch_view.output.awaiting")
        : $t("batch_view.output.awaiting_access", { values: { access } });
    }
    if (status === "pending") return $t("batch_view.output.pending");
    if (status === "running") return item.output ?? $t("batch_view.output.running");
    if (status === "canceled") return $t("batch_view.output.canceled");
    return item.output ?? "";
  }
</script>

<div class="overflow-x-auto rounded-lg border border-border bg-card" data-testid="batch-table">
  <table class="w-full table-fixed border-collapse text-left">
    <thead>
      <tr class="border-b border-border">
        {#each HEADERS as header (header)}
          <th
            scope="col"
            class="px-3 py-2 font-mono text-micro-lg font-semibold uppercase tracking-wider text-muted-foreground/70 {header === 'number' ? 'w-14' : ''} {header === 'status' ? 'w-48' : ''} {header === 'sources' ? 'w-52' : ''}"
          >
            {$t(`batch_view.column.${header}`)}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each items as item (item.rank)}
        {@const status = statusOf(item)}
        {@const entry = entryOf(item)}
        {@const cited = citedSources(item)}
        <tr
          class="cursor-pointer border-t border-border align-top transition-colors first:border-t-0 hover:bg-muted/40 {selectedRank === item.rank ? 'bg-primary/10' : ''}"
          onclick={() => onSelect(item)}
          data-testid="batch-item"
          data-rank={item.rank}
          data-status={status}
        >
          <td class="px-3 py-3 text-body-sm text-muted-foreground">{item.rank + 1}</td>
          <td class="px-3 py-3">
            <button
              type="button"
              class="block w-full break-words text-left text-body-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              aria-pressed={selectedRank === item.rank}
              onclick={(event) => {
                event.stopPropagation();
                onSelect(item);
              }}
              data-testid="batch-item-open"
            >
              {entry.title}
            </button>
            {#if entry.summary !== ""}
              <p class="m-0 mt-0.5 line-clamp-2 break-words text-caption text-muted-foreground">{entry.summary}</p>
            {/if}
          </td>
          <td class="px-3 py-3">
            <Badge variant={ITEM_TONE[status]} outline data-testid="batch-item-status">
              {$t(`batch_view.item_status.${status}`)}
            </Badge>
          </td>
          <td class="px-3 py-3">
            <p
              class="m-0 line-clamp-4 whitespace-pre-line break-words text-body-sm {status === 'failed' ? 'text-danger-a11y' : status === 'completed' ? 'text-foreground' : 'text-muted-foreground'}"
              data-testid="batch-item-output"
            >
              {output(item)}
            </p>
            {#if status === "awaiting"}
              <Button
                variant="outline"
                size="sm"
                class="mt-2"
                onclick={(event: MouseEvent) => {
                  event.stopPropagation();
                  onOpenDecision(item);
                }}
                data-testid="batch-item-decision"
              >
                {$t("batch_view.decision.open")}
              </Button>
            {/if}
          </td>
          <td class="px-3 py-3">
            <p class="m-0 flex items-start gap-1.5 break-words text-caption text-muted-foreground" data-testid="batch-item-sources">
              <BookOpen size={14} strokeWidth={1.5} class="mt-0.5 shrink-0" aria-hidden="true" />
              <span>{[item.origin, ...cited].filter((name) => name !== "").join(" · ")}</span>
            </p>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
